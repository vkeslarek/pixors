# Plano de Implementação: Pipeline MIP-Aware

## Visão Geral

Toda a pipeline de processamento de tiles passa a ser MIP-aware. Cada estágio
(source, operação, sink) recebe e propaga a informação de qual nível MIP está
sendo processado. O objetivo final é processamento batched com MIPs (salvo em
disco) e visualização interativa com tiles carregados sob demanda no MIP visível.

---

## 1. Tipos de Dados — `mip_level` nos Runtime Types

Os tipos do modelo (`model/image/tile.rs`, `model/image/neighborhood.rs`) já
possuem `mip_level`. O foco aqui são os tipos de runtime usados pela pipeline
(`pixors-executor/src/data/`).

### 1.1 `TileCoord` (`data/tile.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub mip_level: u32,   // NOVO — 0 = full-res, 1 = half, etc.
    pub tx: u32,
    pub ty: u32,
    pub px: u32,
    pub py: u32,
    pub width: u32,
    pub height: u32,
}

pub const DEFAULT_TILE_SIZE: u32 = 256;
```

**Construtor atualizado:**

```rust
pub fn new(
    mip_level: u32,
    tx: u32,
    ty: u32,
    tile_size: u32,
    image_width: u32,
    image_height: u32,
) -> Self { ... }
```

As coordenadas `px`/`py` são sempre calculadas como `tx * tile_size` /
`ty * tile_size`. A redução do MIP é implícita no número menor de tiles (pois
cada nível tem metade das colunas/linhas). O `mip_level` é puramente informativo
— ele identifica em qual nível da pirâmide o tile reside.

**Nota:** As dimensões da imagem (`image_width`, `image_height`) passadas ao
construtor são as dimensões NO NÍVEL MIP ATUAL (já divididas por `2^mip`),
não as dimensões do MIP 0. Isso garante que edge tiles sejam calculados
corretamente para cada nível.

### 1.2 `NeighborhoodCoord` (`data/neighborhood.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NeighborhoodCoord {
    pub mip_level: u32,   // NOVO
    pub tx: u32,
    pub ty: u32,
}
```

Usado como chave no cache de vizinhanças do `NeighborhoodAgg`. O `mip_level`
é necessário para diferenciar vizinhanças do mesmo (tx, ty) em diferentes MIPs.

### 1.3 `ScanLine` / `ScanLineCoord` (`data/scanline.rs`)

```rust
#[derive(Debug, Clone, Copy)]
pub struct ScanLineCoord {
    pub mip_level: u32,   // NOVO
    pub width: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub struct ScanLine {
    pub mip_level: u32,   // NOVO
    pub y: u32,
    pub width: u32,
    pub meta: PixelMeta,
    pub data: Buffer,
}
```

Fontes de arquivo (PNG, TIFF) sempre emitem scanlines com `mip_level = 0`.

### 1.4 `Neighborhood` (`data/neighborhood.rs`)

O `Neighborhood` NÃO precisa de campo explícito de `mip_level` porque o
`center: TileCoord` já o carrega. Nenhuma mudança estrutural necessária.

### 1.5 `TileBlock` / `TileBlockCoord` (`data/tile_block.rs`)

NOVO tipo de dados. Representa um bloco 2×2 de tiles completos, pronto para
downsampling. Emitido pelo `MipPyramid` e consumido pelo `MipCompose`.

```rust
/// Coordenadas de um bloco 2×2 no grid de tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileBlockCoord {
    pub mip_level: u32,
    pub tx_tl: u32,   // canto superior-esquerdo do bloco
    pub ty_tl: u32,
}

/// Bloco 2×2 completo. Tiles em ordem row-major:
///   [top-left, top-right, bottom-left, bottom-right]
#[derive(Debug, Clone)]
pub struct TileBlock {
    pub coord: TileBlockCoord,
    pub tiles: [Tile; 4],
}
```

**Motivação:** Separar a responsabilidade de acumulação (MipPyramid) da
responsabilidade de composição/downsampling (MipCompose). O MipPyramid
detecta blocos completos e emite `TileBlock`. O MipCompose recebe `TileBlock`,
faz o downsampling, e emite `Tile` no MIP N+1. Isso é mais modular e
componível do que colocar tudo no mesmo stage.

### 1.6 Propagação por todos os callers

Todo código que constrói `TileCoord`, `NeighborhoodCoord`, `ScanLine` ou
`ScanLineCoord` precisa ser atualizado para incluir `mip_level`. A lista
completa de callers:

| Arquivo | Objeto construído | Valor de `mip_level` |
|---------|-------------------|----------------------|
| `source/image_file_source.rs` | `ScanLine` | `0` |
| `source/file_decoder.rs` | `ScanLine` | `0` |
| `source/cache_reader.rs` | `TileCoord` (via leitura do disco) | O MIP que está sendo lido |
| `operation/data/to_tile.rs` | `TileCoord::new(...)` | Recebido do `ScanLine.mip_level` |
| `operation/data/to_neighborhood.rs` | `Neighborhood::new(...)` center | Do `tile.coord` (já tem mip_level) |
| `operation/data/to_neighborhood.rs` | `NeighborhoodCoord` na chave de cache | Do `tile.coord.mip_level` |
| `operation/data/to_scanline.rs` | `ScanLine::new(...)` | Do `tile.coord.mip_level` |
| `operation/blur.rs` | `Tile::new(...)` output | Do `nbhd.center.mip_level` |
| `operation/mip_pyramid.rs` | `TileBlock` (NOVO) | Do grid level |
| `operation/mip_compose.rs` | `Tile::new(...)` output (NOVO) | `tile_block.coord.mip_level + 1` |
| `operation/mip_filter.rs` | Pass-through | Mantido do tile de entrada |
| `sink/cache_writer.rs` | Gravação em disco | Do `tile.coord.mip_level` |
| `sink/viewport.rs` | `Origin3d` na textura | Propagado do tile |
| `sink/tile_sink.rs` | Callback `fn(px, py, ...)` | Adicionar `mip_level` ao callback |
| `runtime/cpu.rs` | Dummy tile para sources | `0` |

---

## 2. NeighborhoodAgg MIP-Aware

Mesma lógica do Blur: o `pixel_radius` efetivo escala com o MIP.

### 2.1 Cálculo do raio efetivo

```rust
fn effective_radius(&self, mip_level: u32) -> u32 {
    self.pixel_radius >> mip_level
}
```

Quando `effective_radius == 0`, o neighborhood contém apenas 1 tile (o centro).
O `tile_radius` (em unidades de tile) também é reduzido:

```rust
fn tile_radius(&self) -> u32 {
    if self.tile_size == 0 { return 0; }
    self.pixel_radius.div_ceil(self.tile_size)
}
```

Com MIP, `self.pixel_radius` nessa fórmula é o raio original (não reduzido),
porque o `tile_size` é sempre 256 e queremos coletar tiles suficientes para
cobrir `effective_radius` pixels. Na verdade, mantendo o raio original:
- Coletamos tiles vizinhos num raio fixo (em unidades de tile).
- O Blur é quem reduz o raio efetivo na hora de aplicar o box blur.
- Ter mais tiles no neighborhood do que o necessário não causa erro (só
  overhead de memória).

**Decisão:** Manter o `NeighborhoodAgg` coletando tiles com o raio original.
A redução efetiva é feita apenas no Blur (`radius >> mip_level`). Isso simplifica
o código e o overhead de tiles extras é pequeno (tiles são 256x256, mesmo em
MIPs altos o número de tiles extras é mínimo).

---

## 3. Blur MIP-Aware (`operation/blur.rs`)

O `Blur` recebe um `Neighborhood` e aplica box blur. Com MIP, o raio efetivo
é reduzido.

### 3.1 CPU (`BlurCpuRunner`)

```rust
fn process(&mut self, item: Item, emit: &mut Emitter<Item>) -> Result<(), Error> {
    let nbhd = match item {
        Item::Neighborhood(n) => n,
        _ => return Err(Error::internal("expected Neighborhood")),
    };
    let mip_level = nbhd.center.mip_level;
    let effective_radius = self.radius >> mip_level;

    if effective_radius == 0 {
        // No-op: copia o centro do neighborhood direto.
        // Precisa extrair a região central do buffer (sem blur).
        let cw = nbhd.center.width as usize;
        let ch = nbhd.center.height as usize;
        let bpp = 4;
        let mut tile_data = Vec::with_capacity(cw * ch * bpp);
        // Encontra o tile central e copia seus dados
        if let Some(center_tile) = nbhd.tile_at(nbhd.center.tx, nbhd.center.ty) {
            let data = center_tile.data.as_cpu_slice().unwrap();
            tile_data.extend_from_slice(data);
        }
        emit.emit(Item::Tile(Tile::new(nbhd.center, nbhd.meta, Buffer::cpu(tile_data))));
        return Ok(());
    }

    // ... resto do código existente usando effective_radius no lugar de self.radius
}
```

### 3.2 GPU (`gpu_kernel_descriptor`)

O closure `write_params` precisa calcular `effective_radius` e passá-lo ao
shader:

```rust
fn gpu_kernel_descriptor(&self) -> Option<GpuKernelDescriptor> {
    let radius = self.radius;
    Some(GpuKernelDescriptor {
        // ... restante igual ...
        write_params: Some(Arc::new(move |item, dst| {
            let nbhd = match item {
                crate::graph::item::Item::Neighborhood(n) => n,
                _ => return,
            };
            let mip_level = nbhd.center.mip_level;
            let effective_radius = radius >> mip_level;
            let params = BlurParams {
                width: nbhd.center.width + 2 * effective_radius,
                height: nbhd.center.height + 2 * effective_radius,
                radius: effective_radius,
                _pad: 0,
            };
            dst.copy_from_slice(bytemuck::bytes_of(&params));
        })),
    })
}
```

**Nota sobre o shader:** O `blur.spv` atual usa box blur com raio configurável.
Precisa verificar se lida com `radius == 0` (basta retornar o pixel central
sem somar vizinhos). Se não lidar, adicionar guard clause no shader Slang.

### 3.3 Blur com raio configurável via pipeline (não hardcoded no desktop)

Atualmente `BLUR_RADIUS = 32` hardcoded no desktop. Idealmente o `Blur` stage
já é configurável via parâmetro (`Blur { radius: u32 }`). Manter assim.

---

## 4. Novas Operações

### 4.1 `MipFilter` (`operation/mip_filter.rs`)

Filtra tiles por nível MIP. Útil para debug e para garantir que apenas um MIP
específico chegue ao sink.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipFilter {
    pub mip_level: u32,
}
```

**Ports:** 1 input Tile → 1 output Tile
**Hints:** `ReadOnly`, `prefers_gpu: false`
**CpuKernel:** Recebe `Item::Tile`. Se `tile.coord.mip_level == self.mip_level`,
re-emite. Caso contrário, descarta silenciosamente (não emite nada).

**Uso na pipeline do desktop (debug):**
```
... → MipFilter(mip_level=2) → ViewportSink   # Mostra só MIP 2
```

Adicionar ao `OperationNode` enum.

### 4.2 `MipPyramid` (`operation/mip_pyramid.rs`)

Acumula tiles e detecta blocos 2×2 completos. NÃO faz downsampling — apenas
emite `Tile` (pass-through) e `TileBlock` (quando um bloco está completo).
O downsampling é feito pelo `MipCompose` (estágio separado).

**Princípios de design:**
1. Separação de responsabilidades: acumulação vs composição
2. `TileBlock` encapsula 4 tiles prontos para downsampling (ver §1.5)
3. NUNCA usar tuplas como índice — usar `MipTileKey`
4. Roda em CPU (tracking de grid) — tiles podem vir de CPU ou GPU

#### 4.2.1 Tipos modelo

```rust
/// Identificador único de um tile na pirâmide MIP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MipTileKey {
    pub mip_level: u32,
    pub tx: u32,
    pub ty: u32,
}

/// Grade de tiles em um único nível MIP.
#[derive(Debug)]
pub struct MipLevelGrid {
    level: u32,
    tiles: HashMap<MipTileKey, Tile>,
    cols: u32,
    rows: u32,
}

impl MipLevelGrid {
    pub fn new(level: u32, image_width: u32, image_height: u32, tile_size: u32) -> Self { ... }

    pub fn insert(&mut self, key: MipTileKey, tile: Tile) { ... }

    /// Remove e retorna os 4 tiles do bloco 2×2, ou None se incompleto.
    pub fn take_block(&mut self, tx_tl: u32, ty_tl: u32) -> Option<[Tile; 4]> { ... }

    pub fn is_block_ready(&self, tx_tl: u32, ty_tl: u32) -> bool { ... }

    /// Blocos candidatos a estarem completos após inserção em (tx, ty).
    pub fn candidate_blocks(tx: u32, ty: u32) -> Vec<(u32, u32)> { ... }

    pub fn drain_remaining(&mut self) -> Vec<Tile> { ... }
}
```

#### 4.2.2 Estrutura do MipPyramid stage

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipPyramid {
    pub max_level: u32,
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
}
```

**Ports:** 1 input Tile → 1 output (emite Tile e TileBlock no mesmo canal)
**Hints:** `ReadOnly`, `prefers_gpu: false`

#### 4.2.3 CpuKernel: `MipPyramidRunner`

```
process(tile @ MIP N):
  1. Emitir tile (pass-through)
  2. Inserir no grid[N]
  3. Para cada bloco candidato em grid[N]:
     a. Se bloco 2×2 completo:
        - take_block() → [t00, t01, t10, t11]
        - Se N < max_level:
          * Emitir Item::TileBlock(TileBlock { coord, tiles })
        - Remover do grid (já feito pelo take_block)

finish():
  1. Para cada nível N, drena tiles restantes como TileBlocks parciais
     (bordas da imagem com dimensões ímpares)
```

### 4.3 `MipCompose` (`operation/mip_compose.rs`)

Faz o downsampling de `TileBlock` → `Tile` no próximo MIP. Faz passthrough
de `Tile` (não afeta tiles normais, apenas processa blocos).

**Princípios de design:**
1. Passthrough para `Item::Tile` — não modifica
2. Para `Item::TileBlock`: faz downsampling 2×2 → `Tile` no MIP+1
3. O downsampling usa GPU via `Scheduler` (mantém dados na GPU)
4. Também suporta CPU fallback (box filter simples)

#### 4.3.1 Estrutura

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MipCompose {
    pub tile_size: u32,
    pub image_width: u32,   // MIP 0 dimensions
    pub image_height: u32,
}
```

**Ports:** 1 input (Tile | TileBlock) → 1 output Tile
**Hints:** `ReadTransform`, `prefers_gpu: true` (downsampling pode rodar na GPU)

#### 4.3.2 CpuKernel: `MipComposeRunner`

```
process(item):
  match item:
    Item::Tile(tile) → emit tile (passthrough)
    Item::TileBlock(block) →
      - Se tiles estão na GPU: dispatch_gpu_downsample(block) → Tile MIP N+1
      - Se tiles estão na CPU: cpu_downsample(block) → Tile MIP N+1
      - Emitir tile resultante
    _ → erro
```

#### 4.3.3 GPU Downsampling via Scheduler

O `MipComposeRunner` acessa o `GpuContext` (padrão `UploadRunner`/`DownloadRunner`)
para disparar o kernel de downsampling 2×2.

```rust
fn dispatch_gpu_downsample(
    &self,
    block: &TileBlock,
    scheduler: &Scheduler,
) -> Result<Tile, Error> {
    let gbufs: [&GpuBuffer; 4] = [
        block.tiles[0].data.as_gpu().unwrap(),
        block.tiles[1].data.as_gpu().unwrap(),
        block.tiles[2].data.as_gpu().unwrap(),
        block.tiles[3].data.as_gpu().unwrap(),
    ];

    let out_width = (block.tiles[0].coord.width + block.tiles[1].coord.width) / 2;
    let out_height = (block.tiles[0].coord.height + block.tiles[2].coord.height) / 2;

    let mip_level = block.coord.mip_level + 1;
    // Criar KernelSignature com 4 inputs, dispatcher via scheduler
    // Retornar Tile com Buffer::Gpu(out_gbuf)
}
```

#### 4.3.4 CPU Downsampling (fallback)

Box filter 2×2 simples sobre os 4 tiles:

```rust
fn cpu_downsample(block: &TileBlock, tile_size: u32) -> Result<Tile, Error> {
    let out_w = /* média das larguras */;
    let out_h = /* média das alturas */;
    let mut out = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let px = avg_4_pixels(&block.tiles, x, y);
            let off = (y * out_w + x) * 4;
            out[off..off+4].copy_from_slice(&px);
        }
    }
    // ...
}
```

#### 4.3.5 Integração com o Scheduler

Ver §4.2.5 (agora aplicável ao MipCompose). O `Scheduler.dispatch_one()` aceita
múltiplos inputs. Kernel SPIR-V com 4 inputs de leitura + 1 output de escrita.

#### 4.3.6 Shader de downsampling (`shaders/mip_downsample.slang`)

```slang
// mip_downsample.slang — 2×2 box filter
[[vk::binding(0, 0)]] StructuredBuffer<uint> src00;
[[vk::binding(1, 0)]] StructuredBuffer<uint> src01;
[[vk::binding(2, 0)]] StructuredBuffer<uint> src10;
[[vk::binding(3, 0)]] StructuredBuffer<uint> src11;
[[vk::binding(4, 0)]] RWStructuredBuffer<uint> dst;

struct Params {
    out_width: uint,
    out_height: uint,
    // ... dimensões de cada tile de entrada
}
[[vk::push_constant]] ConstantBuffer<Params> p;

[numthreads(8, 8, 1)]
void cs_mip_downsample(uint3 gid : SV_DispatchThreadID) {
    if (gid.x >= p.out_width || gid.y >= p.out_height) return;
    uint ox = gid.x, oy = gid.y;
    uint sx = ox * 2, sy = oy * 2;

    uint4 c00 = clamp_sample(src00, sx,   sy,   p.in00_width, p.in00_height);
    uint4 c01 = clamp_sample(src01, sx,   sy,   p.in01_width, p.in01_height);
    uint4 c10 = clamp_sample(src10, sx,   sy,   p.in10_width, p.in10_height);
    uint4 c11 = clamp_sample(src11, sx,   sy,   p.in11_width, p.in11_height);

    uint4 avg = (c00 + c01 + c10 + c11) / 4u;
    dst[oy * p.out_width + ox] = avg.r | (avg.g << 8) | (avg.b << 16) | (avg.a << 24);
}
```

---

## 5. Sources MIP-Aware

### 5.1 `ImageFileSource` e `FileDecoder`

Sempre emitem `mip_level = 0`. Mudança: adicionar `mip_level: 0` nos
construtores de `ScanLine::new(...)` e `ScanLineCoord`.

### 5.2 `CacheReader` (`source/cache_reader.rs`) — IMPLEMENTAR

Lê tiles de um cache em disco. Suporta filtro por MIP level e range de tiles.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheReader {
    pub cache_dir: PathBuf,
    pub mip_level: u32,
    pub tile_size: u32,
    pub image_width: u32,    // dimensões no MIP 0 (para calcular grid no MIP alvo)
    pub image_height: u32,
    pub tile_range: Option<TileRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileRange {
    pub tx_start: u32,
    pub tx_end: u32,   // exclusivo
    pub ty_start: u32,
    pub ty_end: u32,   // exclusivo
}
```

**CpuKernel (source — sem inputs):**
- Primeira chamada (dummy tile): calcula o grid de tiles para o MIP
  `self.mip_level`. Para cada tile no range (ou todos se `tile_range.is_none()`):
  - Lê arquivo: `{cache_dir}/mip_{mip_level}/tile_{mip_level}_{tx}_{ty}.raw`
  - Se arquivo não existe, pula (não emite nada para esse tile)
  - Constrói `TileCoord::new(mip_level, tx, ty, tile_size, mip_width, mip_height)`
  - Emite `Item::Tile` com `Buffer::cpu(bytes_lidos)`
- Após emitir todos, retorna `Ok(())` sem emitir mais nada
- `finish()`: no-op

**Ports:** 0 inputs, 1 output: Tile

---

## 6. Sinks MIP-Aware

### 6.1 `CacheWriter` (`sink/cache_writer.rs`) — IMPLEMENTAR

Escreve tiles em disco no formato raw RGBA8.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheWriter {
    pub cache_dir: PathBuf,
}
```

**CpuKernel:**
- Recebe `Item::Tile`
- Cria diretório: `{cache_dir}/mip_{tile.coord.mip_level}/`
- Escreve arquivo: `{cache_dir}/mip_{mip}/tile_{mip}_{tx}_{ty}.raw`
- Formato: `[u8; width * height * 4]` — RGBA8 direto, sem header, sem conversão
- Só aceita tile CPU. Se GPU, erro (`"CacheWriter requires CPU tiles"`)
- `finish()`: no-op (cada tile é salvo individualmente)

**Ports:** 1 input Tile, 0 outputs.

**Formato do arquivo de cache:**
```
{cache_dir}/
  mip_0/
    tile_0_0_0.raw
    tile_0_0_1.raw
    ...
  mip_1/
    tile_1_0_0.raw
    ...
```

### 6.2 `ViewportSink` (`sink/viewport.rs`) — ATUALIZAR

**Problema atual:** Descarta silenciosamente tiles CPU. Só aceita GPU.

**Mudanças:**
1. Se tile é GPU: `copy_buffer_to_texture` (comportamento atual)
2. Se tile é CPU: faz upload via `queue.write_texture()` diretamente (mais
   eficiente que GPU intermediate buffer)
3. Propagar `tile.coord.mip_level` para o `mip_level` do `Origin3d`
4. `ViewportTarget` ganha campo `current_mip: u32`. Se o `mip_level` do tile
   não bater com o target, descarta (ou loga warning).

```rust
pub struct ViewportTarget {
    pub texture: Arc<wgpu::Texture>,
    pub queue: Arc<wgpu::Queue>,
    pub current_mip: u32,
}
```

### 6.3 `TileSink` (`sink/tile_sink.rs`) — ATUALIZAR

Callback ganha `mip_level`:

```rust
type TileCommitFn = dyn Fn(u32 /* mip_level */, u32 /* px */, u32 /* py */,
                           u32 /* tw */, u32 /* th */, &[u8]) + Send + Sync;
```

---

## 7. Runtime — Ajustes Mínimos

### 7.1 Dummy tile no source path (`cpu.rs`)

```rust
let dummy = Item::Tile(Tile::new(
    TileCoord::new(0, 0, 0, 0, 0, 0),  // mip_level, tx, ty, ts, iw, ih
    PixelMeta::new(PixelFormat::Rgba8, ColorSpace::SRGB, AlphaPolicy::Straight),
    Buffer::cpu(vec![]),
));
```

### 7.2 Pipeline compilation — sem `force_cpu` (REMOVIDO do plano)

A atribuição de device continua como antes:
```rust
let dev = if gpu_available
    && stage.gpu_kernel_descriptor().is_some()
    && stage.hints().prefers_gpu
{
    Device::Gpu
} else {
    Device::Cpu
};
```

O `MipPyramid` não tem `gpu_kernel_descriptor()` (retorna `None`), portanto é
atribuído a `Device::Cpu`. O Upload/Download automático entre CPU↔GPU é
gerenciado pelo pipeline compiler (que insere `Upload` ou `Download` nas bordas
entre cadeias CPU e GPU).

**Exemplo de pipeline:**
```
Blur (GPU) → MipPyramid (CPU)
```
Pipeline compiler insere `Download` automaticamente entre Blur e MipPyramid.
Isso significa que os tiles são baixados da GPU para CPU uma vez. Depois,
o MipPyramid faz downsampling na GPU (via Scheduler) e o CacheWriter escreve
em disco (CPU).

Para evitar download duplo (GPU→CPU pelo Download automático, depois
CPU→GPU no MipPyramid para downsampling), o MipPyramid pode ser otimizado:

**Otimização:** O MipPyramid poderia rodar na própria GPU chain, evitando o
download. Mas isso requer mudanças maiores no GpuChainRunner. Para a primeira
versão, aceitamos o download (que é o caminho natural já que o CacheWriter
precisa dos dados em CPU de qualquer forma).

### 7.3 `finish()` — sem mudanças

O `finish()` já é chamado no final do stream de cada stage. Para o
`MipPyramid`, o `finish()` processa blocos 2×2 incompletos (bordas da imagem
com dimensões ímpares) e emite tiles parciais. A mecânica de `run_finish` no
`CpuChainRunner` já propaga os itens emitidos no `finish()` pelos kernels
downstream.

---

## 8. Pipeline do Desktop

### 8.1 Pipeline de processamento (batch)

Roda uma vez ao abrir uma imagem. Processa MIP 0 com blur e gera pirâmide
completa, salvando tudo em cache de disco.

```
ImageFileSource(ScanLine, mip=0)
  → ScanLineAccumulator(Tile, mip=0, tile_size=256)
  → NeighborhoodAgg(Radius=32)
  → Blur(Radius=32)
  → MipPyramid(max_level=computed, tile_size=256, img_w, img_h)
  → MipCompose(tile_size=256, img_w, img_h)
  → CacheWriter(cache_dir)
```

O `MipPyramid` emite `Tile` (pass-through do MIP 0) e `TileBlock` (blocos 2×2
completos). O `MipCompose` faz passthrough dos `Tile` e compõe cada `TileBlock`
em um `Tile` no MIP+1 via GPU downsampling. O `CacheWriter` salva todos os
tiles (todos os MIPs) em disco.

**Cálculo de `max_level`:**
```rust
fn compute_max_level(width: u32, height: u32, tile_size: u32) -> u32 {
    let max_dim = width.max(height);
    let levels = (max_dim as f64 / tile_size as f64).log2().floor() as u32;
    levels.min(8) // limite superior razoável
}
```

### 8.2 Pipeline de visualização (interativa)

Quando a imagem é aberta, carrega os tiles visíveis no MIP inicial (fit = MIP 0
ou MIP baixo dependendo do zoom):

```
CacheReader(mip_level=visible_mip, tile_range=visible_tiles)
  → ViewportSink
```

### 8.3 Pipeline de refresh (on-demand)

Quando o usuário faz zoom/pan e novos tiles são necessários:

```
CacheReader(mip_level=new_mip, tile_range=missing_tiles)
  → ViewportSink
```

---

## 9. Viewport MIP-Aware

### 9.1 Cálculo do MIP visível (`camera.rs`)

```rust
impl Camera {
    /// Retorna o nível MIP apropriado para o zoom atual.
    /// zoom ≥ 0.5 → MIP 0 (full res)
    /// zoom < 0.5 → MIP = floor(-log2(zoom))
    pub fn visible_mip_level(&self) -> u32 {
        if self.zoom >= 0.5 {
            0
        } else {
            (-self.zoom.log2().floor() as u32).min(MAX_MIP_LEVEL)
        }
    }
}
```

Exemplos:
- zoom=1.0 → MIP 0
- zoom=0.4 → MIP 1 (log2(0.4) ≈ -1.32, floor → -2, abs → 2? wait: -log2(0.4) = 1.32, floor = 1)
  Corrigindo: `(-self.zoom.log2().floor() as u32)`
  - zoom=0.4 → -log2(0.4) = -(-1.32) = 1.32 → floor = 1
  - zoom=0.25 → -log2(0.25) = -(-2) = 2 → floor = 2
  - zoom=0.1 → -log2(0.1) = -(-3.32) = 3.32 → floor = 3

### 9.2 `TiledTexture` adaptável (`tiled_texture.rs`)

A textura agora tem tamanho variável dependendo do MIP visível.

```rust
pub struct TiledTexture {
    texture: wgpu::Texture,
    full_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// Dimensões atuais da textura (= dimensões da imagem no MIP atual)
    width: u32,
    height: u32,
    /// MIP level que esta textura representa
    mip_level: u32,
}
```

Método `resize(device, new_width, new_height, new_mip_level)`:
- Recria a textura com as novas dimensões
- Recria a TextureView
- Atualiza width, height, mip_level

### 9.3 `ViewportTileCache` (`viewport/tile_cache.rs` — NOVO)

Cache de tiles em RAM para evitar re-leitura do disco durante pan.

```rust
pub struct ViewportTileCache {
    tiles: HashMap<MipTileKey, CachedTile>,
    max_tiles: usize,  // ~256 tiles = ~64 MB para RGBA8 256×256
}

struct CachedTile {
    bytes: Vec<u8>,
    key: MipTileKey,
    px: u32,
    py: u32,
    width: u32,
    height: u32,
}

impl ViewportTileCache {
    pub fn new(max_tiles: usize) -> Self { ... }

    pub fn get(&self, key: &MipTileKey) -> Option<&CachedTile> { ... }

    pub fn insert(&mut self, key: MipTileKey, tile: CachedTile) {
        if self.tiles.len() >= self.max_tiles {
            self.evict_lru();
        }
        self.tiles.insert(key, tile);
    }

    /// Remove todos os tiles de um MIP level específico (quando o MIP muda).
    pub fn clear_mip(&mut self, mip_level: u32) {
        self.tiles.retain(|k, _| k.mip_level != mip_level);
    }

    fn evict_lru(&mut self) { ... }
}
```

### 9.4 `CameraUniform` atualizado (`camera.rs`)

Adicionar campos para o MIP atual:

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub vp_w: f32,
    pub vp_h: f32,
    pub img_w: f32,     // dimensões no MIP atual (não mais MIP 0)
    pub img_h: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub mip_level: f32, // NOVO — substitui _pad
}
```

**Shader WGSL atualizado (`pipeline.rs`):**

```wgsl
struct Camera {
    vp_w: f32, vp_h: f32,
    img_w: f32, img_h: f32,
    pan_x: f32, pan_y: f32,
    zoom: f32, mip_level: f32,
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let screen = in.uv * vec2<f32>(cam.vp_w, cam.vp_h);
    // Converte screen pixels → image space (MIP 0 space)
    let img_xy_mip0 = screen / cam.zoom + vec2<f32>(cam.pan_x, cam.pan_y);
    // Converte MIP 0 space → MIP N space
    let mip_scale = pow(2.0, cam.mip_level);
    let img_xy = img_xy_mip0 / mip_scale;
    if img_xy.x < 0.0 || img_xy.y < 0.0 || img_xy.x >= cam.img_w || img_xy.y >= cam.img_h {
        return vec4<f32>(0.067, 0.067, 0.075, 1.0);
    }
    return textureSample(t, s, img_xy / vec2<f32>(cam.img_w, cam.img_h));
}
```

**Lógica do `to_uniform()`:**
```rust
pub fn to_uniform(&self, mip_level: u32) -> CameraUniform {
    let mip_scale = (1u32 << mip_level) as f32;
    CameraUniform {
        vp_w: self.vp_w,
        vp_h: self.vp_h,
        img_w: self.img_w / mip_scale,  // dimensões no MIP N
        img_h: self.img_h / mip_scale,
        pan_x: self.pan_x,
        pan_y: self.pan_y,
        zoom: self.zoom,
        mip_level: mip_level as f32,
    }
}
```

### 9.5 `PendingTileWrites` atualizado

```rust
pub struct PendingTile {
    pub mip_level: u32,  // NOVO
    pub px: u32,
    pub py: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub bytes: Vec<u8>,
}
```

O `ViewportPipeline::prepare()` agora:
1. Verifica o MIP visível atual (do `Camera`)
2. Se o MIP visível mudou desde o último frame:
   - Re-aloca `TiledTexture` com novas dimensões
   - Limpa tiles do MIP antigo do `ViewportTileCache`
   - Calcula tiles visíveis no novo MIP
   - Dispara pipeline: `CacheReader(new_mip, visible_range) → ViewportSink`
3. Se o MIP não mudou mas há novos tiles pendentes (pan):
   - Verifica cache primeiro
   - Tiles em cache → escreve direto na textura
   - Tiles faltando → dispara pipeline on-demand
4. Drena `PendingTileWrites` e escreve na textura

### 9.6 Fluxo completo de interação

```
1. Usuário abre imagem
   → Pipeline batch: Source → Blur → MipPyramid → CacheWriter
   → Aguarda conclusão (ou começa preview parcial)

2. Batch concluído
   → Camera.fit() → calcula visible_mip_level()
   → Aloca TiledTexture no tamanho do MIP visível
   → Calcula tiles visíveis no viewport
   → Pipeline: CacheReader(mip, visible_range) → ViewportSink
   → Tiles chegam → ViewportSink escreve na textura → Render

3. Usuário faz pan
   → Camera pan atualiza
   → ViewportPipeline::prepare() verifica novos tiles visíveis
   → Tiles em ViewportTileCache → escreve direto
   → Tiles faltando → Pipeline on-demand: CacheReader(mip, missing_range) → ViewportSink
   → Render com textura atual (pode ter buracos temporários para tiles ainda carregando)

4. Usuário faz zoom (muda MIP visível)
   → Camera.visible_mip_level() muda
   → Re-aloca TiledTexture (resize)
   → Limpa cache do MIP antigo
   → Pipeline: CacheReader(new_mip, all_visible) → ViewportSink
   → Render com nova textura
```

### 9.7 Cálculo de tiles visíveis

```rust
fn visible_tiles(
    camera: &Camera,
    mip_level: u32,
    tile_size: u32,
    vp_width: u32,
    vp_height: u32,
) -> TileRange {
    let mip_scale = 1u32 << mip_level;

    // Viewport bounds in MIP 0 space
    let vp_left_mip0 = camera.pan_x;
    let vp_top_mip0 = camera.pan_y;
    let vp_right_mip0 = camera.pan_x + vp_width as f32 / camera.zoom;
    let vp_bottom_mip0 = camera.pan_y + vp_height as f32 / camera.zoom;

    // Convert to MIP N space
    let vp_left = (vp_left_mip0 / mip_scale as f32).floor() as u32;
    let vp_top = (vp_top_mip0 / mip_scale as f32).floor() as u32;
    let vp_right = (vp_right_mip0 / mip_scale as f32).ceil() as u32;
    let vp_bottom = (vp_bottom_mip0 / mip_scale as f32).ceil() as u32;

    // Tile range
    let tx_start = vp_left / tile_size;
    let ty_start = vp_top / tile_size;
    let tx_end = (vp_right / tile_size + 1).min(mip_cols);
    let ty_end = (vp_bottom / tile_size + 1).min(mip_rows);

    TileRange { tx_start, tx_end, ty_start, ty_end }
}
```

---

## 10. Sumário de Arquivos

### pixors-executor

| Arquivo | Mudança |
|---------|---------|
| `src/data/tile.rs` | `mip_level: u32`, `DEFAULT_TILE_SIZE`, construtor atualizado |
| `src/data/neighborhood.rs` | `mip_level: u32` no `NeighborhoodCoord` |
| `src/data/scanline.rs` | `mip_level: u32` no `ScanLine` e `ScanLineCoord` |
| `src/stage.rs` | (sem mudanças — `force_cpu` removido do plano) |
| `src/operation/blur.rs` | `effective_radius = self.radius >> mip_level` no CPU e GPU |
| `src/operation/data/to_tile.rs` | Repassar `mip_level` das scanlines para tiles |
| `src/operation/data/to_neighborhood.rs` | Cache key com `mip_level`; raio escala com MIP |
| `src/operation/data/to_scanline.rs` | Repassar `mip_level` dos tiles para scanlines |
| `src/operation/mip_filter.rs` | **NOVO** — filtra por mip_level |
| `src/operation/mip_pyramid.rs` | **NOVO** — downsample streaming com GPU via Scheduler |
| `src/operation/transfer/upload.rs` | Repassar mip_level (já funciona via tile.coord) |
| `src/operation/transfer/download.rs` | Repassar mip_level (já funciona via tile.coord) |
| `src/operation/mod.rs` | Adicionar `MipFilter`, `MipPyramid` ao `OperationNode` |
| `src/source/image_file_source.rs` | `mip_level = 0` |
| `src/source/file_decoder.rs` | `mip_level = 0` |
| `src/source/cache_reader.rs` | **IMPLEMENTAR** — leitura MIP-aware com `TileRange` |
| `src/source/mod.rs` | `CacheReader(CacheReader)` no `SourceNode` |
| `src/sink/cache_writer.rs` | **IMPLEMENTAR** — escrita raw RGBA8 por MIP |
| `src/sink/viewport.rs` | Suporte a CPU tiles; `current_mip` no target |
| `src/sink/tile_sink.rs` | Callback inclui `mip_level` |
| `src/sink/mod.rs` | `CacheWriter(CacheWriter)` no `SinkNode` |
| `src/runtime/cpu.rs` | Dummy tile com `mip_level = 0` |
| `src/runtime/pipeline.rs` | (sem mudanças — `force_cpu` removido) |
| `kernels/mip_downsample.spv` | **NOVO** — shader de downsampling 2×2 |
| `shaders/mip_downsample.slang` | **NOVO** — fonte Slang do shader |

### pixors-desktop

| Arquivo | Mudança |
|---------|---------|
| `src/ui/file_ops.rs` | Pipeline batch + pipeline de visualização inicial |
| `src/viewport/camera.rs` | `visible_mip_level()`; `to_uniform(mip_level)` |
| `src/viewport/tiled_texture.rs` | `resize()` por MIP; `mip_level` no struct |
| `src/viewport/pipeline.rs` | Shader atualizado com `mip_level`; textura adaptável |
| `src/viewport/program.rs` | Cache `ViewportTileCache`; on-demand pipeline dispatch |
| `src/viewport/tile_cache.rs` | **NOVO** — `ViewportTileCache` LRU |

---

## 11. Ordem de Implementação

1. **Fase 1** — `mip_level` em todos os runtime types + propagação em callers
   - `data/tile.rs`, `data/neighborhood.rs`, `data/scanline.rs`
   - Todos os callers listados em §1.5
   - Compilar e testar após cada subtask

2. **Fase 2** — `MipFilter` (simples, bom warm-up)
   - `operation/mip_filter.rs` + `operation/mod.rs`

3. **Fase 3** — Blur MIP-aware
   - CPU e GPU com `effective_radius`
   - Verificar shader para `radius == 0`

4. **Fase 4** — `MipPyramid` com modelo próprio
   - `MipTileKey`, `MipLevelGrid`
   - `MipPyramidRunner` com GPU downsampling via Scheduler
   - Kernel SPIR-V de downsampling (`kernels/mip_downsample.spv`)

5. **Fase 5** — CacheWriter + CacheReader
   - `sink/cache_writer.rs`, `source/cache_reader.rs`
   - Formato raw RGBA8 em disco

6. **Fase 6** — ViewportSink atualizado
   - Suporte a CPU tiles com `write_texture`
   - `current_mip` no `ViewportTarget`

7. **Fase 7** — Pipeline desktop (batch + visualização)
   - `file_ops.rs` com nova pipeline
   - Testar fluxo completo: abrir imagem → batch → preview

8. **Fase 8** — Viewport MIP-aware
   - `camera.rs`: `visible_mip_level()`, `to_uniform(mip)`
   - `tiled_texture.rs`: resize
   - `pipeline.rs`: shader atualizado
   - `tile_cache.rs`: ViewportTileCache
   - `program.rs`: integração com cache e on-demand pipeline

9. **Fase 9** — Testes e limpeza
   - `cargo check --workspace && cargo test --workspace && cargo clippy --workspace`
   - Remover código não utilizado
   - Testar manualmente com várias imagens e níveis de zoom

---

## 12. Decisões de Design

1. **Dimensões no `TileCoord::new()` são do MIP atual, não do MIP 0.**
   Ex: Se `image_width=4096`, `mip_level=1`, `tile_size=256`, passamos
   `image_width=2048` ao construtor. Edge tiles são calculados corretamente.

2. **`NeighborhoodAgg` NÃO escala o raio com MIP.** O raio em tiles é mantido
   fixo. O Blur escala o raio efetivo. Isso simplifica o código e o overhead de
   tiles extras é pequeno (tiles são 256×256).

3. **Cache de disco usa raw RGBA8, sem conversão de espaço de cor.**
   O `WorkingWriter` (model/storage) usa ACEScg f16, mas para preview rápido
   RGBA8 é suficiente e evita conversão desnecessária.

4. **MipPyramid faz downsampling na GPU via Scheduler, não no GpuChainRunner.**
   A acumulação é CPU-side. O dispatch de GPU usa o `Scheduler` singleton
   (mesmo padrão de `UploadRunner`/`DownloadRunner`). Os tiles ficam na GPU
   durante o downsampling.

5. **ViewportSink aceita tiles CPU e GPU.** Para tiles CPU, usa
   `queue.write_texture()` (upload direto para textura). Para tiles GPU, usa
   `copy_buffer_to_texture`. Isso evita upload intermediário desnecessário.

6. **`ViewportTileCache` é LRU com limite configurável.** Tiles de MIPs
   diferentes do atual são limpos quando o MIP muda, evitando acúmulo
   desnecessário.

7. **Sem `force_cpu` no `StageHints`.** A atribuição CPU/GPU é decidida
   automaticamente pelo pipeline compiler baseado em `prefers_gpu` e
   `gpu_kernel_descriptor()`. Upload/Download são inseridos automaticamente
   nas bordas CPU↔GPU.
