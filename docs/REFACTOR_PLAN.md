# Pixors — Refactor Plan

> Compreensive review of the codebase (sem `pixors-mcp`), seguido de plano detalhado de refatoração. Escrito a partir da leitura direta dos crates `pixors-engine`, `pixors-shader`, `pixors-image`, `pixors-ops`, `pixors-document` e `pixors-desktop` no branch `feature/phase9` em 2026-05-11.

Sumário rápido por categoria:

| # | Item | Severidade |
|---|------|-----------|
| B-01 | `CommitBlur` perde o valor arrastado (commit usa raio antigo) | **bug crítico** |
| B-02 | Ordem de composição CPU/GPU em `Compose` invertida | **bug crítico** |
| B-03 | `Effect::ToggleTransformEnabled` despacha mutação no-op → undo não restaura `enabled` | bug |
| B-04 | `Effect::ReorderTransforms` muta direto sem pushar `DocumentMutation` → undo não desfaz reorder | bug |
| B-05 | `Msg::FilterSearch::Apply` adiciona Blur(5.0) hardcoded ignorando filtro escolhido | bug |
| B-06 | `Dispatcher::run_graph` Apply mode trava o tab sem como destravar | bug latente |
| B-07 | `compose::Compose` retorna erro fatal em port fora de bounds — mata chain | robustez |
| B-08 | `run_blur_preview` percorre toda a imagem em vez do viewport visível | perf |
| B-09 | Produtores (ImageStreamSource) não checam cancelamento | UX |
| B-10 | Cache do mesmo arquivo aberto duas vezes compartilha `cache_dir` | corrupção possível |
| B-11 | `Camera::floor_mip` usa `MAX_TEX_DIM = 8192` hardcoded ao invés de `device.limits` | bug em GPUs antigas |
| S-01 | `controller.rs` 854 linhas faz roteamento, viewport bootstrap, blur, dialogs, ops de tile | estrutura |
| S-02 | `ViewportProgram::draw` muta cache (`take_new_img`, `set_active_mip`) num caminho de render | estrutura |
| S-03 | 4 `HashMap<TabId, _>` paralelos em `App` (tile_caches, viewport_states, mip_queues, …) | estrutura |
| S-04 | Construção de path de cache duplicada (`Tab::layer_cache_dir` vs `CompileCtx::layer_cache_dir`) | DRY |
| S-05 | Duas portas de entrada para mudar estado: `Action` × `DocumentMutation` × `Effect` | abstração |
| S-06 | `OpenFile` constrói graph cru em vez de usar `render::compiler::compile()` | DRY |
| S-07 | `TileCacheSink` registra callback global keyed por `u64` (roteador global) | abstração ruim |
| S-08 | `PathBuilder`/`graph::path::Path` mortos — só Export usa | dead |
| S-09 | `pixors-document::session::PreviewState` + `view::params::ParamValue` mortos | dead |
| S-10 | `develop::Adjustment` duplica `Operation` | abstração |
| S-11 | `BlendMode` re-exportado por `pixors-image` mas vive no `engine` | dependência invertida |
| S-12 | `PngEncoder` + `PngEncoderV2` coexistem; v2 buffera imagem inteira em RAM | redundância + perf |
| S-13 | `Stage` enum + 3 traits + `Box<dyn …>` triplicam definição de cada stage | abstração |
| S-14 | `Dispatcher` tem 3 entries (`dispatch`, `run_graph`, `mutate`) com lifecycle desalinhado | abstração |
| S-15 | Thread forwarder por pipeline só pra re-taggear eventos | desperdício |
| S-16 | `assign_devices` fixed-point com `max_iter` cap morto, two-pass bastaria | overengineering |
| S-17 | `Stage` cacheia `gpu_ctx` por mutação interna em vez de usar `ProcessorContext.gpu` | invariante quebrada |
| C-01 | Lock `.unwrap()` por toda parte → poisoning = crash | qualidade |
| C-02 | Magic numbers (256, 8192, 64, 32) espalhados | qualidade |
| C-03 | `TabView` em `Tab` é dead, real estado vive em `SessionState::view` | dead |
| C-04 | `error::Error` tem ~10 variantes mortas, todos chamam `Error::internal` | dead |
| C-05 | `Tab::title()` retorna `"untitled"` hardcoded | dead |
| C-06 | `panel/filter.rs` repete builders de row 3× (collapsed/expanded/disabled) | duplicação |
| C-07 | `CLAUDE.md`/`ARCHITECTURE.md` falam de `pixors-state` que virou `pixors-document` | docs |

Detalhes, decisão e plano abaixo.

---

## 1. Bugs

### B-01 — Slider de blur compromete o radius errado

**Sintoma**. User arrasta slider de blur, solta. Preview aparece, mas commit grava o radius antigo do transform.

**Causa**. `pixors-desktop/src/controller.rs:594` chama `self.filter_panel.update(&m)` *antes* do match. `FilterPanelState::update` resseta `dragging_radius = None` ao ver `Msg::CommitBlur` (`panel/filter.rs:65`). Depois, o braço `Msg::CommitBlur(_v)` faz `dragging_radius.take()` que retorna `None` e cai no fallback — o valor existente do transform. O `_v` recebido vem do slider em `panel/filter.rs:516` com `.on_release(Msg::CommitBlur(0.0))` — **hardcoded zero**. Os dois canais que poderiam carregar o valor (state e mensagem) são inutilizados.

**Fix**. Carregar o valor *na mensagem*. Slider já tem `state.dragging_radius` que reflete o último Drag — usar isso no `on_release`:

```rust
// panel/filter.rs build_filter_controls (Blur arm)
let current = r;  // r = blur_preview_radius.unwrap_or(*radius)
slider(1.0..=64.0, r, Msg::SetBlur)
    .width(Length::Fill)
    .step(0.5)
    .on_release(Msg::CommitBlur(current)),
```

Mais robusto: o slider chama `Msg::CommitBlur(r)` carregando o `r` corrente do view. No controller, usar `v` diretamente:

```rust
filters_panel::Msg::CommitBlur(v) => {
    self.blur_preview_radius = None;
    let radius = v;
    // … dispatch UpdateTransformOp / AddTransform with `radius`
}
```

E remover o `update()` reset de `dragging_radius` em `CommitBlur` — quem dita o valor passa a ser a mensagem.

---

### B-02 — Ordem de composição invertida

**Sintoma**. Stack de camadas: layer 0 (fundo) + layer 1 (topo). Render mostra layer 0 visível como se estivesse no topo, layer 1 escondida sob.

**Causa**. `render/compiler.rs:155-159` mapeia layer index `i` → port `(n - 1 - i)` e passa `blend_modes`/`opacities` invertidos (`.rev()`). Resultado:
- visible[0] (mais baixo) → port n-1
- visible[n-1] (mais alto) → port 0

GPU compose (`pixors-ops/src/processor/compose.rs:gpu_compose`) ordena por port asc e itera 0..n-1. Primeira iteração: port 0 = topo, sobre acumulador zerado. Segunda: port 1 = camada abaixo do topo, composta como `b` sobre `a=acc`. Continua até port n-1 = fundo composto por cima de todo o resto. **Fundo cobre tudo.**

CPU compose tem o mesmo defeito mas pior: itera tiles na ordem que `try_compose` produz (port asc), `alpha_over_f32(top=new_pixel, result)` aplica `new_pixel` SOBRE `result`. Igualmente invertido.

**Fix**. Inverter o mapeamento OU iterar do port mais alto pro mais baixo. Mais limpo: port = índice (bottom=0, top=n-1), iteração natural bottom→top:

```rust
// render/compiler.rs::compile_layer_stack
let compose = ctx.graph.add_stage(Stage::Processor(Box::new(Compose::new(
    n as u16,
    visible.iter().map(|l| l.blend.mode).collect(),       // sem .rev()
    visible.iter().map(|l| l.blend.opacity).collect(),    // sem .rev()
))));
for (i, layer) in visible.iter().enumerate() {
    let layer_out = compile_layer(layer, ctx);
    ctx.graph.add_edge(layer_out, compose, EdgePorts {
        from_port: 0,
        to_port: i as u16,           // port = index (bottom = 0)
    });
}
```

Adicionar teste integração (Compose 3 layers, alfas conhecidos, validar pixel central).

---

### B-03 — Toggle de transform.enabled não vai pro history

`controller.rs:791-819` (Effect::ToggleTransformEnabled): despacha `UpdateTransformOp { before: t.op.clone(), after: t.op.clone() }` (no-op) só pra registrar history, depois muta `transform.enabled` direto. Undo só restaura `op`, não `enabled`. Toggle é invisível ao undo stack.

**Fix**. Criar mutação dedicada:

```rust
// pixors-document/src/mutation/impls.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTransformEnabled {
    pub tab: TabId,
    pub layer: NodeId,
    pub transform_id: NodeId,
    pub before: bool,
    pub after: bool,
}
#[typetag::serde]
impl DocumentMutation for SetTransformEnabled {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && let Some(t) = l.transforms.iter_mut().find(|t| t.id == self.transform_id) {
            t.enabled = self.after;
        }
    }
    fn undo(&self, doc: &mut Document) { /* swap */ }
    fn label(&self) -> &str { if self.after { "Enable Filter" } else { "Disable Filter" } }
}
impl_document_action!(SetTransformEnabled, tab);
```

Effect passa a despachar essa mutação. Resync redraw via `QueueDisplayRefresh`.

---

### B-04 — Reorder de transforms não vai pro history

`controller.rs:820-836` (Effect::ReorderTransforms): muta `layer.transforms` swap direto, bumpa redraw_seq, recomposita. Nada de history.

**Fix**. Adicionar mutação `ReorderTransform { tab, layer, from, to }` análoga a `SwapLayers`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderTransform { pub tab: TabId, pub layer: NodeId, pub from: usize, pub to: usize }
#[typetag::serde]
impl DocumentMutation for ReorderTransform {
    fn apply(&self, doc: &mut Document) {
        if let Some(l) = doc.find_layer_mut(self.layer)
            && self.from < l.transforms.len() && self.to < l.transforms.len() {
            let t = l.transforms.remove(self.from);
            l.transforms.insert(self.to, t);
        }
    }
    fn undo(&self, doc: &mut Document) { /* inverse: remove at to, insert at from */ }
    fn label(&self) -> &str { "Reorder Filter" }
}
impl_document_action!(ReorderTransform, tab);
```

---

### B-05 — FilterSearch Apply hardcoda Blur(5.0)

`controller.rs:119-156`: ao aplicar um item do search, ignora `idx` e sempre adiciona `Operation::Blur { radius: 5.0 }`. Provável placeholder.

**Fix**. `FilterSearch::items[idx]` deve carregar a `Operation` template (e seu default). Trocar `.map(|t| t.clone())` por usar o item escolhido:

```rust
// modal/filter_search.rs deve expor: Item { op: Operation, label: &str }
let op = self.filter_search.items[idx].op.clone();
// e em controller:
op: op,
```

---

### B-06 — `run_graph` em modo Apply trava tab sem destrancar

`dispatcher.rs::run_graph` (action/mod.rs:242) seta `td.locked = true` em apply mode mas não insere nada em `active_apply_actions`. Quando o Done chega, `on_pipeline_done` itera os dois mapas e não acha nada — não destranca. Tab fica travada pra sempre.

Hoje só caminhos Background usam `run_graph` (viewport tiles), então não dispara. Latente. Remover Apply de `run_graph` ou consertar:

```rust
// se for usar Apply, registrar uma action sentinela ou track tab→true em map dedicado:
if is_apply && let Some(tid) = tab_id { self.no_action_apply_locks.insert(tid); }
// e em on_pipeline_done liberar
```

Recomendado: remover o param `mode` de `run_graph` e fixar Background — Apply sempre vai por `dispatch(Action)`.

---

### B-07 — `Compose` retorna erro hard em port fora de bounds

`compose.rs:70`: `if ctx.port >= self.layer_count { return Err(...) }`. Erro fatal → mata chain → pipeline aborta. Mas qualquer descompasso (e.g. recompile com nº de layers diferente) gera isso.

**Fix**. Logar e ignorar:

```rust
if ctx.port >= self.layer_count {
    tracing::warn!("Compose: port {} >= layer_count {}, dropping tile", ctx.port, self.layer_count);
    return Ok(());
}
```

---

### B-08 — `run_blur_preview` calcula a imagem inteira

`controller.rs:436-490`: `viewport = TileRange { 0..mip_w/TS, 0..mip_h/TS }` — toda a imagem no mip atual. Em imagem grande o preview é lento.

**Fix**. Usar `camera.padded_tile_range(mip, TILE_SIZE, 3)` como `run_mip_fetch` faz. Cuidado: blur precisa de tiles além da borda visível (radius). Adicionar padding extra igual a `radius / TILE_SIZE + 1`:

```rust
let extra_pad = (radius as u32).div_ceil(TILE_SIZE);
let viewport_range = vs.camera.padded_tile_range(mip, TILE_SIZE, 3 + extra_pad);
```

---

### B-09 — Cancelamento não atinge produtores

`ImageStreamSource::produce` itera scanlines via `decoder.open_stream(...)`. Não checa `cancelled` flag. Open de arquivo grande não para.

**Fix**. Passar `Arc<AtomicBool>` ao `ProcessorContext` (já existe em `ChainRunner` via `self.cancelled`), expor via `ctx.cancelled()`. Produtores checam dentro do loop de scanline:

```rust
// stage/context.rs
pub struct ProcessorContext<'a> {
    pub port: u16, pub device: Device, pub emit: &'a mut Emitter<Item>,
    pub gpu: Option<Arc<GpuContext>>,
    pub cancelled: Arc<AtomicBool>,
}
impl ProcessorContext<'_> {
    pub fn is_cancelled(&self) -> bool { self.cancelled.load(Ordering::Relaxed) }
}
// ImageStreamSource::produce loop:
while let Some(scanline) = self.stream.next() {
    if ctx.is_cancelled() { return Ok(()); }
    ctx.emit.emit(Item::ScanLine(scanline));
}
```

---

### B-10 — Cache compartilhado entre instâncias do mesmo path

`OpenFile::prepare` (open_file.rs:61): `let cache_dir = self.path.with_extension("pixors_cache");`. Mesmo arquivo aberto duas vezes (dois tabs) → mesmo dir → escritores concorrentes corrompem.

**Fix**. Compor o cache dir com tab_id ou hash do session:

```rust
let cache_dir = std::env::temp_dir()
    .join("pixors")
    .join(format!("tab_{:016x}", tab_id.0));
```

Limpar no `CloseTab::apply`.

---

### B-11 — `Camera::floor_mip` constante hardcoded 8192

`pixors-desktop/src/viewport/camera.rs:79`. iGPUs antigas têm max 4096; ainda menores em mobile. Vai dar `wgpu::ValidationError`.

**Fix**. Buscar `device.limits().max_texture_dimension_2d` no momento que GPU é inicializado, propagar pro Camera via `App`. Singleton OK:

```rust
// pixors-engine/src/gpu/context.rs
pub fn max_texture_dim() -> u32 {
    TARGET.get().map(|c| c.device.limits().max_texture_dimension_2d).unwrap_or(8192)
}
```

---

### B-12 (menor) — Forwarder thread duplica trabalho de tagging

`action/mod.rs:170-185` e `:265-278`: dois forwarder threads quase idênticos copiam events do pipeline pra broadcast, re-taggeando. Mas `Pipeline::compile` já recebe `tag` e produz events taggeados. Forwarder é redundante. Solução: pipeline broadcasta direto.

```rust
// Pipeline::compile recebe broadcast::Sender em vez de SyncSender:
pub fn compile(g: ExecGraph, events: Option<broadcast::Sender<PipelineEvent>>,
               cancelled: Arc<AtomicBool>, tag: u64) -> Result<Self, Error>;
```

Elimina ~50 linhas duplicadas e 1 thread por dispatch.

---

### B-13 — Memory blowup no `PngEncoderV2::finish`

`pixors-image/src/sink/png_encoder_v2.rs:88-106`: aloca `vec![0u8; iw * ih * bpp]` — imagem inteira em RAM, depois passa pro `PngEncoder::encode`. Image 10kx10k RGBA8 = 400 MB. Mesmo problema no `TiffEncoderStage`.

**Fix curto**. Documentar limite e abortar com erro acima de N pixels.
**Fix longo**. Trocar pra streaming PNG row-by-row: ordenar tiles por (py, px), escrever scanlines via `PngEncoder::start_streaming(...) → write_rows(...) → finish()`.

---

## 2. Estrutura

### S-01 — `controller.rs` é monolito (854 linhas)

Responsabilidades misturadas:
1. Roteamento `Msg → handler`
2. Inicialização de viewport por tab (`init_viewport_for_tab`)
3. Construção de pipelines de mip fetch e blur preview
4. Tick (drain `mip_queues`, atualizar `redraw_seq`)
5. Handlers de panels (layers, filter), menu, dialogs
6. Execução de Effects
7. Tile cache transparent fill

**Decisão**. Quebrar em módulos:

```
pixors-desktop/src/
├── controller/
│   ├── mod.rs           // App::update — só dispatch
│   ├── keyboard.rs      // handle_keyboard, atalhos
│   ├── pipeline.rs      // handle_pipeline_event, lock resync
│   ├── viewport_boot.rs // init_viewport_for_tab, recomposite_current_view
│   ├── tick.rs          // handle_tick, drain mip_queues
│   ├── filters.rs       // handle_filters_msg + blur preview pipelines
│   ├── layers.rs        // handle_layers_msg
│   ├── dialogs.rs       // export, ui_showcase, filter_search
│   └── effects.rs       // execute_effects
```

Cada sub-módulo é `impl App` em arquivo separado. Permitido em Rust via `mod controller; … impl App` em cada um.

---

### S-02 — `ViewportProgram::draw` muta estado

`viewport/program.rs:131-148`: dentro de `draw`, faz `cache.lock()` e `guard.take_new_img()`, `state.camera.fit()`, `state.current_mip = …`. Render é supostamente puro.

**Decisão**. Mover a lógica "new img detected → fit camera, choose mip" para o caminho de tick. ViewportProgram apenas lê camera/cache:

```rust
// app.rs handle_tick após processar mip_requests:
for tab in &state.tabs {
    if let Some(cache) = self.tile_caches.get(&tab.id) {
        if let Some((w, h)) = cache.lock().ok().and_then(|mut g| g.take_new_img()) {
            if let Some(vs) = self.viewport_states.get(&tab.id) {
                let mut s = vs.write().unwrap();
                s.camera.img_w = w as f32; s.camera.img_h = h as f32;
                if !s.user_interacted { s.camera.fit(); }
                s.current_mip = s.camera.visible_mip_level();
            }
        }
    }
}
```

`draw` passa a ser read-only — toma snapshot do camera/range e emite Primitive.

---

### S-03 — Quatro HashMaps paralelos por TabId

`App` mantém `tile_caches`, `viewport_states`, `mip_queues` e (no Dispatcher) `tabs: HashMap<TabId, TabDispatcher>`. Adicionar/remover tab requer touch em N maps.

**Decisão**. Consolidar:

```rust
// pixors-desktop/src/viewport/tab_state.rs
pub struct ViewportTab {
    pub cache: Arc<Mutex<TileCache>>,
    pub state: Arc<RwLock<ViewportState>>,
    pub mip_queue: Arc<Mutex<Vec<(TabId, u32, TileRange)>>>,
}

// App:
pub viewport_tabs: HashMap<TabId, ViewportTab>,
```

`tab_bar::Close` faz um único `viewport_tabs.remove(&id)`.

---

### S-04 — Path de cache duplicado

`pixors-document/src/tab.rs:29` (`Tab::layer_cache_dir`) e `pixors-document/src/render/compiler.rs:134` (`CompileCtx::layer_cache_dir`) constroem o mesmo `format!("layer_{:016x}", node_id.0)`.

**Decisão**. Extrair função pura:

```rust
// pixors-document/src/document/asset.rs (ou novo cache.rs)
pub fn layer_cache_dir(root: &Path, layer: NodeId) -> PathBuf {
    root.join(format!("layer_{:016x}", layer.0))
}
```

Usar em ambos os lados.

---

### S-05 — Três caminhos pra mudar estado

| Camada | Tipo | Uso |
|--------|------|-----|
| UI panel | `Effect` enum | retornado de `update(msg, ctx)` |
| Dispatcher | `Action` trait | recebido por `dispatcher.dispatch` |
| History | `DocumentMutation` trait + macro `impl_document_action!` | gera Action automática que pusha no history |

Resultado: adicionar uma mudança de estado requer 1) variant em Effect, 2) handler em `execute_effects`, 3) mutation `impl DocumentMutation`, 4) macro pra Action, 5) trigger de QueueDisplayRefresh. Cinco arquivos.

**Decisão**. Reduzir camadas:

```rust
// Unificar Effect em duas formas só:
pub enum Effect {
    Mutation(Arc<dyn DocumentMutation>),       // <-- substituí Dispatch(Action)
    Action(Arc<dyn Action>),                    // pipelines / actions não-history
    Ui(UiEffect),                               // TogglePane, ShowFilterSearch, PushError…
}
```

`Mutation(m)` em `execute_effects` faz `dispatch(Arc::new(MutationAction(m)))`, onde `MutationAction` é um wrapper único que pusha no history e emite refresh. Elimina a macro `impl_document_action!`. Cada DocumentMutation novo precisa só do `impl DocumentMutation for …`.

```rust
struct MutationAction(Arc<dyn DocumentMutation>);
impl Action for MutationAction {
    fn target_tab(&self) -> Option<TabId> { self.0.target_tab() }
    fn prepare(&self, _: &mut EditorState) -> Result<PreparedAction, String> {
        Ok(PreparedAction::StateOnly)
    }
    fn apply(&self, state: &mut EditorState, _: PipelineStatus) {
        if let Some(tab) = state.tab_mut(self.0.target_tab().unwrap()) {
            tab.history.push(self.0.clone(), &mut tab.document);
            tab.session.redraw_seq += 1;
        }
    }
    fn undo(&self, _: &mut EditorState) {}
    fn record_in_history(&self) -> bool { false }
}
```

`DocumentMutation` ganha `target_tab(&self) -> TabId`.

---

### S-06 — `OpenFile::prepare` constrói graph cru

`pixors-document/src/action/actions/open_file.rs:97-159` monta `ImageStreamSource → ScanLineToTile → ColorConvert → MipDownsample → CacheWriter` à mão por página. `render::compiler::compile` existe pra exatamente isso, mas só compila *o lado de display*. O lado de *ingest* (decode + cache write) está fora.

**Decisão**. Introduzir um segundo entrypoint no compiler:

```rust
// pixors-document/src/render/compiler.rs
pub fn compile_ingest(doc: &Document, image: &Image, config: &CompileConfig) -> ExecGraph {
    let mut g = ExecGraph::new();
    for layer in &doc.layers {
        let PixelSource::PrimaryAsset { page } = &layer.source else { continue };
        let cache_dir = layer_cache_dir(&config.cache_dir, layer.id);
        build_ingest_chain(&mut g, image, *page, cache_dir, config);
    }
    g
}
```

`OpenFile::prepare` chama `compile_ingest(&document, &img, &config)`. Documenta o pipeline em um único lugar.

---

### S-07 — `TileCacheSink` é roteador global por `u64`

`pixors-desktop/src/viewport/tile_cache_sink.rs:13` mantém `LazyLock<RwLock<Option<HashMap<u64, Arc<CacheCommitFn>>>>>`. Sink é construído com `routing_key: u64`, callback é registrada via `register_tile_cache(key, fn)`. Globalmente mutável, key arbitrária.

**Decisão**. Passar callback no construtor:

```rust
type CacheCommitFn = Arc<dyn Fn(u64, u32, u32, u32, u32, u32, u32, u32, &[u8]) + Send + Sync>;

pub struct TileCacheSink {
    pub generation: u64,
    pub callback: CacheCommitFn,
}
```

`init_viewport_for_tab` constrói sink com closure que captura `Arc<Mutex<TileCache>>`. Remove o roteador global e `install_router`/`register_tile_cache`/`unregister_tile_cache`. Cuidado: sink é serializado dentro de ExecGraph (pelo `Stage` enum) — não precisa ser `Clone` se passamos por `Box<dyn Consumer>`.

---

### S-08 — `PathBuilder`/`Path` mortos

`pixors-engine/src/graph/path_builder.rs` e `path.rs`: PathBuilder usado só em `Export::prepare`. `Path` (módulo `path.rs`) não é usado em lugar nenhum. CLAUDE.md indica `PathBuilder` como entry preferido, mas o compiler real é o de `render/compiler.rs` que constrói o ExecGraph diretamente.

**Decisão**. 
- Deletar `pixors-engine/src/graph/path.rs` (zero uso).
- Manter ou reescrever `PathBuilder` como DSL fluent que escreve `ExecGraph` mas que **suporte múltiplas entradas/saídas**. Estado atual aceita só cadeia linear → Compose (variable inputs) precisa do construtor cru. Plano: estender PathBuilder com `merge(other)` ou abandonar e usar `ExecGraph` direto em todos os lugares. Recomendado: abandonar; Export pode usar `ExecGraph` direto como `compile()` faz.

---

### S-09 — `PreviewState` e `ParamValue` mortos

`pixors-document/src/session.rs:9` (`PreviewState`) e `view/params.rs:5` (`ParamValue`) são definidos e re-exportados. Nenhum consumidor — `compile_preview` recebe `Operation` explicitamente, não usa `session.active_preview`. `UpdatePreview` action existe (preview.rs) mas nenhum lugar a despacha.

**Decisão**. Deletar `PreviewState`, `ParamValue`, e `actions/preview.rs`. Se preview por-parâmetro genérico voltar no roadmap, reintroduzir com cliente real.

---

### S-10 — `Adjustment` duplica `Operation`

`pixors-document/src/document/develop.rs:21` define `Adjustment::Blur` / `Adjustment::Exposure`. `transform::Operation` tem `Blur` / `Exposure`. Duas enums paralelos.

**Decisão**. Unificar: develop pre-stack usa o mesmo `Operation`. `DevelopAdjustment` vira `{ id: NodeId, op: Operation, enabled: bool }`. Compiler renderiza develop antes do layer stack.

---

### S-11 — `BlendMode` mora no lugar errado

`pixors-engine/src/common/blend.rs` define `BlendMode`. `pixors-image/src/image.rs:29` re-exporta `pub use pixors_engine::common::blend::BlendMode;`. `pixors-document::BlendSpec` consome `pixors_image::image::BlendMode` (open_file.rs:7) — então document depende de image só pelo re-export.

**Decisão**. Importar direto de `pixors_engine::common::blend::BlendMode` em document. Manter o re-export em image só por compatibilidade temporária (ou remover).

---

### S-12 — `PngEncoder` + `PngEncoderV2` coexistem

`pixors-image/src/sink/png_encoder.rs` (v1, batch full-image) e `png_encoder_v2.rs` (v2, recebe tiles, junta full-image, chama v1). Tiff também tem v1+stage.

**Decisão**. Reescrever encoder pra stream linha-a-linha (mantendo só "v2") usando os crates `png` / `tiff` em modo streaming. Renomear arquivo pra `png_encoder.rs` apenas. Ver B-13.

---

### S-13 — `Stage` enum + 3 traits triplicam definição

Cada stage tem que:
1. Definir struct
2. Implementar `Producer` ou `Processor` ou `Consumer`
3. Embrulhar em `Stage::Processor(Box::new(...))` no compilador

`Stage` enum + `Box<dyn …>` é dispatch dupla. Match no compile, vtable no run. Permite type-erased mas paga.

**Decisão (não bloqueante)**. Considerar unificação:

```rust
pub trait Stage: Send + Sync + Debug {
    fn kind(&self) -> &'static str;
    fn hints(&self) -> StageHints;
    fn role(&self) -> StageRole;          // Producer/Processor/Consumer
    fn input_ports(&self) -> PortGroup;
    fn output_ports(&self) -> PortGroup;
    fn run(&mut self, ctx: ProcessorContext, input: Option<Item>) -> Result<(), Error>;
}
```

`run` recebe `Option<Item>` (None → produce/finish). Apaga o enum, deixa só `Arc<dyn Stage>`. Compilador detecta role via método.

Trade-off: API mais uniforme, mas Producer não recebe Item, Consumer não emite. Pode confundir. **Decisão real**: deixar como está até que valha. Manter na lista pra reavaliar.

---

### S-14 — `Dispatcher` com 3 entries

`dispatch(action)`, `run_graph(graph, mode, tab)`, `mutate(state, f)`. `mutate` é só `f(state)`. `run_graph` é metade de `dispatch` (sem action lifecycle).

**Decisão**. Eliminar `mutate` (call sites podem chamar `state` direto). Renomear `run_graph` para `run_background_graph(graph, tab) -> Result<PipelineHandle, _>` e fixar Background. Apply sempre via `dispatch(Action)`. Reduz lifecycle pra um caminho coerente.

---

### S-15 — Forwarder thread por pipeline

Tratado em B-12.

---

### S-16 — `assign_devices` overengineered

`pipeline.rs:307-378`: fixed-point com `max_iter`. Grafo é DAG topologicamente ordenado — dá pra fazer em dois passes (toposort: Fixed→Either decisão; reverse toposort: backfill). O loop atual converge em 1-2 iter na prática.

**Decisão**. Substituir por implementação O(V+E):

```rust
fn assign_devices(g: &ExecGraph, order: &[StageId], gpu_ok: bool) -> HashMap<StageId, Device> {
    let mut devs = HashMap::new();
    // Pass 1: fixed
    for &id in order {
        match g.stage(id).hints().device {
            Device::Cpu => { devs.insert(id, Device::Cpu); }
            Device::Gpu => { devs.insert(id, if gpu_ok { Device::Gpu } else { Device::Cpu }); }
            Device::Either => {}
        }
    }
    // Pass 2: Either, prefer-match-neighbors
    for &id in order {
        if devs.contains_key(&id) { continue; }
        let h = g.stage(id).hints();
        // ... (mesma lógica, sem loop externo)
    }
    devs
}
```

Caller passa o `order` já calculado.

---

### S-17 — Stages cachando GPU context

`MipDownsample::gpu: Option<Arc<GpuContext>>` e `TileToNeighborhood::gpu_ctx: Option<Arc<GpuContext>>` são fields persistidos. Stage processa primeiro item, lê `ctx.gpu`, salva. Próximos items usam o cache. **Invariante violada**: stages não devem ter estado de contexto — `ProcessorContext` é authority.

**Decisão**. Remover esses fields. Cada `process()` lê `ctx.gpu` direto.

```rust
// MipDownsample::downsample_block(&mut self, block, emit, gpu_ctx: Option<&Arc<GpuContext>>)
// chamado com ctx.gpu.as_ref()
```

Ganho: stage stateless quanto a GPU, testável sem singleton.

---

## 3. Código — qualidade e detalhes

### C-01 — `.unwrap()` em locks

`controller.rs`, `tile_cache.rs`, `viewport_state.rs` chamam `.lock().unwrap()` ou `.write().unwrap()` direto. Lock poisoning panica.

**Decisão**. Helper:

```rust
// pixors-desktop/src/util.rs
pub fn lock_or_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| {
        tracing::warn!("mutex poisoned, recovering");
        e.into_inner()
    })
}
```

Aplicar em todos sites de Mutex<TileCache>.

---

### C-02 — Magic numbers

| Constante | Local | Onde usar |
|-----------|-------|-----------|
| `256` (tile size) | pixors-document::TILE_SIZE — ok | já centralizado |
| `8192` (max tex) | viewport/camera.rs:79 | mover pra GPU init |
| `64` (channel bound) | runtime/runner.rs:5 | manter |
| `32` (batch size) | gpu/scheduler.rs:11 | manter |
| `5.0` (filter search default blur) | controller.rs:141 | bug B-05 |
| `0.95` (camera fit margin) | camera.rs:58 | const local com nome |

---

### C-03 — `Tab::TabView` duplicado

`pixors-document/src/tab.rs:21` define `TabView` mas `Tab` não tem campo desse tipo — TabView vive em `session.view`. Field `Tab::view` não existe; controller acessa `tab.session.view`. Tipo `TabView` exportado mas referenciado apenas por session.

**Decisão**. Mover `TabView` pra `session.rs`, deletar export de `tab.rs`. Atualizar `OpenFile::prepare` que monta `session.view = TabView { … }`.

---

### C-04 — `Error` variants dead

`pixors-engine/src/error.rs` lista 13 variants. Grep no projeto: `Error::InvalidDimensions`, `Error::UnsupportedSampleType`, `Error::ColorConversion`, `Error::AlphaOperation` etc nunca construídos via construtor (só `Error::internal` usado). Hard to maintain.

**Decisão**. Manter apenas variants usados: `Io`, `Png`, `Tiff`, `Internal`. Reduzir API.

---

### C-05 — `Tab::title()` hardcoded

`tab.rs:35`: `pub fn title(&self) -> &str { "untitled" }`. Caller real é `EditorState::push_tab` que computa title a partir do primary_path. Função dead.

**Decisão**. Deletar. Se UI precisar de title, expor via `EditorState::tab_title(id) -> String`.

---

### C-06 — `panel/filter.rs` repete builders

`build_collapsed_filter_row`, `build_expanded_filter_row`, `build_disabled_filter_row` reusam ~80% do código (grip + content_btn + actions). Diferenças: cor (full vs dim), border (none vs accent), corpo extra (controls vs nenhum).

**Decisão**. Função única com flags:

```rust
fn build_filter_row<'a>(idx: usize, num: &str, t: &'a Transform,
                        preview: Option<f32>, state: FilterRowState) -> Element<'a, Msg>
where state ∈ { Collapsed, Expanded, Disabled }
```

Calcula cor/border/extra controls dentro. Reduz ~200 linhas.

---

### C-07 — Docs out of date

`CLAUDE.md` linhas 18-30 falam de `pixors-state` (crate hoje é `pixors-document`). `ARCHITECTURE.md` linha 25 idem. `MEMORY.md` mantido pelo agente; mais externo, ok.

**Decisão**. Search-and-replace `pixors-state → pixors-document` em CLAUDE.md, ARCHITECTURE.md, AGENTS.md, REFACTOR_ROADMAP.md. Atualizar tabela "Key Files" pra novo layout.

---

## 4. Abstrações — visão de conjunto

### A camada de domínio

`pixors-document` mistura quatro responsabilidades:
1. **Modelo puro** — `Document`, `LayerNode`, `Transform`, `BlendSpec` (serde, immutable shape)
2. **Mutações reversíveis** — `DocumentMutation` trait + `impls`
3. **Editor state** — `EditorState`, `Tab`, `SessionState`, `History`
4. **Action runtime** — `Action`, `Dispatcher`, `PipelineMode`, `PipelineStatus`
5. **Render compilation** — `compile()`, `compile_preview()`, `CompileConfig`

Estes não têm o mesmo "tempo de vida": (1)(2) são serializáveis e persistem em disco; (3) é estado vivo de UI; (4) é runtime com threads e GPU; (5) depende de runtime (engine, ops).

**Decisão**. Quebrar `pixors-document` em 3 crates:

```
pixors-document   → (1)(2) puro, sem deps no engine além de tipos básicos (BlendMode, ColorSpace)
pixors-editor     → (3) EditorState, Tab, SessionState, History — depende de document
pixors-runtime    → (4)(5) Dispatcher, Action, render compiler — depende de editor + engine + ops
```

Permite testes de mutation sem GPU, MCP usa document+editor sem runtime.

---

### Action × Mutation × Effect — descrito em S-05

A trindade vai virar dupla: `Effect` na UI, `Action` no runtime. `DocumentMutation` continua sendo a unidade undo-able, mas o adapter pra `Action` deixa de ser macro e vira wrapper único `MutationAction`. Vide S-05.

---

### Ports e DataKind

`Stage` declara `input_ports`/`output_ports` com `PortGroup::Fixed(&[PortDeclaration])` ou `Variable(&PortDeclaration)`. Validação acontece em `validate_ports` (pipeline.rs:237). Para Variable, valida só índices máximos observados nas edges — sem upper bound.

**Decisão**. `PortGroup::Variable { decl: &PortDeclaration, max: Option<usize> }` pra Compose declarar `max: layer_count` e validar.

---

### Mensageria entre chains

`sync_channel<Option<RoutedItem>>(64)` — `None` sinaliza fim. Bound 64 fixo. `merge_inputs` faz fan-in com threads soltos.

**Decisão (deferida)**. R2 do roadmap já flagou: usar `std::thread::scope` no merge. Manter prioridade.

---

### Buffer

`Buffer::Cpu(Arc<Vec<u8>>)` — `Arc` permite share-no-copy, mas todo writer existente faz `.to_vec()` ou `Arc::try_unwrap`. Arc não economiza muita coisa hoje.

**Decisão**. Aceitar o overhead — quando `clone()` for legítimo (e.g., Compose recebendo tile de várias rotas), Arc paga. Não mudar.

---

### `Stage` enum vs trait

Vide S-13. Adiar.

---

## 5. Roadmap proposto, fases ordenadas

Lista executável em ordem. Cada fase deixa o repo verde (build+test).

### Fase R1 — Bugs críticos (1-2 dias)

- B-01 — Slider commit usa valor da mensagem (1h)
- B-02 — Compose layer order (2h + teste)
- B-03 — `SetTransformEnabled` mutation (1h)
- B-04 — `ReorderTransform` mutation (1h)
- B-05 — FilterSearch carrega Operation do item (2h)
- B-07 — Compose port soft-fail (15min)

### Fase R2 — Cleanup dead code (meio dia)

- S-08 deletar `graph/path.rs`
- S-09 deletar PreviewState, ParamValue, actions/preview.rs
- C-03 mover TabView
- C-04 enxugar Error variants
- C-05 deletar Tab::title
- C-07 atualizar docs

### Fase R3 — Estrutura: viewport + controller (1 dia)

- S-03 `ViewportTab` struct (consolidar 3 maps)
- S-02 mover side-effects de `draw` pra tick
- S-01 quebrar controller.rs em módulos `controller/*`

### Fase R4 — Renomear/unificar ingest + render (1 dia)

- S-06 `compile_ingest` em render/compiler
- OpenFile usa compile_ingest
- S-04 helper `layer_cache_dir`
- B-10 cache_dir por tab_id
- B-11 max_texture_dim de GPU limits

### Fase R5 — Dispatcher + eventos (1 dia)

- B-06 remover Apply de `run_graph` ou consertar
- B-12 broadcast direto do pipeline (elimina forwarder threads)
- S-14 dispatcher API enxuto (apenas dispatch + run_background_graph)
- S-15 / S-16 simplificar `assign_devices`

### Fase R6 — Sink/Source sem globals (1 dia)

- S-07 TileCacheSink com closure no construtor — apaga roteador global
- B-09 cancelamento em produtores

### Fase R7 — Effect/Mutation unificado (2 dias)

- S-05 `Effect::Mutation(Arc<dyn DocumentMutation>)` + MutationAction wrapper
- Remover macro `impl_document_action!`
- DocumentMutation ganha `target_tab(&self)`
- Refactor todos panels/effects

### Fase R8 — Crate split (2-3 dias)

- A camada de domínio → 3 crates (pixors-document, pixors-editor, pixors-runtime)
- Adjust workspace deps
- MCP atualiza

### Fase R9 — Encoders streaming (2-3 dias)

- B-13 / S-12 PngEncoder streaming, deletar v1
- TiffEncoder streaming

### Fase R10 — Qualidade (contínuo)

- C-01 helper lock_or_recover
- C-02 const nomeadas
- C-06 builder único de filter row
- S-17 stages stateless quanto a gpu_ctx
- S-10 unificar Adjustment/Operation
- S-11 BlendMode no engine
- B-08 blur preview com padded_tile_range

---

## 6. Decisões consolidadas

Cada decisão explicada acima é referenciada por ID. Em ordem de impacto:

**1. Camada de domínio em 3 crates** (S-05, S-14, R8)
Razão: separar serializável de runtime separa o testável do não-testável. MCP e CLI futuros usam apenas document+editor; desktop carrega runtime. Custo: refactor mecânico de imports.

**2. Eliminar globals (TileCacheSink router, install_router)** (S-07)
Razão: callbacks globais por u64 são acoplamento implícito invisível ao type system. Construtor com Arc<dyn Fn> resolve.

**3. Effect → Mutation** (S-05)
Razão: hoje cada mudança de estado toca 4-5 arquivos. Centralizar history-aware mutations num único wrapper deixa adicionar transform/layer mutations em 1 arquivo.

**4. Compose ordering fix + teste** (B-02)
Razão: bug visível ao usuário, ainda assim ninguém pegou — denuncia ausência de tests. Adicionar teste integração serve dois propósitos.

**5. Quebrar controller.rs** (S-01)
Razão: 854 linhas pra `App::update` é o sinal mais claro de que App acumula responsabilidades. Sub-módulos `impl App` permite split sem refactor de tipos.

**6. Streaming encoders** (B-13, S-12)
Razão: bloqueio real pra export de imagens > 100 MP. Suficientemente isolado pra entrar como fase única.

**7. Adiar:** unificação Stage trait (S-13), Arc<Vec<u8>> change (buffer). Custo > benefício hoje.

---

## 7. Métricas de sucesso

Antes/depois:
- LoC em `controller.rs`: 854 → < 200 por sub-módulo
- Quantos arquivos pra adicionar uma nova mutação: 5 → 2 (impls.rs + UI)
- `TabId → state` map count: 4 → 1 (`viewport_tabs`)
- Forwarder threads por pipeline: 1 → 0
- Bugs em `KNOWN_BUGS.md`: 4 → ≤ 1 (BUG-02 vira responsabilidade da R2)
- Tests: 0 → suite mínima (compose order, blur radius, mutation roundtrip)

---

## 8. Riscos

- **R8 (crate split)**: bom potencial pra ciclo de dependência. Plano: começar extraindo `pixors-editor` (deps: document+engine), depois `pixors-runtime` (deps: editor+ops). MCP fica em `pixors-runtime`.
- **B-02 fix**: pode invalidar exports passados (se alguém exportou com a ordem errada). Não é regression: comportamento atual está errado. Documentar em release notes.
- **S-07 (sink sem global)**: ExecGraph contém `Box<dyn Consumer>`; closures capturam Arc<Mutex<TileCache>>. ExecGraph não é Clone — confirmar que compile pipeline não tenta clonar.
- **R5 (broadcast direto)**: Pipeline passa a depender de `tokio::sync::broadcast`. Quebra pixors-engine como crate puramente sync. Solução: opt-in via feature flag `events` ou trait `EventSink`.

---

## 9. O que NÃO está no escopo

- `pixors-mcp` (ainda sem nada testável)
- Reescrita de shaders Slang
- Modelo de transform composta (`OutputMode::Composite` ainda é todo!())
- Sistema de máscaras (`Mask` é placeholder)
- ICC profile pipeline
- Drag-out / drag-in arquivo
- Async file dialog (já no roadmap, A6)

Esses ficam em PHASE_11+.
