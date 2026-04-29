# Job & Operation Modeling

> Working document — Phase 9 A2 design. Not final.

---

## 1. Two layers

```
Job        ── orquestrador: monta Pipeline(s), define Preview/Apply, gerencia
              progresso e cancelamento. É o que o usuário dispara.
  └── Operation  ── unidade tipada: transforma item de entrada I em zero ou
                    mais itens de saída O. Stateless ou stateful local.
                    Roda em thread gerenciada pelo runtime do Pipeline.
```

Source/Sink continuam como bordas — não viram Operation. Eles ligam o Pipeline ao mundo (disco, WorkingWriter, Viewport).

**Mata `Pipe`**. `Operation<In=Frame, Out=Frame>` cobre tudo que `Pipe` fazia.
**Adapter não é categoria separada** — adapter é só uma `Operation` que muda tipo (`Frame → Neighborhood`).

---

## 2. Trait `Operation`

```rust
/// Unidade de processamento tipada. Roda em thread gerenciada pelo runtime.
/// O runtime cuida de canais, cancelamento e progresso — Operation só transforma.
pub trait Operation: Send + 'static {
    type In:  Send + 'static;
    type Out: Send + 'static;

    fn name(&self) -> &'static str;
    fn cost(&self) -> f32 { 1.0 }

    /// Processa um item. Pode emitir 0..N saídas.
    /// Retorna Err para abortar todo o Job.
    fn process(
        &mut self,
        item: Self::In,
        emit: &mut dyn FnMut(Self::Out),
    ) -> Result<(), Error>;

    /// Chamado uma vez quando o stream upstream termina.
    /// Default: nada. Override para flush de buffers (neighborhood, batch, agg).
    fn finish(&mut self, _emit: &mut dyn FnMut(Self::Out)) -> Result<(), Error> {
        Ok(())
    }
}
```

### O que o runtime faz por você

- spawn da thread
- alocação dos canais (`mpsc::sync_channel(64)` — backpressure)
- check de `cancel` antes de cada `process`
- chamada de `on_item()` após cada `emit` (progresso)
- drenagem do canal de entrada e chamada de `finish`
- propagação de `Err` → cancela Job inteiro

### O que Operation NÃO faz

- não chama `std::thread::spawn`
- não conhece `mpsc::Receiver`
- não checa flag de cancel
- não conhece JobHandle, JobContext, eventos

Operation é puramente lógica de transformação. Isso deixa o trait simples e óbvio.

### Exemplos

```rust
// Mantém estado local (cache de tiles vizinhos), 1→0..N saídas.
pub struct NeighborhoodOp {
    grid: Arc<TileGrid>,
    radius: u32,
    cache: HashMap<(u32, u32), Arc<[u8]>>,
}

impl Operation for NeighborhoodOp {
    type In = Frame;
    type Out = Neighborhood;

    fn name(&self) -> &'static str { "neighborhood" }

    fn process(&mut self, frame: Frame, emit: &mut dyn FnMut(Neighborhood)) -> Result<(), Error> {
        let coord = frame.tile_coord();
        self.cache.insert((coord.tx, coord.ty), frame.data_arc());
        for nbhd in self.flush_complete_neighborhoods() {
            emit(nbhd);
        }
        Ok(())
    }

    fn finish(&mut self, emit: &mut dyn FnMut(Neighborhood)) -> Result<(), Error> {
        for nbhd in self.flush_remaining_with_clamp() { emit(nbhd); }
        Ok(())
    }
}

// Stateless puro, 1→1.
pub struct BlurOp { radius: u32 }

impl Operation for BlurOp {
    type In = Neighborhood;
    type Out = Frame;
    fn name(&self) -> &'static str { "blur" }
    fn cost(&self) -> f32 { self.radius as f32 * 0.5 }

    fn process(&mut self, nbhd: Neighborhood, emit: &mut dyn FnMut(Frame)) -> Result<(), Error> {
        emit(separable_box_blur(nbhd, self.radius));
        Ok(())
    }
}

// Single-shot (input acumulado vira 1 saída).
pub struct SamSegmentOp { model: Arc<Model> }

impl Operation for SamSegmentOp {
    type In = Image;
    type Out = Vec<Selection>;
    fn name(&self) -> &'static str { "sam" }
    fn cost(&self) -> f32 { 50.0 }

    fn process(&mut self, img: Image, emit: &mut dyn FnMut(Vec<Selection>)) -> Result<(), Error> {
        emit(self.model.segment(&img)?);
        Ok(())
    }
}
```

---

## 3. Dados entre Operations

Channel envia tipos `Send`. Para evitar cópia de buffers grandes:

- **Frame** já tem `data: Cow<'static, [u8]>`. Mantém. `Cow::Borrowed` para refs estáticas, `Cow::Owned` para tiles processados.
- **Acumuladores** (`NeighborhoodOp`, batchers) guardam `Arc<[u8]>` no estado. Pra emitir, clonam o `Arc` — refcount++, zero memcpy.
- **Mutação in-place** (BlurOp): recebe `Neighborhood { tiles: Vec<Arc<[u8]>> }`, escreve resultado em buffer novo (`Vec<u8>` → `Cow::Owned`). Os Arcs upstream são dropados quando refcount cai a 0.

Regra geral: **Operations passam por valor; o valor é barato (Arc/Cow); cópia de bytes só quando há mutação.**

---

## 4. Builder `Pipeline`

```rust
let pipeline = Pipeline::from(source)                  // TileSource → Pipeline<Frame>
    .then(NeighborhoodOp::new(grid, 4))                // Pipeline<Neighborhood>
    .par(BlurOp { radius: 4 }, 8)                      // 8 workers, Pipeline<Frame>
    .into(WorkingSink::new(writer));                   // Sink<Frame> → SealedPipeline
```

- `from(source)` — começa com `TileSource` (impl atual).
- `.then(op)` — adiciona Operation em 1 thread. Tipos casam estaticamente: `op.In == Pipeline::Item`.
- `.par(op, n)` — dispatcher round-robin + N workers. Exige `Op: Clone`. Cada worker tem sua cópia.
- `.fork(n)` — duplica stream em N consumidores (substitui `tee`).
- `.collect()` — coleta output em `Vec<Op::Out>`. Devolve `SealedCollectPipeline<T>`.
- `.into(sink)` — sela com destino externo. Devolve `SealedPipeline`.

Tipos errados = erro de compilação. **Não existe runtime type mismatch.**

### 4.1 `par(op, n)` e Clone — o que é clonado

`par` exige `Clone` porque o runtime cria N cópias da Operation — uma por worker — cada uma com seu `&mut self` isolado. **O que é clonado são os parâmetros de configuração, nunca os dados de processamento.**

| Operation | Campos | Clone custo | Shared state? |
|-----------|--------|-------------|---------------|
| `BlurOp { radius }` | `u32` | trivial | Não |
| `ColorConvertOp { conv }` | `ColorConversion` (lookup tables) | ~poucos KB | Não |
| `SamSegmentOp { model }` | `Arc<Model>` | refcount++ | Sim (Arc, read-only) |
| `MipOp { tile_size, levels }` | `u32, u32` | trivial | Não |

99% dos casos são stateless ou leves. Se houver estado mutável entre workers, usar `Arc<Mutex<State>>` — mas isso é exceção (raríssimo). O padrão é: Clone da Operation é clone de config, não de dados. Os dados (`Frame`, `Neighborhood`) fluem pelo canal, nunca são clonados pelo `par`.

---

## 5. Trait `Job`

```rust
/// Orquestrador de alto nível. Compõe Pipelines, executa Preview/Apply,
/// retorna handle observável.
pub trait Job: Send + Sync {
    fn name(&self) -> &str;

    /// Constrói e dispara o trabalho. Recebe Scope para variar entrada/destino.
    fn run(&self, ctx: &JobContext, scope: Scope) -> JobHandle;
}

pub enum Scope {
    /// Só MIP visível, sink temporário (auto-destroy). Cancela em mudança de zoom.
    Preview { mip_level: u32 },
    /// Todos os MIPs, sink real. Cancela só por ação explícita.
    Apply,
}

pub struct JobContext {
    pub tab_id: Uuid,
    pub tab: Arc<RwLock<TabData>>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<EngineEvent>,
}
```

`Job::run` é **uma só função**. Mata duplicação `preview()`/`execute()`. `Scope` decide source range, sink, política de cancelamento.

### 5.1 Job simples (Blur)

```rust
impl Job for BlurJob {
    fn name(&self) -> &str { "blur" }

    fn run(&self, ctx: &JobContext, scope: Scope) -> JobHandle {
        // Extrai refs do TabData e solta o lock
        let (layer, grid) = {
            let tab = ctx.tab.read();
            let layer = tab.find_layer(self.layer_id).clone();
            (layer, layer.tile_grid().clone())
        };

        let (source, sink) = match scope {
            Scope::Preview { mip_level } => (
                WorkingReaderSource::single_mip(&layer, mip_level),
                WorkingSink::temp(&ctx.tab),
            ),
            Scope::Apply => (
                WorkingReaderSource::all_mips(&layer),
                WorkingSink::persistent(&layer),
            ),
        };

        Pipeline::from(source)
            .then(NeighborhoodOp::new(grid, self.radius))
            .par(BlurOp { radius: self.radius }, 8)
            .into(sink)
            .start(ctx)
    }
}
```

### 5.2 Job multi-pipeline (Focus Stacking)

```rust
impl Job for FocusStackingJob {
    fn run(&self, ctx: &JobContext, scope: Scope) -> JobHandle {
        JobHandle::compose(ctx, |runner| {
            // Fase 1: N pipelines em paralelo, coleta resultado
            let ips: Vec<Vec<InterestPoints>> = runner.all_same(
                self.images.iter().map(|img| {
                    Pipeline::from(ImageFileSource::new(img))
                        .then(InterestPointsOp::new())
                        .collect()
                })
            )?;

            // Fase 2: cálculo síncrono curto
            let transforms = compute_ransac(&ips)?;

            // Fase 3: pipeline streaming final
            runner.stream(
                Pipeline::from(WorkingReaderSource::all_mips(&self.output_layer))
                    .par(PixelMergeOp::new(transforms), 8)
                    .into(WorkingSink::persistent(&self.output_layer))
            )
        })
    }
}
```

`runner.all_same` lança N `SealedCollectPipeline`s em paralelo, espera todos, retorna `Vec<Vec<T>>`. `runner.stream` roda 1 pipeline streaming com sink. Ambos respeitam cancel do `JobHandle`.

### 5.3 Primitivas de paralelismo

```rust
/// Future que coleta output de um Pipeline.
/// Spawn = thread roda. result() = bloqueia até terminar.
pub struct JobFuture<T> {
    handle: JoinHandle<Result<Vec<T>, Error>>,
    cancel: Arc<AtomicBool>,
}

impl<T: Send + 'static> JobFuture<T> {
    pub fn result(self) -> Result<Vec<T>, Error> {
        self.handle.join().unwrap()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
```

**Uso — mesmo tipo:** `all_same` (coleta e barreira implícita)

```rust
let futures: Vec<JobFuture<Vec<Frame>>> = sources
    .into_iter()
    .map(|s| Pipeline::from(s).then(op).collect().spawn(&cancel))
    .collect();
let results: Vec<Vec<Frame>> = all_same(futures)?;  // barreira + coleta
```

**Uso — tipos diferentes:** spawn + `.result()` individual (barreira implícita em cada `.result()`)

```rust
let a: JobFuture<Vec<Frame>> = blur_pipeline.collect().spawn(&cancel);
let b: JobFuture<Vec<Vec<Selection>>> = sam_pipeline.collect().spawn(&cancel);
// Ambos rodando em paralelo
let blur_result = a.result()?;   // bloqueia até terminar
let sam_result = b.result()?;    // já terminou ou termina logo
```

`all_same` é o caso comum (N imagens → mesmo processamento). Tipos diferentes são raros e se resolvem com `.result()` individual.

### 5.4 WorkingReaderSource

```rust
/// Lê tiles do WorkingWriter e os emite como stream de Frame.
/// Substitui ImageFileSource para operações que trabalham sobre pixels existentes.
pub struct WorkingReaderSource {
    pub writer: Arc<WorkingWriter>,
    pub scope: MipScope,
    pub tile_size: u32,
    pub generation: u64,
}

impl WorkingReaderSource {
    pub fn single_mip(layer: &LayerSlot, mip_level: u32) -> Self {
        let writer = if mip_level == 0 {
            Arc::clone(&layer.tile_store)
        } else {
            Arc::clone(&layer.mip_pyramid.level(mip_level).tile_store)
        };
        Self { writer, scope: MipScope::Preview { mip_level }, tile_size: writer.tile_size(), generation: 0 }
    }

    pub fn all_mips(layer: &LayerSlot) -> Self {
        Self { writer: Arc::clone(&layer.tile_store), scope: MipScope::Apply, tile_size: layer.tile_size, generation: 0 }
    }
}

impl TileSource for WorkingReaderSource {
    fn open(self) -> Result<mpsc::Receiver<Frame>, Error> {
        let grid = TileGrid::new(self.writer.image_width(), self.writer.image_height(), self.tile_size);
        let (tx, rx) = mpsc::sync_channel(64);

        std::thread::spawn(move || {
            let tile_coords = match self.scope {
                MipScope::Apply => grid.all_mip_tiles(),
                MipScope::Preview { mip_level } => grid.mip_tiles(mip_level),
            };
            let total = tile_coords.len() as u32;
            for (i, coord) in tile_coords.into_iter().enumerate() {
                if let Ok(Some(tile)) = self.writer.read_tile(coord) {
                    let frame = Frame::new(
                        FrameMeta { mip_level: coord.mip_level, total_tiles: total, .. },
                        FrameKind::Tile { coord },
                        Self::serialize_for_frame(&tile),
                    );
                    if tx.send(frame).is_err() { break; }
                }
                if i % 10 == 0 {
                    let _ = tx.send(Frame::progress(i as u32, total));
                }
            }
            let _ = tx.send(Frame::stream_done());
        });
        Ok(rx)
    }
}
```

---

## 6. Cancelamento

**Rust não mata threads.** Tudo cooperativo. Modelo:

```rust
pub struct JobHandle {
    pub id: Uuid,
    pub name: String,
    pub state: Arc<RwLock<JobState>>,
    pub progress: Arc<AtomicF32>,
    cancel: Arc<AtomicBool>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl JobHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn join(self) -> JobState { /* aguarda todas threads */ }
}
```

### Como o cancel propaga

Cada Operation roda dentro de um wrapper do runtime:

```rust
fn run_op_thread<O: Operation>(
    mut op: O,
    rx: Receiver<O::In>,
    tx: SyncSender<O::Out>,
    cancel: Arc<AtomicBool>,
    on_item: Arc<dyn Fn() + Send + Sync>,
) {
    let mut emit = |out| { let _ = tx.send(out); on_item(); };
    while let Ok(item) = rx.recv() {
        if cancel.load(Ordering::Relaxed) { break; }
        if op.process(item, &mut emit).is_err() {
            cancel.store(true, Ordering::Release);  // erro = cancela todo o Job
            break;
        }
    }
    // finish NÃO é chamado em cancel — output descartado
    // tx é dropado aqui → downstream vê canal fechado → propaga
}
```

**Propagação natural via drop de canal:**
1. `cancel()` seta flag.
2. Operation upstream sai do loop no próximo check, dropa `tx`.
3. Operation downstream recebe `Err` no `recv()`, sai, dropa seu `tx`.
4. Cascata até o Sink. Sink fecha. JoinHandles terminam.
5. `JobHandle::join()` desbloqueia.

**Granularidade:** check é por item. Item de blur dura ~ms — fino. Item de SAM dura segundos — grosso. Para Operations pesadas (SAM, RANSAC), injetar `Arc<AtomicBool>` via construtor permite check cooperativo interno:

```rust
pub struct SamSegmentOp {
    model: Arc<Model>,
    cancel: Arc<AtomicBool>,  // injetado pelo runtime se Operation impl WithCancel
}
// model.segment_with_check(&img, &self.cancel) faz check em mid-inferência
```

Runtime injeta a flag se Operation impl `WithCancel` (trait opcional). Default: sem injeção, check só entre itens.

### 6.1 Finish em cancel

Em cancel, **pular `finish`**. Cancel mata todo o estado do Job — output é descartado, cache invalida, framework lida com redo. `finish` só serve para flush de output válido em execução completa.

### 6.2 Erro em Operation

`process` retorna `Err(Error)`:
- runtime seta `cancel` (cascata acima)
- registra `Error` no JobHandle (`JobState::Failed(e)`)
- emite `JobEvent::Failed { job_id, error }`
- demais Operations terminam por canal fechado, naturalmente

### 6.3 Cancel por troca de Preview

`Job::Preview` ativo. Usuário move slider. Frontend dispara novo Preview. JobService:
1. Cancela Preview anterior (`handle.cancel()`).
2. Não bloqueia esperando — sink temp é descartado quando JoinHandles terminarem (refcount).
3. Spawna novo Preview imediatamente. Tem `generation` diferente — eventos do Preview velho são ignorados pelo Viewport (já tem isso hoje).

### 6.4 Threads "presas" em I/O

Reader de arquivo pode estar preso em `read()` no kernel. Cancel não tira ele de lá. Mitigação: I/O em chunks pequenos, check entre chunks. Aceito como limitação — pior caso espera o read terminar (~ms).

---

## 7. Mapeamento ao código atual

| Hoje (`stream/`)       | Depois                                       |
|------------------------|----------------------------------------------|
| `Pipe` (Frame→Frame)   | morre; vira `Operation<In=Frame, Out=Frame>` |
| `ParPipe`              | `Pipeline::par(op, n)`                       |
| `tee`                  | `Pipeline::fork(n)`                          |
| `TileSource`           | mantém — `Pipeline::from(src)`               |
| `TileSink`             | mantém — `Pipeline::into(sink)`              |
| `Frame`/`FrameKind`    | mantém                                       |
| `ColorConvertPipe`     | `ColorConvertOp: Operation<Frame, Frame>`    |
| `MipPipe`              | `MipOp: Operation<Frame, Frame>`             |
| `CompositePipe`        | `CompositeOp`                                |
| `ProgressSink`         | desnecessário — runtime emite progresso      |
| `operation/Brightness` | `BrightnessOp: Operation<Frame, Frame>`      |

Migração: introduzir `Operation`+`Pipeline` ao lado, reescrever loader, depois deprecar `Pipe`/`ParPipe`/`tee`.

---

## 8. Ciclo Frontend (Blur, com Preview)

```
1. Usuário seleciona layer → "Blur..."
2. <OperationDialog> abre (slider radius, toggle Preview, Apply/Cancel)

3. Preview ON, slider arrasta → debounce 150ms:
   ├─ dispatch { run_job, scope: Preview { mip }, op: blur(radius) }
   ├─ JobService: cancela Preview anterior, spawn novo
   ├─ BlurJob::run(ctx, Preview { mip })
   │   └─ Pipeline: WorkingReaderSource(mip único)
   │      → NeighborhoodOp → ParBlurOp → WorkingSink::temp
   ├─ tile pronto → PreviewEvent::TileReady { mip, coord, data }
   └─ Viewport sobrescreve tile na tela (não no disco)

4. Apply:
   ├─ dispatch { run_job, scope: Apply, op: blur(radius) }
   ├─ BlurJob::run(ctx, Apply)
   │   └─ Pipeline: WorkingReaderSource(todos mips)
   │      → NeighborhoodOp → ParBlurOp → WorkingSink::persistent
   ├─ JobEvent::Progress { job_id, percent }  (runtime calcula)
   └─ JobEvent::Done → Viewport invalida cache, recarrega
```

---

## 9. Componentes novos no frontend

| Componente             | Propósito                                                |
|------------------------|----------------------------------------------------------|
| `Dialog` / `Modal`     | Portal + backdrop                                        |
| `OperationDialog`      | Wrapper genérico: título, params, Apply/Cancel           |
| `SliderParam`          | Input range com label                                    |
| `ToggleParam`          | Checkbox/switch                                          |
| `usePreviewJob` hook   | Debounce + ciclo de Preview                              |
| Tipos `engine/types.ts`| `preview_tile_ready`, `preview_done`, `job_progress`, `job_done`, `job_failed`, `run_job` |

---

## 10. Service Integration

### 10.1 JobService

```rust
pub struct JobService {
    active_jobs: RwLock<HashMap<Uuid, Vec<JobHandle>>>,  // tab_id → handles
}

#[derive(Deserialize)]
pub struct JobCommand {
    pub tab_id: Uuid,
    pub scope: Scope,
    /// Serialized job spec: { kind: "blur", radius: 5, layer_id: "..." }
    pub job_spec: JobSpec,
}

impl JobService {
    pub async fn handle(&self, cmd: JobCommand, state: &AppState, ctx: &mut ConnectionContext) {
        match cmd {
            JobCommand::Run { tab_id, job_spec, scope } => {
                // 1. Deserialize job from spec
                let job: Box<dyn Job> = job_from_spec(&job_spec)?;

                // 2. Build JobContext
                let jctx = JobContext {
                    tab_id,
                    tab: state.session_manager.tab(tab_id)?,
                    event_tx: ctx.event_tx.clone(),
                };

                // 3. Run → JobHandle
                let handle = job.run(&jctx, scope);

                // 4. Register
                self.active_jobs.write().entry(tab_id).or_default().push(handle);

                // 5. Emit Started
                emit_event(JobEvent::Started { tab_id, job_id: handle.id, name: job.name() });

                // 6. Background monitor (spawns task to watch handle.state → emit Done/Failed)
                spawn_monitor(handle.clone(), ctx.event_tx.clone());
            }
            JobCommand::Cancel { job_id } => {
                if let Some(handle) = self.find_handle(job_id) {
                    handle.cancel();
                }
            }
        }
    }
}
```

### 10.2 Deserialization (job registry)

No boundary entre frontend e engine, precisamos mapear `job_spec.kind` → `Box<dyn Job>`:

```rust
/// Registry — match statement inevitável na borda de deserialização.
/// Cada Job novo adiciona um braço. O trait dá extensibilidade no comportamento,
/// o registry mapeia nomes para construtores.
pub fn job_from_spec(spec: &JobSpec) -> Result<Box<dyn Job>, Error> {
    match spec.kind.as_str() {
        "blur"         => Ok(Box::new(BlurJob::from_spec(spec)?)),
        "focus_stack"  => Ok(Box::new(FocusStackingJob::from_spec(spec)?)),
        "export_png"   => Ok(Box::new(ExportPngJob::from_spec(spec)?)),
        _ => Err(Error::invalid_param(format!("Unknown job: {}", spec.kind))),
    }
}

#[derive(Deserialize)]
pub struct JobSpec {
    pub kind: String,
    #[serde(flatten)]
    pub params: serde_json::Value,  // passado para Job::from_spec
}
```

A `match` statement é inevitável na borda — é um dispatch table, não uma enum de comportamento. O comportamento fica no trait.

### 10.3 Progresso

O runtime calcula progresso automaticamente: a cada `emit()` de uma Operation, `on_item()` é chamado. O `JobHandle` agrega:

```rust
progress = (items_processed as f32 / total_estimated_items as f32).clamp(0.0, 1.0)
```

O `total_estimated_items` vem de:
- `TileSource::total_tiles()` (já existe no `FrameMeta`)
- multiplicado pelos `cost()` de cada Operation no Pipeline
- `cost()` default `1.0` = 1 unidade por item

Para Jobs que não seguem o modelo item-a-item (SAM, RANSAC), o Job sobrescreve o cálculo de progresso via callback no `JobContext`. O `JobHandle` permite `set_progress(f32)` manual para esses casos.

### 10.4 Command routing

O `EngineCommand` ganha a variante `Job(JobCommand)`. O `route_command` em `app.rs` adiciona:

```rust
EngineCommand::Job(c) => state.job_service.handle(c, state, ctx).await,
```

`JobService` é registrado em `AppState` junto com os demais serviços do A1.

---

## 11. Decisões tomadas

1. **Job no topo, Operation na unidade.** Job orquestra; Operation transforma.
2. **Pipe morre.** `Operation<Frame, Frame>` cobre tudo.
3. **Adapter é só Operation com tipos diferentes.** Nada de categoria separada.
4. **Runtime gerencia threads/canais/cancel/progresso.** Operation só implementa `process()` + `finish()`.
5. **`emit` síncrono = backpressure.** Channel `bounded(64)`.
6. **`par(op, n)` exige `Clone`.** Clone é de config, não de dados. 99% dos casos: `Clone` trivial. Estado compartilhado via `Arc<Mutex<>>` é exceção raríssima.
7. **`Scope` decide Preview vs Apply** num `run()` único. Não duplica métodos.
8. **Cancel cooperativo via `AtomicBool` + drop de canal.** Cascata natural. `finish` não é chamado em cancel — output descartado, framework lida com redo.
9. **Erros em `process` cancelam todo o Job.** Estado `Failed(Error)`.
10. **`Cow`/`Arc` para zero-copy entre Operations.** Cópia só em mutação.
11. **Paralelismo: `all_same()` para mesmo tipo; `.result()` individual para tipos diferentes.** Interface mínima. `JobFuture` encapsula thread + resultado.
12. **`WorkingReaderSource` é o Source canônico para operações.** Lê tiles do WorkingWriter com `MipScope`. Implementa `TileSource`.
13. **Registry de deserialização é um `match` na borda.** Evitável? Não. Problemático? Também não. Cada Job novo = 1 linha.

---

## 12. Questões em aberto

1. **Lock de zoom durante Preview ativo?** Postergável. Preview velho é cancelado e ignorado (generation check).
2. **Layer com múltiplos Jobs simultâneos:** mutações na mesma layer serializadas (lock por layer). Loads de layers diferentes independentes.
3. **Undo após Apply:** fora do escopo Phase 9. `Operation` trait pronto pra receber undo buffer no futuro.
4. **`WithCancel` opcional para Operations longas:** trait separado ou param de construtor por convenção? Depende de quantas Operations precisarem.
5. **`WorkingReaderSource` emite progress frames** automaticamente (a cada 10 tiles). Isso conflita com o runtime que também quer emitir progresso? Resolver: Source emite `FrameKind::Progress` como antes; runtime traduz `Progress` frames em chamadas a `on_progress` do JobHandle (sem passar pela Operation).
