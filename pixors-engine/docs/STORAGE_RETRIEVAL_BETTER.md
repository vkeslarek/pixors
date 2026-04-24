# Tile Storage, MIP Pyramid & Viewport Pipeline — Architecture Plan

## 0. Problema Real

Editor precisa ser interativo. Operações em imagem 4K inteira antes de exibir = freeze de 500ms–5s. Inaceitável.

Solução: tiles + MIPs + viewport pull model. Nada novo, é como Lightroom/Photoshop funciona.

---

## 1. Decisão Fundamental: Formato dos Tiles

**Duas opções:**

| Critério | ACEScg f16 | sRGB u8 |
|----------|-----------|---------|
| MIP downsampling correto | ✅ linear | ❌ incorreto (aplica em gamma) |
| Operações diretas | ✅ sem conversão | ❌ precisa desconverter antes |
| Display cost | ~1ms/tile SIMD | ~0ms (cópia direta) |
| Tamanho em disco | 128KB/tile (f16 RGBA) | 256KB/tile (u8 RGBA) |
| Precisão para operações | ✅ 10 bits reais | ❌ 8 bits |

**Decisão: Tiles armazenados em ACEScg f16 premultiplied.**

sRGB u8 é apenas display cache — gerado on-demand, invalidado quando tile muda.

### Por que não sRGB para storage?

Blur de 128px em sRGB = errado. Cada operação que mistura pixels (blur, resize, dissolve, gradients) produz resultados incorretos em gamma space. Downsampling de MIPs em sRGB = darkening incorreto. O custo de conversão para display é trivial comparado a fazer tudo errado.

### Fast path para imagens sRGB comuns

Conversão sRGB → ACEScg é: LUT(R, G, B) + matrix 3×3. Com SIMD:
- LUT de gamma: tabela 256 floats pré-computada → lookup O(1) por canal
- Matrix 3×3 × 4 pixels via `wide::f32x4` → ~16 ops por 4 pixels
- Estimativa: 256×256 tile → ~65K pixels → ~2ms (sem SIMD) → ~0.5ms (com SIMD)

---

## 2. Estrutura de Dados

```
Imagem aberta:
  ├── TileStore (MIP 0) — ACEScg f16 premul, 256×256 tiles, disco
  ├── MipLevel 1        — ACEScg f16 premul, tiles de imagem 1/2 resolução
  ├── MipLevel 2        — ACEScg f16 premul, tiles de imagem 1/4 resolução
  ├── ...
  └── TileCache (RAM)   — sRGB u8 para tiles visíveis, invalidável por TileCoord
```

### Tile storage em disco (ACEScg f16)

```
tile_MIP{level}_{tx}_{ty}.raw:
  bytes: width × height × 4 canais × 2 bytes (f16) = 128KB por tile 256×256
  layout: RGBA interleaved, row-major, f16 IEEE 754, little-endian, premultiplied
  color space: ACEScg (primaries AP1)
```

### TileCache em RAM (sRGB u8)

```
HashMap<TileCoord, Arc<Vec<u8>>>
  bytes: width × height × 4 (RGBA u8)
  invalidado via TileCoord quando tile ACEScg subjacente muda
  frontend recebe TileInvalidated e pede novo tile
```

---

## 3. Abstração Central: `Tile<P>` Genérico

Um tile é um tile — independente de MIP level, de estar em RAM ou disco, de ser ACEScg ou sRGB. O que muda é o pixel type `P`.

```rust
// Em src/image/tile.rs — refactor do Tile atual

/// Identidade de um tile — tudo que o engine precisa para localizar qualquer tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoord {
    pub mip_level: u32,
    pub tx: u32,     // índice X do tile no grid (não pixel X)
    pub ty: u32,     // índice Y do tile no grid
    pub px: u32,     // pixel X do canto superior esquerdo = tx * tile_size
    pub py: u32,     // pixel Y
    pub width: u32,  // largura real (borda pode ser < tile_size)
    pub height: u32,
}

impl TileCoord {
    /// Cria coordenada dado tile index e dimensões da imagem
    pub fn new(
        mip_level: u32,
        tx: u32,
        ty: u32,
        tile_size: u32,
        image_width: u32,
        image_height: u32,
    ) -> Self {
        let px = tx * tile_size;
        let py = ty * tile_size;
        let width = (image_width - px).min(tile_size);
        let height = (image_height - py).min(tile_size);
        Self { mip_level, tx, ty, px, py, width, height }
    }

    pub fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }
}

/// Tile com dados em memória — genérico sobre pixel type.
/// `P = Rgba<f16>` para storage ACEScg.
/// `P = u8` para display sRGB (Vec<u8> raw RGBA8).
#[derive(Clone)]
pub struct Tile<P: Clone> {
    pub coord: TileCoord,
    pub data: Arc<Vec<P>>,
}

impl<P: Clone> Tile<P> {
    pub fn new(coord: TileCoord, data: Vec<P>) -> Self {
        Self { coord, data: Arc::new(data) }
    }
}

/// Conversão entre formatos via ColorConversion — sem acoplamento a implementação.
impl Tile<Rgba<f16>> {
    /// Converte ACEScg f16 → sRGB u8 usando matrix pré-computada.
    /// Retorna Tile de display pronto para WebSocket.
    pub fn to_srgb_u8(&self, conv: &ColorConversion) -> Tile<u8> {
        let srgb = acescg_f16_to_srgb_u8_simd(&self.data, conv.matrix());
        Tile { coord: self.coord, data: Arc::new(srgb) }
    }

    /// Converte para f32 para operações (sem overhead de conversão dupla).
    pub fn to_f32_straight(&self) -> Vec<Rgba<f32>> {
        self.data.iter().map(|px| {
            let a = px.a.to_f32();
            if a < 1e-6 {
                Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }
            } else {
                let inv = 1.0 / a;
                Rgba {
                    r: px.r.to_f32() * inv,
                    g: px.g.to_f32() * inv,
                    b: px.b.to_f32() * inv,
                    a,
                }
            }
        }).collect()
    }
}
```

### Por que não `CompletedTile`, `BandTile`, etc.?

Um só tipo `Tile<P>` cobre todos os casos:
- `Tile<Rgba<f16>>` = tile ACEScg no disco ou em RAM
- `Tile<u8>` = tile display sRGB pronto para envio
- Conversão é explícita via método, sem magic

---

## 4. Pipeline de Abertura de Imagem — Stream Agnóstico

### Estado atual (ruim)
```
PNG → BandBuffer (PNG-específico) → Vec<Rgba<f16>> por tile → disco
```

### Estado desejado
```
Qualquer fonte → ImageBuffer (com BufferDesc) → Tile<Rgba<f16>> → disco (MIP 0)
                                                               → gera MIP 1..N background
```

O stream é agnóstico ao formato de entrada porque consome `ImageBuffer` (que já tem `BufferDesc` descrevendo layout, color space, etc.). Para TIFF planar, PNG RGB, ou qualquer outro: basta criar o `ImageBuffer` com o descritor correto.

### Interface principal

```rust
// Em src/storage/tile_store.rs ou src/io/mod.rs
/// Stream qualquer ImageBuffer para tiles ACEScg f16 no TileStore.
/// Agnóstico: lê via BufferDesc, converte via ColorConversion.
pub fn stream_image_to_tiles(
    source: Arc<ImageBuffer>,    // qualquer formato, qualquer color space
    tile_size: u32,
    store: &TileStore,           // destino dos tiles no disco
) -> Result<(), Error> {
    let conv = source.desc.color_space.converter_to(ColorSpace::ACES_CG)?;
    let w = source.desc.width;
    let h = source.desc.height;
    let matrix = conv.matrix().as_3x3_array();

    // Buffer pré-alocado para uma band — nenhuma alocação dentro do loop
    let tiles_x = (w + tile_size - 1) / tile_size;
    let mut band_buf: Vec<Rgba<f16>> = vec![Rgba::ZERO; (w * tile_size) as usize];
    let mut rows_in_band: u32 = 0;
    let mut band_start_y: u32 = 0;

    for row_y in 0..h {
        // Converte row via BufferDesc — funciona para QUALQUER layout
        // (interleaved RGB, planar RGBA, etc.)
        convert_buffer_row_to_acescg_simd(
            &source,
            row_y,
            &mut band_buf[(rows_in_band * w) as usize..((rows_in_band + 1) * w) as usize],
            &conv,
        );
        rows_in_band += 1;

        // Band cheia (ou última row) → flush tiles para disco
        let is_last_row = row_y == h - 1;
        if rows_in_band == tile_size || is_last_row {
            let actual_rows = rows_in_band;
            for tx in 0..tiles_x {
                let tile_px = tx * tile_size;
                let actual_w = (w - tile_px).min(tile_size);
                let coord = TileCoord::new(0, tx, band_start_y / tile_size, tile_size, w, h);

                // Extrai tile da band sem alocação extra (copia por row)
                let mut tile_data = Vec::with_capacity((actual_w * actual_rows) as usize);
                for r in 0..actual_rows {
                    let src_start = (r * w + tile_px) as usize;
                    tile_data.extend_from_slice(
                        &band_buf[src_start..src_start + actual_w as usize]
                    );
                }

                store.write_tile(&Tile::new(coord, tile_data))?;
            }

            rows_in_band = 0;
            band_start_y = row_y + 1;
        }
    }

    Ok(())
}
```

### Conversão de row via BufferDesc (agnóstica ao formato)

```rust
// Em src/convert/simd.rs
/// Converte uma row de ImageBuffer para ACEScg f16 usando BufferDesc.
/// Funciona para QUALQUER layout: interleaved, planar, RGB, RGBA, Gray, etc.
/// Agnóstico porque usa `source.read_sample(plane, x, y)` — o descritor
/// cuida dos offsets/strides corretos.
pub fn convert_buffer_row_to_acescg_simd(
    source: &ImageBuffer,
    row_y: u32,
    dst: &mut [Rgba<f16>],
    conv: &ColorConversion,
) {
    let w = source.desc.width;
    let has_alpha = source.desc.planes.len() >= 4;
    let is_gray = source.desc.planes.len() <= 2;
    let matrix = conv.matrix().as_3x3_array();

    // SIMD 4 pixels por vez
    let chunks = dst.chunks_exact_mut(4);
    let mut x = 0u32;

    for chunk in chunks {
        for i in 0..4u32 {
            let px = x + i;
            // read_sample usa PlaneDesc.sample_offset → correto para qualquer layout
            let (r, g, b) = if is_gray {
                let v = source.read_sample(0, px, row_y);
                (v, v, v)
            } else {
                (
                    source.read_sample(0, px, row_y),
                    source.read_sample(1, px, row_y),
                    source.read_sample(2, px, row_y),
                )
            };
            let a = if has_alpha {
                source.read_sample(if is_gray { 1 } else { 3 }, px, row_y)
            } else {
                1.0
            };

            // Decode gamma via transfer fn (já incluso em conv.decode_to_linear)
            let [lr, lg, lb] = conv.decode_to_linear([r, g, b]);

            // Matrix primaries → ACEScg (SIMD: fora do loop interno para ganho real)
            let ar = matrix[0][0] * lr + matrix[0][1] * lg + matrix[0][2] * lb;
            let ag = matrix[1][0] * lr + matrix[1][1] * lg + matrix[1][2] * lb;
            let ab = matrix[2][0] * lr + matrix[2][1] * lg + matrix[2][2] * lb;

            chunk[i as usize] = Rgba {
                r: f16::from_f32(ar * a),
                g: f16::from_f32(ag * a),
                b: f16::from_f32(ab * a),
                a: f16::from_f32(a),
            };
        }
        x += 4;
    }

    // Remainder (< 4 pixels)
    for px in x..w {
        let (r, g, b) = if is_gray {
            let v = source.read_sample(0, px, row_y);
            (v, v, v)
        } else {
            (
                source.read_sample(0, px, row_y),
                source.read_sample(1, px, row_y),
                source.read_sample(2, px, row_y),
            )
        };
        let a = if has_alpha {
            source.read_sample(if is_gray { 1 } else { 3 }, px, row_y)
        } else {
            1.0
        };
        let [lr, lg, lb] = conv.decode_to_linear([r, g, b]);
        let ar = matrix[0][0] * lr + matrix[0][1] * lg + matrix[0][2] * lb;
        let ag = matrix[1][0] * lr + matrix[1][1] * lg + matrix[1][2] * lb;
        let ab = matrix[2][0] * lr + matrix[2][1] * lg + matrix[2][2] * lb;
        dst[(px) as usize] = Rgba {
            r: f16::from_f32(ar * a),
            g: f16::from_f32(ag * a),
            b: f16::from_f32(ab * a),
            a: f16::from_f32(a),
        };
    }
}

// Nota: SIMD real (f32x4 para matrix) é otimização do inner loop acima.
// Estrutura já permite: extrair r[4], g[4], b[4] e fazer matrix 3x3 via f32x4.
// Implementação scalar acima é correta e serve de referência.
```

### Uso: abrir qualquer formato

```rust
// PNG
let buffer = load_png(path)?;                          // retorna Arc<ImageBuffer>
stream_image_to_tiles(buffer, tile_size, &store)?;

// TIFF planar (futuro) — apenas muda o buffer, stream idêntico
let buffer = load_tiff_planar(path)?;                  // cria ImageBuffer com PlaneDesc planar
stream_image_to_tiles(buffer, tile_size, &store)?;

// Raw float (futuro)
let buffer = ImageBuffer {
    desc: BufferDesc { color_space: ColorSpace::ACES_CG, planes: [...], ... },
    data: raw_bytes,
};
stream_image_to_tiles(Arc::new(buffer), tile_size, &store)?;
```

---

## 5. Pipeline de MIP Pyramid

### Geração: logo após abertura, em background (rayon)

```rust
// Em src/image/mip.rs
pub struct MipPyramid {
    pub levels: Vec<MipLevel>,
}

pub struct MipLevel {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub store: Arc<TileStore>,
}

impl MipPyramid {
    /// Gera todos os níveis a partir do MIP 0.
    /// Roda via tokio::task::spawn_blocking (CPU-bound, rayon internamente).
    pub fn generate_from_mip0(
        mip0: Arc<TileStore>,
        tab_id: Uuid,
    ) -> Result<MipPyramid, Error> {
        let mut levels: Vec<MipLevel> = Vec::new();
        let mut src_store = mip0.clone();
        let tile_size = mip0.tile_size;
        let mut width = mip0.image_width;
        let mut height = mip0.image_height;
        let mut level = 0u32;

        loop {
            level += 1;
            width = (width + 1) / 2;
            height = (height + 1) / 2;

            // Para quando tile cobre imagem inteira (nenhum benefício adicional)
            if width <= tile_size && height <= tile_size {
                break;
            }

            let dst_store = Arc::new(TileStore::new_for_mip(
                &tab_id, level, tile_size, width, height
            )?);

            downsample_level_rayon(&src_store, &dst_store)?;

            levels.push(MipLevel { level, width, height, tile_size, store: dst_store.clone() });
            src_store = dst_store;
        }

        Ok(MipPyramid { levels })
    }
}

/// Downsampling 2:1 box filter — paralelo por tile via rayon.
/// Linear space (ACEScg f16) = matematicamente correto.
fn downsample_level_rayon(src: &TileStore, dst: &TileStore) -> Result<(), Error> {
    use rayon::prelude::*;

    let tiles_x = (dst.image_width + dst.tile_size - 1) / dst.tile_size;
    let tiles_y = (dst.image_height + dst.tile_size - 1) / dst.tile_size;

    (0..tiles_y).into_par_iter().try_for_each(|ty| {
        (0..tiles_x).try_for_each(|tx| {
            let coord = TileCoord::new(
                dst.mip_level, tx, ty,
                dst.tile_size, dst.image_width, dst.image_height,
            );
            let mut data = Vec::with_capacity(coord.pixel_count());

            for dy in 0..coord.height {
                for dx in 0..coord.width {
                    // Cada pixel dst = média de 4 pixels src (2× coordenadas)
                    let sx = coord.px + dx;
                    let sy = coord.py + dy;
                    let p00 = src.sample(sx * 2,     sy * 2)?;
                    let p10 = src.sample(sx * 2 + 1, sy * 2)?;
                    let p01 = src.sample(sx * 2,     sy * 2 + 1)?;
                    let p11 = src.sample(sx * 2 + 1, sy * 2 + 1)?;
                    data.push(avg4(p00, p10, p01, p11));
                }
            }

            dst.write_tile(&Tile::new(coord, data))
        })
    })
}

#[inline]
fn avg4(a: Rgba<f16>, b: Rgba<f16>, c: Rgba<f16>, d: Rgba<f16>) -> Rgba<f16> {
    // Média em f32 (sem risco de overflow com f16)
    macro_rules! avg { ($ch:ident) => {
        f16::from_f32((a.$ch.to_f32() + b.$ch.to_f32() + c.$ch.to_f32() + d.$ch.to_f32()) * 0.25)
    }}
    Rgba { r: avg!(r), g: avg!(g), b: avg!(b), a: avg!(a) }
}
```

---

## 6. Pipeline de Display — Viewport Pull Model

### Problema do push model atual

Engine faz push de todos os tiles do viewport a cada frame, mesmo que nada tenha mudado. Viewport não tem controle sobre o que pedir. Engine não sabe o que o viewport precisa com antecedência.

### Pull model

Viewport é inteligente: ele sabe o que precisa. Engine só serve quando pedido e invalida quando muda.

```
Frontend calcula viewport atual (posição, zoom → MIP level)
  ↓
Frontend calcula TileCoord[] necessários para cobrir viewport
  ↓
ws → engine: TileRequest { tab_id, coords: Vec<TileCoord> }

Engine por coord:
  → TileCache hit (sRGB u8)? responde imediatamente
  → miss? carrega ACEScg f16 do disco → converte → cacheia → responde
  ws → frontend: TileResponse { coord, data: Vec<u8> (sRGB RGBA8) }

Engine invalidação (quando gera MIPs ou no futuro quando op muda tile):
  ws → frontend: TileInvalidated { tab_id, coords: Vec<TileCoord> }
  Frontend: descarta tiles invalidados, pede novos se visíveis
```

### Frontend pode ser preemptivo

```
Pan em andamento:
  Frontend pede tiles da nova posição + 1 tile de margem ao redor

Zoom out:
  Frontend pede MIP level N para viewport
  Enquanto aguarda, pode exibir MIP N+1 upscaled (fallback visual)

Idle:
  Frontend pede tiles adjacentes ao viewport (prefetch silencioso)
```

### TileCache (engine-side)

```rust
// Em src/storage/tile_cache.rs — refactor
pub struct TileCache {
    // ACEScg f16: fonte de verdade para conversão e MIP generation
    acescg: DashMap<(Uuid, TileCoord), Arc<Vec<Rgba<f16>>>>,
    acescg_capacity: usize,

    // sRGB u8: pronto para servir ao frontend via WebSocket
    display: DashMap<(Uuid, TileCoord), Arc<Vec<u8>>>,
    display_capacity: usize,
}

impl TileCache {
    /// Retorna tile sRGB u8. Carrega e converte se necessário.
    pub async fn get_display(
        &self,
        tab_id: Uuid,
        coord: TileCoord,
        store: &TileStore,       // TileStore do MIP correto
        conv: &ColorConversion,  // ACEScg → sRGB
    ) -> Result<Arc<Vec<u8>>, Error> {
        let key = (tab_id, coord);

        // 1. Display cache hit
        if let Some(v) = self.display.get(&key) {
            return Ok(v.clone());
        }

        // 2. ACEScg cache hit → só converte para sRGB
        if let Some(acescg_tile) = self.acescg.get(&key) {
            let tile = Tile { coord, data: acescg_tile.clone() };
            let display = tile.to_srgb_u8(conv);
            let data = display.data.clone();
            self.display.insert(key, data.clone());
            return Ok(data);
        }

        // 3. Miss → carrega do disco, converte, cacheia ambos
        let acescg_tile = store.read_tile(coord)?;
        let tile = Tile::new(coord, acescg_tile);
        let display = tile.to_srgb_u8(conv);

        self.acescg.insert(key, tile.data.clone());
        self.display.insert(key, display.data.clone());
        Ok(display.data)
    }

    /// Invalida tile do display cache (ACEScg persiste para regenerar MIPs).
    /// Chama quando tile muda no disco.
    pub fn invalidate_display(&self, tab_id: Uuid, coord: TileCoord) {
        self.display.remove(&(tab_id, coord));
    }

    /// Invalida tudo de um MIP level (ex: depois de gerar MIP N).
    pub fn invalidate_mip(&self, tab_id: Uuid, mip_level: u32) {
        self.display.retain(|k, _| !(k.0 == tab_id && k.1.mip_level == mip_level));
        self.acescg.retain(|k, _| !(k.0 == tab_id && k.1.mip_level == mip_level));
    }
}
```

### Conversão ACEScg f16 → sRGB u8 (SIMD)

```rust
// Em src/convert/simd.rs
/// Converte tile ACEScg f16 premul → sRGB u8 RGBA, SIMD 4 pixels por vez.
pub fn acescg_f16_to_srgb_u8_simd(
    src: &[Rgba<f16>],
    matrix: [[f32; 3]; 3], // ACEScg → sRGB primaries
) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() * 4);

    let chunks = src.chunks_exact(4);
    let rem = chunks.remainder();

    for chunk in chunks {
        let mut r = [0.0f32; 4];
        let mut g = [0.0f32; 4];
        let mut b = [0.0f32; 4];
        let mut a = [0.0f32; 4];

        for i in 0..4 {
            let alpha = chunk[i].a.to_f32();
            a[i] = alpha;
            if alpha > 1e-6 {
                let inv = 1.0 / alpha;
                r[i] = chunk[i].r.to_f32() * inv;
                g[i] = chunk[i].g.to_f32() * inv;
                b[i] = chunk[i].b.to_f32() * inv;
            }
        }

        // Matrix via SIMD
        let rv = f32x4::from(r);
        let gv = f32x4::from(g);
        let bv = f32x4::from(b);

        let rout: [f32; 4] = (f32x4::splat(matrix[0][0]) * rv
            + f32x4::splat(matrix[0][1]) * gv
            + f32x4::splat(matrix[0][2]) * bv).into();
        let gout: [f32; 4] = (f32x4::splat(matrix[1][0]) * rv
            + f32x4::splat(matrix[1][1]) * gv
            + f32x4::splat(matrix[1][2]) * bv).into();
        let bout: [f32; 4] = (f32x4::splat(matrix[2][0]) * rv
            + f32x4::splat(matrix[2][1]) * gv
            + f32x4::splat(matrix[2][2]) * bv).into();

        for i in 0..4 {
            out.push(encode_srgb_u8(rout[i]));
            out.push(encode_srgb_u8(gout[i]));
            out.push(encode_srgb_u8(bout[i]));
            out.push((a[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }

    for px in rem {
        let a = px.a.to_f32();
        let (r, g, b) = if a > 1e-6 {
            let inv = 1.0 / a;
            (px.r.to_f32() * inv, px.g.to_f32() * inv, px.b.to_f32() * inv)
        } else {
            (0.0, 0.0, 0.0)
        };
        let lr = matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b;
        let lg = matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b;
        let lb = matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b;
        out.push(encode_srgb_u8(lr));
        out.push(encode_srgb_u8(lg));
        out.push(encode_srgb_u8(lb));
        out.push((a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }

    out
}

#[inline(always)]
fn encode_srgb_u8(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}
```

### Protocolo WebSocket (engine ↔ frontend)

```rust
// Mensagens frontend → engine
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewportRequest {
    RequestTiles {
        tab_id: Uuid,
        coords: Vec<TileCoord>,
    },
}

// Mensagens engine → frontend
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewportEvent {
    TileReady {
        tab_id: Uuid,
        coord: TileCoord,
        data: Vec<u8>, // sRGB RGBA8, tamanho = coord.width × coord.height × 4
    },
    TileInvalidated {
        tab_id: Uuid,
        coords: Vec<TileCoord>,
    },
    MipLevelReady {
        tab_id: Uuid,
        mip_level: u32, // frontend pode pedir tiles desse nível agora
    },
}
```

---

## 7. TileStore: Interface Simplificada

```rust
// Em src/storage/tile_store.rs — refactor
pub struct TileStore {
    dir: PathBuf,
    pub tile_size: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub mip_level: u32,
    // Hot cache de tiles lidos recentemente (downsampling usa sample() muito)
    hot_cache: parking_lot::RwLock<lru::LruCache<(u32, u32), Arc<Vec<Rgba<f16>>>>>,
}

impl TileStore {
    pub fn new_for_mip(tab_id: &Uuid, mip: u32, tile_size: u32, w: u32, h: u32) -> Result<Self, Error>;

    /// Lê Tile<Rgba<f16>> do disco (com hot cache).
    pub fn read_tile(&self, coord: TileCoord) -> Result<Vec<Rgba<f16>>, Error>;

    /// Escreve Tile<Rgba<f16>> no disco. Invalida hot cache para essa key.
    pub fn write_tile(&self, tile: &Tile<Rgba<f16>>) -> Result<(), Error>;

    /// Lê pixel único em coordenada absoluta.
    /// Pode cruzar fronteira de tile — usado em downsample.
    pub fn sample(&self, x: u32, y: u32) -> Result<Rgba<f16>, Error> {
        let tx = x / self.tile_size;
        let ty = y / self.tile_size;
        let local_x = x % self.tile_size;
        let local_y = y % self.tile_size;

        let key = (tx, ty);
        let tile_w = (self.image_width - tx * self.tile_size).min(self.tile_size);

        {
            let cache = self.hot_cache.read();
            if let Some(data) = cache.peek(&key) {
                return Ok(data[(local_y * tile_w + local_x) as usize]);
            }
        }

        let coord = TileCoord::new(self.mip_level, tx, ty, self.tile_size, self.image_width, self.image_height);
        let data = Arc::new(self.read_tile(coord)?);
        let px = data[(local_y * tile_w + local_x) as usize];
        self.hot_cache.write().put(key, data);
        Ok(px)
    }
}
```

---

## 8. Formato de Arquivo dos Tiles (disco)

```
tile_MIP{level}_{tx}_{ty}.raw:
  header: nenhum (dimensões inferidas pelo TileStore: coord.width × coord.height)
  body: width × height × 4 × 2 bytes
        RGBA interleaved, f16 IEEE 754, little-endian, premultiplied
        R_f16_LE | G_f16_LE | B_f16_LE | A_f16_LE | R... (próximo pixel)

Serialização:
  tile.data.iter().flat_map(|px| [
      px.r.to_bits().to_le_bytes(),
      px.g.to_bits().to_le_bytes(),
      px.b.to_bits().to_le_bytes(),
      px.a.to_bits().to_le_bytes(),
  ].into_iter().flatten()).collect::<Vec<u8>>()

Deserialização:
  bytes.chunks_exact(8).map(|c| Rgba {
      r: f16::from_bits(u16::from_le_bytes([c[0], c[1]])),
      g: f16::from_bits(u16::from_le_bytes([c[2], c[3]])),
      b: f16::from_bits(u16::from_le_bytes([c[4], c[5]])),
      a: f16::from_bits(u16::from_le_bytes([c[6], c[7]])),
  }).collect::<Vec<Rgba<f16>>>()
```

---

## 9. AppState Simplificado

```rust
// Em src/server/app.rs
pub struct AppState {
    pub tab_service: Arc<TabService>,
    pub viewport_service: Arc<ViewportService>,
    pub tile_cache: Arc<TileCache>,        // ACEScg f16 + sRGB u8, invalidável
    pub event_bus: Arc<EventBus<EngineEvent>>,
    pub conv: ConversionMatrices,          // matrizes pré-computadas no startup
}

/// Matrizes de conversão pré-computadas uma vez no startup.
/// Evita recomputar para cada tile servido.
pub struct ConversionMatrices {
    pub srgb_bt709_to_acescg: [[f32; 3]; 3],
    pub acescg_to_srgb_bt709: [[f32; 3]; 3],
    // Adicionar outros color spaces na medida que forem suportados
}

impl ConversionMatrices {
    pub fn new() -> Self {
        // ColorConversion::matrix() retorna a matriz 3×3 já calculada
        let to_acescg   = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
        let from_acescg = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
        Self {
            srgb_bt709_to_acescg: to_acescg.matrix().as_3x3_array(),
            acescg_to_srgb_bt709: from_acescg.matrix().as_3x3_array(),
        }
    }
}
```

---

## 10. Onde Modificar o Quê

### Arquivos a criar (novos)

| Arquivo | Responsabilidade |
|---------|-----------------|
| `src/convert/simd.rs` | `convert_buffer_row_to_acescg_simd`, `acescg_f16_to_srgb_u8_simd` |

### Arquivos a modificar (existentes)

| Arquivo | O que muda |
|---------|-----------|
| `src/image/tile.rs` | `TileCoord` (novo), `Tile<P>` (genérico), remover tipos ad-hoc |
| `src/storage/tile_store.rs` | `read_tile(TileCoord)`, `write_tile(&Tile<Rgba<f16>>)`, `sample(x, y)`, `new_for_mip` |
| `src/storage/tile_cache.rs` | `get_display(coord, store, conv)`, `invalidate_display`, `invalidate_mip` |
| `src/image/mip.rs` | `MipPyramid::generate_from_mip0`, `downsample_level_rayon` |
| `src/io/png.rs` | Remove lógica de tile; `load_png` só retorna `Arc<ImageBuffer>` |
| `src/io/mod.rs` | `stream_image_to_tiles(source: Arc<ImageBuffer>, store: &TileStore)` |
| `src/server/app.rs` | `AppState` com `TileCache` + `ConversionMatrices`, remove `op_executor` |
| `src/server/service/tab.rs` | `open_image` chama `stream_image_to_tiles` + `generate_from_mip0` em background |
| `src/server/service/viewport.rs` | Handlers para `RequestTiles`; emite `TileReady`, `TileInvalidated`, `MipLevelReady` |
| `src/server/ws/types.rs` | Adiciona `ViewportRequest`, `ViewportEvent` ao protocolo |

### Arquivos a deletar/simplificar

| Arquivo | O que acontece |
|---------|---------------|
| `src/storage/source.rs` | `ImageSource` / `PngSource` podem simplificar: só precisam retornar `Arc<ImageBuffer>` |
| `src/image/buffer.rs` | `BandBuffer` deletado (substituído por banda na própria `stream_image_to_tiles`) |
| `src/convert/pipeline.rs` | Funções antigas de conversão são mortas; manter só helpers usados pelos testes |

---

## 11. Sequência de Implementação

### Passo 1 — `TileCoord` + `Tile<P>` genérico (`src/image/tile.rs`)
Definir structs. Sem lógica ainda.  
Testar: `TileCoord::new`, `Tile<Rgba<f16>>::to_srgb_u8` com pixel sintético.

### Passo 2 — `TileStore` refatorado (`src/storage/tile_store.rs`)
Implementar `read_tile(TileCoord)`, `write_tile(&Tile<Rgba<f16>>)`, `sample(x, y)`.  
Serialização f16 LE. Hot cache com `lru::LruCache`.  
Testar: write + read round-trip, sample cross-tile boundary.

### Passo 3 — SIMD conversions (`src/convert/simd.rs`)
Implementar `convert_buffer_row_to_acescg_simd` (scalar first, SIMD depois) e `acescg_f16_to_srgb_u8_simd`.  
Testar: compare resultado contra referência escalar pixel-a-pixel.

### Passo 4 — `stream_image_to_tiles` (`src/io/mod.rs`)
Substituir `stream_png_to_tiles_sync`. Testar com `example1.png` → verifica tiles no disco.

### Passo 5 — MIP generation (`src/image/mip.rs`)
Implementar `generate_from_mip0`. Testar: imagem 512×512 → MIP 1 = 256×256 = 1 tile.  
Verificar: pixel médio do MIP 1 ≈ média dos 4 pixels do MIP 0.

### Passo 6 — `TileCache` (`src/storage/tile_cache.rs`)
Implementar `get_display`, `invalidate_display`, `invalidate_mip`.  
Testar: cache miss → lê disco. Cache hit → não lê disco.

### Passo 7 — Pull model no servidor (`src/server/service/viewport.rs`)
Handler `RequestTiles` → `TileCache::get_display` → `TileReady` event.  
Emite `MipLevelReady` quando `generate_from_mip0` conclui.  
Testar: abre imagem, pede tile via WebSocket, recebe bytes corretos.

### Passo 8 — Frontend pull model (`pixors-ui/src/engine/`)
Frontend calcula `TileCoord[]` para viewport atual, envia `RequestTiles`.  
Exibe tiles conforme chegam. Descarta tiles em `TileInvalidated`.  
Testar: pan e zoom sem glitch, tiles aparecem progressivamente.

---

## 12. Estimativas de Performance (4K, 16 tiles visíveis)

| Operação | Antes | Depois |
|----------|-------|--------|
| Abertura (PNG → tiles) | 500ms | ~150ms (SIMD + agnóstico) |
| MIP generation (background) | nenhum funcional | ~300ms (rayon) |
| `MipLevelReady` emitido após | N/A | ~450ms total |
| Display cache miss → tile | 10ms | ~1ms (SIMD convert) |
| Display cache hit → tile | 10ms | <0.1ms (Arc clone) |
| Viewport pan: tiles visíveis | push todo frame | pull on-demand |
| Prefetch: tiles vizinhos | nenhum | frontend solicita em idle |

---

## Apêndice: Notas de Implementação

### f16 vs f32 para storage

Tiles em f16 = 128KB por tile 256×256 (RGBA). f32 = 256KB.  
f16 tem ~3 dígitos de precisão (10 bits mantissa). Suficiente para display e armazenamento.  
Operações futuras: leia como f32 (`tile.to_f32_straight()`), processe, salve de volta em f16.

### Por que não comprimir tiles no disco?

zstd em tile fotográfico: ~30-50%. Economiza ~200MB para 4K inteiro.  
Mas decode zstd = ~5-10ms por tile. Para scrubbing rápido, custo alto.  
Compressão faz sentido só quando imagem > RAM disponível (>100MP).

### Por que rayon e não tokio para MIP generation?

tokio é para I/O bound (read/write disco, rede). CPU-bound no tokio bloqueia o runtime.  
rayon = work-stealing, ideal para loops de pixel. Integração: `tokio::task::spawn_blocking` → rayon.

### Alpha handling

Tiles no disco: premultiplied alpha.  
Conversão para display (to_srgb_u8): desafa premul antes de aplicar matrix, repremultiplica alpha no output u8.  
Operações futuras: `to_f32_straight()` remove premul. Resultado salvo com premul.
