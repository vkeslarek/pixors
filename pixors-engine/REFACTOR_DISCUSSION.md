# Pipeline Refactor — Análise e Plano

> Audiência: implementador (humano ou modelo) que vai escrever o código.
> Objetivo: cortar boilerplate sem perder força. As abstrações que ficam
> precisam **carregar peso real** — semântica útil, garantias estáticas,
> mensagens de erro precisas, extensão limpa para GPU/MCP/multi-formato.

---

## 1. O que existe hoje (estado pré-refactor)

### 1.1 Camada de pixel (sólida)

| Item                | Local                          | Avaliação |
|---------------------|--------------------------------|-----------|
| `Component`         | `pixel/component.rs`           | Bom. `u8` / `u16` / `f16` / `f32` com `to_f32` / `from_f32_clamped`. |
| `Pixel` trait       | `pixel/mod.rs`                 | Bom. `unpack` / `pack_x4` / `pack_one` — ponte SIMD para o intermediário `[f32; 4]`. |
| `Rgba<T>` etc       | `pixel/rgba.rs`, `rgb.rs`, …   | `#[repr(C)]` + `bytemuck::Pod` — castável de `&[u8]` zero-copy. |
| `PixelFormat` enum  | `pixel/format.rs`              | Discriminador runtime. **Desconectado do `Pixel` trait** — não há ponte tipo↔enum. |
| `AlphaPolicy`       | `pixel/mod.rs`                 | Bom. Runtime, três variantes. |
| `PixelAccumulator`  | `pixel/accumulator.rs`         | Útil para blur/downsample. Implementado para `[u8;4]` e `Rgba<f16>`. |

A camada de pixel está **bem desenhada** — não preciso mexer. Falta só
plugar `PixelFormat` no `Pixel` trait via `const FORMAT`.

### 1.2 Camada de cor (madura)

`color/` tem `ColorSpace`, `RgbPrimaries`, `WhitePoint`, `TransferFn`,
`Matrix3x3`, `ColorConversion` (LUTs decode/encode + matriz),
`detect.rs` (PNG/TIFF chromaticity matching). É **funcional e testado**.
Único débito: `conversion.rs` tinha `use crate::image::meta::SampleFormat`
de um módulo deletado — limpei inline para `enum SampleFormat` local.

### 1.3 Camada de container (esquelética)

| Item              | Avaliação |
|-------------------|-----------|
| `Tile`            | Carregava só `coord + meta` — **não tinha bytes**. Já corrigi para `data: TileData::Cpu(Vec<u8>) \| Gpu(...)`. |
| `PixelMeta`       | `format + colorspace + alpha_policy`. Bom. |
| `Layer` / `Layers`| Existiam, sem dados. Removi até serem necessários. |
| `ScanLine`        | Sem dados. Removi. |
| `Neighborhood`    | Sem dados, lista de `TileCoord`. Removi até blur entrar. |
| `Image`           | Duplicava `Tile`. Removi. |
| `ContainerInstance` + `Buffer` enum | Indireção sem cliente. Removi; `TileData` substitui. |

### 1.4 Camada de pipeline (problemática — o foco do refactor)

#### Versão antes do meu primeiro corte (commit `9792c77`)

Quatro traits paralelos, **cada um com seu espelho `AnyXxx`**:

- `Source { type Output, run_cpu, finish_cpu }` + `AnySource`
- `Sink { type Input, consume_cpu, finish_cpu }` + `AnySink`
- `Operation { type Input, type Output, process_cpu, finish_cpu }` + `AnyOperation`
- `Converter { type Input, type Output, process_cpu, finish_cpu }` + `AnyConverter`

Cada `AnyXxx` (~70 linhas idênticas) dispatchava `&mut dyn FnMut(Box<dyn Any + Send>)` via downcast. **Quase 300 linhas de boilerplate** que escondiam a ideia de que **todo nó é a mesma coisa: input → output**.

Outros pontos ruins:
- `RunnerOptions { cpu, gpu, preferred, modify_in_place }` — capability como **dado runtime**, default `false/false` (todo nó nasce mentindo). Capability é estática: pertence ao sistema de tipos.
- `Emitter<T> { items: Vec<T> }` — wrapper trivial sobre `Vec::push`. **Mata streaming** (acumula tudo antes de entregar). É um closure `&mut dyn FnMut(T)` disfarçado.
- `executor::run` iterava arestas em ordem de inserção — não topo. Fan-out só funcionava por sorte. `finish` era chamado em ordem errada.
- `PathBuilder<T>` com `PhantomData` dava type-safety estática para a chain — bom, **mas** as duas únicas usadas reais (testes + JSON deserialização) são runtime. Custo (4 traits, 4 erasures) > benefício.
- `Tile` sem dados (item 1.3).
- Sem Source/Sink real — só struct-shells de `FileImageSource { path }` sem `produce` implementado.

#### Versão após meu primeiro corte (estado atual no working tree)

Cortei demais. O que escrevi é:

```rust
pub enum Item { Tile(Tile) }
pub enum ItemKind { None, Tile }

pub trait Node: Send {
    fn name(&self) -> &'static str;
    fn input(&self) -> ItemKind;
    fn output(&self) -> ItemKind;
    fn produce(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn run(&mut self, _: Item, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn finish(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
}
```

E `PngSource`/`PngSink` como `impl Node`. **O que ficou ruim:**

1. `let Item::Tile(tile) = item;` é refutável-mas-irrefutável-por-acidente. Quando entrar `Item::Layers`, ele deixa de compilar **silenciosamente** (warning) ou roda bagunça em runtime — depende de como Rust tratar. Pattern frágil.
2. **`ItemKind` é vazio.** Diz "Tile" mas não diz qual formato, qual color space, qual alpha. PngSink declara `input() = ItemKind::Tile`, aceita qualquer Tile, e só descobre na hora de escrever que o formato não cabe. Validação inútil.
3. **PngSource/PngSink não declaram contrato.** "Aceito Tile" — mas qual? Se um dia a engine ler PNG 16-bit, o output muda silenciosamente.
4. **`PixelFormat` ainda desconectado de `Pixel`.** Toda op nova vai ter que fazer `match format { Rgba8 => ..., ... }` na mão, repetindo 6 linhas, copiando lógica.
5. **Sem dispatch helper.** Op como `Exposure` (point-wise) é genérica em `P: Pixel`. Sem macro para roteador o autor escreve 6× o corpo.
6. **`FromItem` não existe.** Cada `run()` faz `let Item::Tile(t) = item` na mão — boilerplate + frágil.
7. **`Item::Tile.clone()` em fan-out copia `Vec<u8>` inteiro.** Aceitável para MVP, mas precisa de plano.
8. **`Cargo.toml` ainda tem `tiff = "0.11.3"`** sem cliente.

---

## 2. Princípios

Antes de listar abstrações, fixar critérios:

1. **Abstração só justifica seu peso se carregar semântica.** Wrapper que existe só para "bater" no Vec não vale. Wrapper que **garante** invariante (formato bate com tipo, color space é linear, residency é CPU) vale.
2. **Erros caem cedo, com hint.** Mismatch de formato/colorspace deve falhar em `validate()` (compile do grafo), não em `run()` (depois de decodar arquivo de 80 MB).
3. **Generics estáticos onde possível, runtime dispatch onde inevitável.** Pixel é runtime (formato vem do arquivo); ops são genéricas; ponte explícita via macro.
4. **GPU é cidadão de primeira classe.** Não é opcional, não é runtime hint, não é struct. É **trait**, declarada por `impl GpuNode for X`. Schedulee escolhe.
5. **MCP/JSON é contrato.** Cada nó publica schema (input port, output port, params). LLM monta grafo, valida antes de mandar.
6. **Sem associated types em traits dyn-safe na borda quente.** Type erasure só onde inevitável (canal entre nós heterogêneos no executor) — e mesmo lá é via `Item` enum, não `Box<dyn Any>`.

---

## 3. Abstrações propostas (a parte que importa)

### 3.1 Ponte tipo↔formato: `Pixel::FORMAT`

```rust
pub trait Pixel: Copy + Pod {
    const FORMAT: PixelFormat;
    fn unpack(self) -> [f32; 4];
    fn pack_one(rgba: [f32; 4], mode: AlphaPolicy) -> Self;
    fn pack_x4(...);
    fn unpack_x4(...);
}

impl Pixel for Rgba<u8>  { const FORMAT: PixelFormat = PixelFormat::Rgba8U; ... }
impl Pixel for Rgba<f16> { const FORMAT: PixelFormat = PixelFormat::Rgba16F; ... }
impl Pixel for Rgba<f32> { const FORMAT: PixelFormat = PixelFormat::Rgba32F; ... }
impl Pixel for Rgb<u8>   { const FORMAT: PixelFormat = PixelFormat::Rgb8U; ... }
// etc para todos os tipos concretos existentes
```

`PixelFormat` é estendido para cobrir todos os `Pixel` (atualmente: `Rgba8`,
`Rgba16U`, `Rgba16F`, `Rgba32F`, `Rgb8U`, `Rgb16U`, `Rgb16F`, `Rgb32F`,
`Gray8U`, `GrayAlpha8U`). Naming: `<Layout><Bits><Format>` onde `Format` é
`U` (unsigned int) ou `F` (float).

**Ganho:** runtime ↔ tipo é uma única linha. Sem possibilidade de
inconsistência.

### 3.2 Acesso tipado: `Tile::as_pixels<P>`

```rust
impl Tile {
    pub fn as_pixels<P: Pixel>(&self) -> Result<&[P], Error> {
        if self.meta.format != P::FORMAT {
            return Err(Error::format_mismatch(self.meta.format, P::FORMAT));
        }
        let bytes = self.cpu_bytes()
            .ok_or_else(|| Error::residency_mismatch("expected CPU"))?;
        Ok(bytemuck::cast_slice(bytes))
    }

    pub fn as_pixels_mut<P: Pixel>(&mut self) -> Result<&mut [P], Error> { ... }

    pub fn from_pixels<P: Pixel>(coord, color_space, alpha, pixels: Vec<P>) -> Self {
        let bytes = bytemuck::cast_slice(&pixels).to_vec();
        // ou cast_vec quando estável
        Self { coord, meta: PixelMeta::new(P::FORMAT, color_space, alpha), data: TileData::Cpu(bytes) }
    }
}
```

**Ganho:** op escreve `let pixels: &[Rgba<u8>] = tile.as_pixels()?;` e
recebe slice tipado. Mismatch é erro nomeado, não panic em
`bytemuck::cast` ou bug silencioso.

### 3.3 Dispatch genérico: macro `dispatch_pixel!`

```rust
#[macro_export]
macro_rules! dispatch_pixel {
    ($format:expr, $f:ident::<P>($($args:expr),*)) => {
        match $format {
            PixelFormat::Rgba8U   => $f::<Rgba<u8>>($($args),*),
            PixelFormat::Rgba16U  => $f::<Rgba<u16>>($($args),*),
            PixelFormat::Rgba16F  => $f::<Rgba<f16>>($($args),*),
            PixelFormat::Rgba32F  => $f::<Rgba<f32>>($($args),*),
            PixelFormat::Rgb8U    => $f::<Rgb<u8>>($($args),*),
            PixelFormat::Rgb16U   => $f::<Rgb<u16>>($($args),*),
            PixelFormat::Rgb16F   => $f::<Rgb<f16>>($($args),*),
            PixelFormat::Rgb32F   => $f::<Rgb<f32>>($($args),*),
            PixelFormat::Gray8U   => $f::<Gray<u8>>($($args),*),
            PixelFormat::GrayAlpha8U => $f::<GrayAlpha<u8>>($($args),*),
        }
    };
}
```

Variante adicional `dispatch_pixel_pair!` para ops com formato
src+dst (ex.: ColorConvert).

**Uso (op Exposure):**

```rust
fn run(&mut self, item, emit) -> Result<()> {
    let mut tile = Tile::from_item(item)?;
    fn apply<P: Pixel>(tile: &mut Tile, ev: f32) -> Result<()> {
        let pixels = tile.as_pixels_mut::<P>()?;
        for px in pixels {
            let [r, g, b, a] = px.unpack();
            let factor = 2f32.powf(ev);
            *px = P::pack_one([r * factor, g * factor, b * factor, a], AlphaPolicy::Straight);
        }
        Ok(())
    }
    dispatch_pixel!(tile.meta.format, apply::<P>(&mut tile, self.ev))?;
    emit(Item::Tile(tile));
    Ok(())
}
```

Op nova = uma fn genérica + uma chamada de macro. Sem 6× copy-paste.

**Restrição honesta:** alguns formatos podem não suportar uma operação
(ex.: convert para HDR só faz sentido em float). A op declara isso na
sua `PortSpec` (próxima abstração) e a macro só dispatcha o subset.
Variante `dispatch_pixel_subset!(format, [Rgba8U, Rgba16F], …)` cobre.

### 3.4 Contrato de porta: `PortSpec` substitui `ItemKind`

```rust
pub enum ContainerKind {
    None,            // source não tem input; sink não tem output
    Tile,
    Neighborhood,
    Layers,
    Layer,
    // ...
}

pub enum Constraint<T: PartialEq + Clone> {
    Any,
    Exact(T),
    OneOf(Vec<T>),
}

impl<T: PartialEq + Clone> Constraint<T> {
    pub fn fits(&self, expected_by_consumer: &Self) -> bool { ... }
    pub fn intersect(&self, other: &Self) -> Option<Self> { ... }
}

pub struct PortSpec {
    pub container: ContainerKind,
    pub pixel:      Constraint<PixelFormat>,
    pub colorspace: Constraint<ColorSpace>,
    pub alpha:      Constraint<AlphaPolicy>,
    pub residency:  Constraint<Residency>,   // Cpu agora; Gpu quando wgpu entrar
}

impl PortSpec {
    pub const fn none() -> Self { /* ContainerKind::None */ }

    pub fn fits(&self, consumer: &Self) -> Result<(), PortMismatch> {
        // checagem campo a campo, retorna primeiro erro útil
    }
}

pub struct PortMismatch {
    pub field: &'static str,         // "pixel", "colorspace", ...
    pub got: String,
    pub expected: String,
    pub hint: Option<String>,
}
```

**Trait `Node` redefine:**

```rust
pub trait Node: Send {
    fn name(&self) -> &'static str;
    fn input(&self) -> PortSpec;
    fn output(&self) -> PortSpec;
    fn params(&self) -> serde_json::Value { Value::Null }
    fn cache_key(&self) -> u64 { 0 }

    fn produce(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn run(&mut self, _: Item, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn finish(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
}
```

**PngSource declara honestamente:**

```rust
fn output(&self) -> PortSpec {
    PortSpec {
        container: ContainerKind::Tile,
        pixel: Constraint::OneOf(vec![
            PixelFormat::Rgba8U, PixelFormat::Rgb8U,
            PixelFormat::Gray8U, PixelFormat::GrayAlpha8U,
            PixelFormat::Rgba16U, PixelFormat::Rgb16U,    // PNG 16-bit
            PixelFormat::Gray16U, PixelFormat::GrayAlpha16U,
        ]),
        colorspace: Constraint::Exact(ColorSpace::SRGB),  // chunk cHRM/iCCP refina depois
        alpha: Constraint::Exact(AlphaPolicy::Straight),
        residency: Constraint::Exact(Residency::Cpu),
    }
}
```

**PngSink:**

```rust
fn input(&self) -> PortSpec {
    PortSpec {
        container: ContainerKind::Tile,
        pixel: Constraint::OneOf(vec![/* mesmos formatos PNG suporta */]),
        colorspace: Constraint::Any,   // grava embed cHRM se não-SRGB
        alpha: Constraint::OneOf(vec![AlphaPolicy::Straight, AlphaPolicy::OpaqueDrop]),
        residency: Constraint::Exact(Residency::Cpu),
    }
}
```

**Validação do grafo** em `Graph::validate()` chama `from.output().fits(&to.input())` para cada aresta. Erro fica:

```
edge n3 → n4: 'colorspace' got 'ACEScg', expected 'SRGB'
hint: insert a ColorConvert(target = SRGB) node between n3 and n4
```

**Ganho duro:**
- Erros pegam **antes** de I/O.
- LLM (MCP) lê schema da porta, monta grafo válido na primeira tentativa, ou recebe erro com hint para auto-corrigir.
- ColorConvert é uma porta declarando `output.colorspace = Exact(target)` — combinação válida emerge de tipos, não de doc humana.

### 3.5 Item dataflow + extração tipada

```rust
#[derive(Debug, Clone)]
pub enum Item {
    Tile(Tile),
    Neighborhood(Neighborhood),
    Layer(Layer),
    Layers(Layers),
    // expandido conforme containers entram
}

impl Item {
    pub fn container_kind(&self) -> ContainerKind { ... }
}

pub trait FromItem: Sized {
    fn from_item(item: Item) -> Result<Self, Error>;
    fn from_item_ref(item: &Item) -> Result<&Self, Error>;
}

impl FromItem for Tile {
    fn from_item(item: Item) -> Result<Self, Error> {
        match item {
            Item::Tile(t) => Ok(t),
            other => Err(Error::container_mismatch(
                ContainerKind::Tile, other.container_kind()
            )),
        }
    }
    fn from_item_ref(item: &Item) -> Result<&Self, Error> { ... }
}
// idem para Neighborhood, Layer, Layers
```

**Ganho:** `let tile = Tile::from_item(item)?;` é uma linha, com erro
nomeado. Quando `Item` ganhar variante, o `match` exaustivo no `FromItem`
quebra **compile** — não silencioso.

### 3.6 Fan-out clonável: `Item: Clone`

`Item` deriva `Clone`. Em `executor::route()`, fan-out faz
`item.clone()` para cada sucessor. Hoje `Tile.data: TileData::Cpu(Vec<u8>)`
clona. Quando virar gargalo:

```rust
pub enum TileData {
    Cpu(Arc<Vec<u8>>),
    Gpu(GpuHandle),
}
```

Arc faz fan-out grátis para readers; ops mut ainda clonam (CoW).
**Mudança contida** ao `TileData`. Resto do pipeline intocado.

### 3.7 Capability CPU/GPU como traits, não dados

```rust
pub trait Node: Send { /* base, sem produce/run/finish — só identidade e portas */ }

pub trait CpuNode: Node {
    fn produce(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn run(&mut self, _: Item, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
    fn finish(&mut self, _: &mut dyn FnMut(Item)) -> Result<(), Error> { Ok(()) }
}

pub trait GpuNode: Node {
    fn record(&mut self, ctx: &mut GpuCtx, item: Item, emit: &mut dyn FnMut(Item))
        -> Result<(), Error>;
    // produce_gpu / finish_gpu equivalentes
}
```

**Container de grafo:**

```rust
pub struct NodeBox {
    inner: Box<dyn Node>,                  // identidade + portas (sempre)
    cpu: Option<Box<dyn CpuDyn>>,          // dyn-safe view do CpuNode, ou None
    gpu: Option<Box<dyn GpuDyn>>,          // idem para GpuNode, None até wgpu
}

impl NodeBox {
    pub fn cpu<N: CpuNode + 'static>(n: N) -> Self {
        // Arc<Mutex<N>> compartilhado entre `inner` e `cpu` views.
        // Mutex é uncontested (executor sequencial por nó); custo negligível.
        // Quando GPU sumir, cai pra Box<N> direto.
    }
    pub fn gpu<N: GpuNode + 'static>(n: N) -> Self { ... }
    pub fn cpu_gpu<N: CpuNode + GpuNode + 'static>(n: N) -> Self { ... }
}
```

**Scheduler:**

```rust
fn pick_backend(node: &NodeBox) -> Backend {
    match (&node.cpu, &node.gpu) {
        (Some(_), Some(_)) => /* heurística: tamanho do tile, residency upstream */,
        (Some(_), None)    => Backend::Cpu,
        (None, Some(_))    => Backend::Gpu,
        (None, None)       => unreachable!("constructor garante pelo menos um"),
    }
}
```

**Por que beat `RunnerOptions`:**
- Capability é **bound de trait**. `NodeBox::cpu_gpu(BlurOp::new())` só compila se `BlurOp: CpuNode + GpuNode`. Sem como mentir.
- `produce/run/finish` (CPU) e `record` (GPU) têm assinaturas diferentes — refletindo a realidade. GPU precisa de `ctx` (queue, device, encoder), CPU não. Forçar mesma assinatura era ficção.
- Adicionar GPU = adicionar `impl GpuNode for X` e trocar constructor. Outros nós intocados.

**Trade-off documentado:** `Arc<Mutex<N>>` interno para casos `cpu_gpu`. Pra MVP só CPU, é `Box<N>` puro — sem custo. Quando virar `cpu_gpu`, `Mutex::lock()` é uncontested (executor processa um nó de cada vez por backend). Se mais tarde virar gargalo: split em duas instâncias clonadas, ops stateless aceitam de boa.

### 3.8 PortSpec + PixelMeta: garantia em runtime

Quando um `Tile` chega em `run()`, o nó **sabe** que `tile.meta.format`
está no `Constraint` declarado, porque o validator do grafo conferiu.
Ainda assim, `as_pixels::<P>()` confere de novo (defesa em profundidade,
custo zero — `if format != P::FORMAT { ... }`).

Em produção (`#[cfg(not(debug_assertions))]`) o compiler pode otimizar
o branch. Em debug, pega bug de uso.

### 3.9 Builder + Registry — runtime, não tipo-jogo

```rust
pub struct Builder {
    graph: Graph,
    last: Option<usize>,
}

impl Builder {
    pub fn source(self, n: NodeBox) -> Result<Self>;
    pub fn pipe(self, n: NodeBox) -> Result<Self>;     // valida fits()
    pub fn fork(self) -> ForkBuilder;                   // múltiplos sucessores
    pub fn build(self) -> Result<Graph>;
}

pub struct NodeRegistry {
    factories: HashMap<&'static str, fn(&serde_json::Value) -> Result<NodeBox>>,
    schemas: HashMap<&'static str, NodeSchema>,
}

pub struct NodeSchema {
    pub name: &'static str,
    pub input: PortSpec,
    pub output: PortSpec,
    pub params_schema: serde_json::Value,    // JSON Schema do params
    pub category: NodeCategory,              // Source/Sink/Operation/Aggregator
}
```

Sem `PathBuilder<T>` com PhantomData. Validação é runtime, mensagem é
informativa, MCP usa o mesmo path.

### 3.10 Executor — topo + streaming

```rust
pub fn run(graph: &mut Graph) -> Result<(), Error> {
    graph.validate()?;
    let order = topo_sort(graph)?;
    let mut pending: Vec<VecDeque<Item>> = vec![VecDeque::new(); graph.nodes.len()];

    for idx in order {
        // 1. Sources: produce
        if graph.input_edges_of(idx).is_empty() {
            let mut buf = Vec::new();
            graph.nodes[idx].produce(&mut |i| buf.push(i))?;
            route(idx, buf, graph, &mut pending)?;
        }

        // 2. Drain incoming queue, run() each
        while let Some(item) = pending[idx].pop_front() {
            let mut buf = Vec::new();
            graph.nodes[idx].run(item, &mut |i| buf.push(i))?;
            route(idx, buf, graph, &mut pending)?;
        }

        // 3. Finish (aggregators flush)
        let mut buf = Vec::new();
        graph.nodes[idx].finish(&mut |i| buf.push(i))?;
        route(idx, buf, graph, &mut pending)?;
    }
    Ok(())
}
```

Streaming real (cada item passa imediatamente para sucessores; não
acumula tudo de um nó antes de avançar). Fan-out: clone por sucessor.
`route` empilha em `pending[succ]` em vez de chamar `run` recursivamente
— evita borrow checker recursivo e mantém topo discipline.

**Versão com threads (futuro):** cada nó vira thread + `mpsc::sync_channel(64)` (modelo do doc original). Mesma topologia.

---

## 4. Layout de arquivos pós-refactor

```
src/
├── error.rs                               # + format_mismatch, container_mismatch, residency_mismatch helpers
├── lib.rs
├── pixel/
│   ├── mod.rs                             # + Pixel::FORMAT
│   ├── format.rs                          # PixelFormat ampliado
│   ├── component.rs (intocado)
│   ├── rgba.rs / rgb.rs / gray.rs / pack.rs (+ const FORMAT)
│   ├── accumulator.rs (intocado)
│   ├── xyz.rs (intocado)
│   └── dispatch.rs                        # NOVO — macros dispatch_pixel!
├── color/                                 # (intocado, exceto SampleFormat já corrigido)
├── container/
│   ├── mod.rs
│   ├── meta.rs                            # PixelMeta, Residency
│   └── tile.rs                            # + Tile::as_pixels<P>, from_pixels<P>
├── pipeline/
│   ├── mod.rs
│   ├── item.rs                            # Item, ContainerKind, FromItem
│   ├── port.rs                            # PortSpec, Constraint, PortMismatch
│   ├── node.rs                            # Node, CpuNode, GpuNode, NodeBox
│   ├── graph.rs                           # Graph, edges, validate
│   ├── executor.rs                        # topo + streaming + fan-out
│   ├── builder.rs                         # Builder runtime
│   ├── registry.rs                        # NodeRegistry, GraphSpec, NodeSchema
│   └── nodes/
│       ├── mod.rs
│       ├── png_source.rs                  # PortSpec real + dispatch_pixel
│       ├── png_sink.rs                    # idem
│       └── (futuro) tiff_source, color_convert, blur, exposure, ...
└── approx.rs                              # macro de teste — usado, manter
```

`storage/` permanece deletado. `image/` permanece deletado.

---

## 5. Ordem de implementação

1. **`Pixel::FORMAT` + `PixelFormat` ampliado.** Sem isso nada flui.
2. **`dispatch.rs` macros.** Simples, vale antes de qualquer node usar.
3. **`Residency` enum em `container/meta.rs`** (ou inline em Tile já — escolher um lugar).
4. **`Tile::as_pixels<P>` / `from_pixels<P>`.**
5. **`Item` + `ContainerKind` + `FromItem`** em `pipeline/item.rs`.
6. **`PortSpec` + `Constraint` + `PortMismatch`** em `pipeline/port.rs`.
7. **`Node` base + `CpuNode` + `GpuNode` (stub) + `NodeBox`** em `pipeline/node.rs`.
8. **`Graph::validate()`** usa PortSpec.fits.
9. **`executor::run`** topo + streaming.
10. **`Builder`** com `.pipe()` validando port fit.
11. **`NodeRegistry`** + `GraphSpec` JSON.
12. **`PngSource` real** (decode 8-bit e 16-bit, gray/rgb/alpha).
13. **`PngSink` real** (encode todos os formatos suportados).
14. **Teste integração:** roundtrip PNG → PngSource → PngSink → PNG, comparar bytes.
15. **Teste validate:** grafo com mismatch produz `PortMismatch` esperado.
16. **Teste fan-out:** Tee implícito (multi-edge) entrega item clonado a 2 sinks.

---

## 6. O que não entra agora (mas tem espaço reservado)

| Coisa            | Onde reserva espaço |
|------------------|---------------------|
| GPU runtime (wgpu) | `GpuNode` trait + `Residency::Gpu` + `NodeBox::gpu`/`cpu_gpu` constructors. Stub não roda hoje. |
| `Neighborhood`   | `ContainerKind::Neighborhood` + `Item::Neighborhood`. Construct quando blur entrar. |
| `Layers`         | Idem. PNG single-layer, TIFF multi-IFD usa. |
| `Tee` explícito  | Multi-edge resolve sem node especial. Adicionar `Tee` node identidade só se MCP/UI pedir. |
| Threads por nó (mpsc::sync_channel) | Executor MVP é single-thread topo. Refactor futuro mantém topologia, troca for-loop por threads. |
| Cache nodes (DiskCache/DisplayCache) | `NodeCategory::Cache` + `cache_key` no trait. Implementar na Phase 14. |
| Persistent graph (Arc-clone, Cow) | Quando undo/redo entrar. Hoje `Vec<NodeBox>` simples. |

---

## 7. Anti-padrões a evitar

- ✗ **`Box<dyn Any + Send>` no canal entre nós.** Use `Item` enum.
- ✗ **`AnyXxx` espelho por trait.** Um `Node` trait + `Item` resolve.
- ✗ **Capability como dado runtime (`RunnerOptions`).** Use traits.
- ✗ **`Emitter<T>` wrapper sobre `Vec`.** Use `&mut dyn FnMut(T)`.
- ✗ **Container sem dados (`Tile { coord, meta }`).** Tile carrega `TileData`.
- ✗ **`PixelFormat` desconectado de `Pixel` trait.** `const FORMAT` cola.
- ✗ **`PortSpec` ignorado em `validate()`.** Sempre checar `fits()`.
- ✗ **`let Item::Tile(t) = item` (irrefutável-por-acidente).** Use `Tile::from_item(item)?`.
- ✗ **`if let` exaustivo manual em ops.** Use `dispatch_pixel!`.
- ✗ **`PathBuilder<T>` com PhantomData.** Validação runtime via PortSpec é equivalente em segurança e melhor para MCP.

---

## 8. Métricas de sucesso

Após implementação, deve ser verdade que:

1. Trait `Node` cabe em < 30 linhas. `CpuNode`/`GpuNode` < 30 cada.
2. Adicionar uma op nova (point-wise) = uma fn genérica em `P: Pixel` + uma chamada `dispatch_pixel!` + declaração de `PortSpec`. ~30-50 linhas total.
3. Mensagem de erro de mismatch nomeia campo + got + expected + hint útil.
4. Teste roundtrip PNG passa para Rgba8U, Rgb8U, Gray8U, GrayAlpha8U.
5. Teste validate produz `PortMismatch` para combinação inválida.
6. Não existe `Box<dyn Any>` no código de pipeline.
7. Não existe trait `AnyXxx` no código de pipeline.
8. PortSpec serializa para JSON (introspecção MCP).
9. Adicionar `impl GpuNode for ExposureOp` é uma mudança contida — outros nós, executor topo, validation intocados.
