# Phase 8 — Stream Pipeline Review

> Análise crítica da nova arquitetura `src/stream/` (TileStream → Pipes → Sinks)
> Audiência: IA implementadora que vai consolidar Phase 8 antes de Phase 9.
> Escopo: bugs, simplificações, ganhos de paralelismo. Sem bagagem do código legado.
>
> **Revisão 2** — corrige itens apontados em revisão cruzada: descartadas propostas
> de `Bytes`/`PixelLayout`/`coord-em-meta`. Mantém core: bug fixes, MipPipe
> recursivo, dois ramos pós-tee, sinks triviais.

---

## TL;DR

Pipeline de `Frame` sobre `mpsc::channel` + threads é conceitualmente correta. Problemas:

1. **3 bugs reais**: frame-drop em edge tile do `MipPipe`, `tee` derruba consumidores saudáveis, `disk_handle` órfão em `close_image`.
2. **MipPipe chain instável**: o `target_src_mip` faz filtro por nível, mas é workaround frágil — pequena alteração na ordem de emissão quebra. Solução: pipe recursivo único.
3. **Paralelismo subutilizado**: cada estágio é single-thread. `WorkingSink` faz a conversão de cor mais cara dentro do consumidor (deveria ser pipe, não sink).
4. **MIP ACEScg duplicada**: `MipPipe` produz só MIPs sRGB pro Viewport; ACEScg ainda usa `generate_from_mip0` offline. Ramo pós-tee resolve.

Recomendação: refatorar `stream/` antes de Phase 9.

---

## 1. Inventário rápido

```
src/stream/
├── frame.rs   Frame { meta, kind: Tile{coord}/LayerDone/MipLevelDone/StreamDone, data: Cow<[u8]> }
├── pipe.rs    trait Pipe; chain(); tee(); map(); flat_map()
├── source.rs  TileStreamNew::open() — thread decode + emite Frames
├── color.rs   ColorConvertPipe — converte u8 source → u8 sRGB ou ACEScg
├── mip.rs     MipPipe { tile_size, target_src_mip } — acumula 2×2, emite mip+1
└── sink.rs    Viewport (cache RAM), ViewportSink, WorkingSink
```

Pipeline em `tab.rs::open_image` (~L385):

```rust
rx = TileStreamNew::open(path, tile_size)?;          // 1 thread
let (vp_rx, wk_rx) = tee(rx, 2);                     // 1 thread fan-out
vp_rx = ColorConvertPipe(src→sRGB).pipe(vp_rx);      // 1 thread
for mip in 0..N { vp_rx = MipPipe::new(ts, mip).pipe(vp_rx); }  // N threads
ViewportSink.run(vp_rx);                              // 1 thread (RAM HashMap)
WorkingSink.run(wk_rx);                               // 1 thread (faz u8→f16 inline!)
```

---

## 2. Bugs

### 2.1 [CRÍTICO] `MipPipe` engole frames originais em edge case

`stream/mip.rs:81` e `:147` têm `continue;` / `return;` dentro do bloco `if complete { … }` **antes** do pass-through em `:156`:

```rust
if actual_w == 0 || actual_h == 0 { continue; }    // L81 — pula tx.send(frame) abaixo
…
if tx.send(Frame::new(…)).is_err() { return; }     // L147 — idem
```

Resultado: tile de destino degenerado (dim ímpar perto da borda final, `image_w >> 1 == 0`) → tile **original** desaparece do stream. Buracos no Viewport.

**Fix**: nunca pular o pass-through.

```rust
if complete {
    let actual_w = …; let actual_h = …;
    if actual_w > 0 && actual_h > 0 {
        // …downsample, build out_frame, tx.send(out_frame)…
    }
    entry[0] = None; entry[1] = None; entry[2] = None; entry[3] = None;
}
if tx.send(frame).is_err() { return; }   // pass-through SEMPRE
```

### 2.2 [CRÍTICO] `tee()` mata consumidores saudáveis

`stream/pipe.rs:50-55`: ao primeiro `send` falhar, `break` aborta o for, **deixando demais consumidores sem o frame**. Pior: o duplo loop (`frames` vec + outro for de envio) clona payload N vezes — `frame.clone()` no for de coleta + `frames[i].clone()` no envio — completamente sem propósito.

**Fix**: `Vec<Option<Sender>>`; consumidor caído vira `None`, prossegue com os demais.

```rust
pub fn tee(rx: mpsc::Receiver<Frame>, n: usize) -> Vec<mpsc::Receiver<Frame>> {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..n).map(|_| mpsc::channel()).unzip();
    std::thread::spawn(move || {
        let mut txs: Vec<_> = txs.into_iter().map(Some).collect();
        while let Ok(frame) = rx.recv() {
            for slot in txs.iter_mut() {
                if let Some(tx) = slot {
                    if tx.send(frame.clone()).is_err() { *slot = None; }
                }
            }
            if txs.iter().all(Option::is_none) { break; }
        }
    });
    rxs
}
```

### 2.3 `disk_handle` órfão em `close_image`

`tab.rs::close_image` faz `self.layers.clear();` sem joinar `disk_handle`. Threads `WorkingSink` continuam escrevendo contra `WorkingWriter` cujo `Drop` apaga `base_dir` (`auto_destroy: true`). Race: `std::fs::write` em path apagado.

**Fix**:
```rust
impl Drop for LayerSlot {
    fn drop(&mut self) {
        if let Some(h) = self.disk_handle.take() { let _ = h.join(); }
    }
}
```

Ou shutdown-token (`Arc<AtomicBool>`) para encerrar o thread imediatamente sem esperar todos os tiles.

### 2.4 `MipPipe` chain — `target_src_mip` é workaround frágil

**Nota**: o campo `target_src_mip` existe (`stream/mip.rs:12`) e funciona — filtra por nível para que pipe-i só acumule mip-i. **Não é alucinação** — é workaround real e funcional. Mas é frágil:

- Cada `MipPipe` na chain processa **todos** os frames pra fazer pass-through, mas só acumula no quadrante quando `mip == target_src_mip`. Frames de outros níveis viajam por todos os pipes em sequência.
- `tab.rs` precisa saber `num_levels` antecipadamente (lógica duplicada de `MipPyramid::new`).
- Custo: cada tile mip-0 trafega por N threads, com clone em cada pipe (Cow::Owned). Em chain de 6 pipes, 6× clone do payload.
- Se `LayerDone` chegar e algum quadrante estiver incompleto (deveria ser impossível, mas), o estado vaza pro próximo loop.

**Fix recomendado**: um único pipe recursivo (§4.3). Elimina chain, elimina `target_src_mip`, elimina `for mip in 0..N` em `tab.rs`.

### 2.5 `WorkingSink` engole erros silenciosamente

`stream/sink.rs:101` loga `tracing::error!` e segue. Disco cheio / permissão negada → app continua "carregando" com tiles faltando. Nenhum sinal pra cima.

**Fix**: `JoinHandle<Result<u32, Error>>` (tiles escritos com sucesso). Caller decide.

### 2.6 Source reconstrói `TileCoord` por contador

`stream/source.rs:62-64`:

```rust
let tx_tile = (emitted % tiles_x) as u32;
let ty_tile = emitted / tiles_x;
```

Assume raster-scan. `StreamWriterNew::write_tile` recebe `coord: TileCoord` mas descarta (`_coord`). Se reader emitir fora de ordem (TIFF tiled, JPEG-2000 futuro) → corrupção silenciosa.

**Fix**: canal `mpsc::channel::<(TileCoord, Vec<u8>)>`; consumidor usa coord recebido.

### 2.7 `ColorConvertPipe` deriva dimensão por divisão

`stream/color.rs:32-34`:
```rust
let actual_pixels = src.len() / bpp.max(1);
let tile_h = actual_pixels / tile_w.max(1);
```

`coord` já tem `width` e `height`. Se `data.len()` tiver padding ou for inconsistente, `tile_h` vira lixo silencioso.

**Fix**: usar `coord.width × coord.height`; assertar `data.len() == w*h*bpp`.

---

## 3. Onde o paralelismo está sendo desperdiçado

### 3.1 Cada Pipe é fila linear single-thread

| Stage | Threads | Concurrent tiles |
|------|---------|------------------|
| Source decode | 1 | 1 (PNG decode é serial mesmo) |
| ColorConvert | 1 | 1 |
| MipPipe (×N) | N | 1 cada |
| ViewportSink | 1 | 1 |
| WorkingSink | 1 (faz conv u8→f16 inline!) | 1 |

CPU multi-core fica ociosa enquanto cada thread processa um tile por vez.

### 3.2 `WorkingSink` faz a conversão mais cara dentro do consumidor

`storage/writer.rs::WorkingWriter::write_tile<u8>` (L179-201) chama `conv.convert_buffer(pixels, &d, AlphaPolicy::PremultiplyOnPack)` por tile, sequencial. Viola "Sink = dreno".

**Fix**: separar em pipe + sink:

```
… → ColorConvertPipe<u8 → f16 ACEScg premul> → WorkingSink (apenas serializa LE + write)
```

`WorkingSink` fica I/O-bound, conversão paraleliza com tudo. Bônus: `WorkingWriter` perde `Option<ColorConversion>` / `Option<BufferDesc>` / método `new_with_conversion`.

### 3.3 MIP em ACEScg duplicada

Hoje:
- `MipPipe` calcula MIPs em sRGB-u8 só pro Viewport.
- `MipPyramid::generate_from_mip0` (rayon, lendo disco) recalcula MIPs em ACEScg-f16 quando user dá zoom out.

Primeira vez que user puxa zoom slider, espera `ensure_mip_level`. Podia ter sido feito durante load.

**Fix arquitetural**: dois ramos pós-tee, cada um com seu `MipPipe`:

```
source ─┬─ ColorConvertPipe<u8→u8 sRGB Straight> ─ MipPipe ─→ ViewportSink   (RAM, display)
        └─ ColorConvertPipe<u8→f16 ACEScg PremulOnPack> ─ MipPipe ─→ WorkingSink  (disk, working)
```

`MipPipe` precisa ser genérico em bytes-per-pixel (atualmente assume RGBA8 — `out[oi+3] = (a/div) as u8`). Generalizar com closure de "average de 4 amostras" ou gerar duas variantes (u8/f16).

Mata `generate_from_mip0` e `ensure_mip_level`. MIPs prontas no fim do load.

### 3.4 Source (PNG) é serial — paralelizar pós-decode

PNG decoder lê scanlines em sequência (zlib streaming). Não dá pra paralelizar `next_row`. Mas tile pronto pode despachar conversão + downsample em paralelo.

**Fix simples**: `ColorConvertPipe` vira `par_map` com pool rayon:

```rust
pub fn par_map<F>(rx, threads: usize, f: F) -> mpsc::Receiver<Frame>
where F: Fn(Frame) -> Frame + Send + Sync + 'static
```

Implementação: `rayon::ThreadPool` privado, ordering preservada via FIFO buffer (reorder por seq#) ou simplesmente "a ordem não importa pro Viewport/disk".

### 3.5 MipPipe recursivo evita chain de threads

Single thread acumulando + fazendo downsample. Em chain de 6, tile mip-0 só termina quando passou pelos 6 threads. Recursivo (§4.3) faz tudo num thread só com pool interno pro downsample — menos context-switch, menos clones.

---

## 4. Refatoração proposta

### 4.1 Tipos centrais — manter

`Frame { meta, kind, data: Cow<'static, [u8]> }` fica como está. `Cow` é suficiente: `Borrowed` para pass-through barato, `Owned` quando pipe muta. Nada de `Bytes` (dependência nova sem ganho real).

`FrameKind::Tile { coord }` fica como está. Tipagem mais segura — só tiles têm coord; `LayerDone`/`StreamDone` são markers puros.

`FrameMeta { layer_id, mip_level, image_w, image_h, color_space, total_tiles }` fica. `ColorSpace` + conhecimento "RGBA u8" basta — sem novo enum `PixelLayout`.

### 4.2 `Pipe` trait — manter, adicionar `par_map`

```rust
pub trait Pipe: Send + 'static {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame>;
}

pub fn par_map<F>(rx: mpsc::Receiver<Frame>, threads: usize, f: F) -> mpsc::Receiver<Frame>
where F: Fn(Frame) -> Frame + Send + Sync + 'static;
```

Remover `flat_map`, `map`, `chain` se não tiverem uso (`grep -rn "flat_map(" src/` confirma).

### 4.3 `MipPipe` único, recursivo

```rust
pub struct MipPipe { tile_size: u32, max_levels: u32 }

impl Pipe for MipPipe {
    fn pipe(self, rx) -> rx {
        // Estado: HashMap<(layer_id, src_mip, dst_tx, dst_ty), [Option<Frame>;4]>
        // Para cada Tile recebido:
        //   1. tx.send(frame.clone())                            // pass-through
        //   2. se frame.meta.mip_level < max_levels:
        //        compute (src_mip, dst_tx, dst_ty, qi)
        //        acumula no quadrante
        //        se 2×2 completo:
        //          downsample (rayon::spawn pra liberar thread principal)
        //          push frame mip+1 numa fila local
        //          loop volta a (1) com esse frame
    }
}
```

Vantagens:
- 1 pipe (não N), 1 thread (não 6).
- Sem `target_src_mip`.
- `tab.rs` perde `for mip in 0..num_levels`.
- LayerDone descarta quadrantes incompletos com warn.
- Genérico em formato (recebe `bpp` e função "average de N").

### 4.4 `ColorConvertPipe` rayon-paralelo

```rust
pub struct ColorConvertPipe { conv: Arc<ColorConversion>, src_desc: BufferDesc, threads: usize }

impl Pipe for ColorConvertPipe {
    fn pipe(self, rx) -> rx {
        par_map(rx, self.threads, move |mut f: Frame| {
            if let FrameKind::Tile { coord } = f.kind {
                let dst = convert_tile(&self.conv, &self.src_desc, coord, &f.data);
                f.data = Cow::Owned(dst);
                f.meta.color_space = self.conv.dst();
            }
            f
        })
    }
}
```

Roda paralelo entre tiles — usa todos os cores disponíveis.

### 4.5 Sinks ficam triviais

```rust
impl TileSink for ViewportSink {
    fn run(&self, rx) -> JoinHandle<Result<u32,Error>> {
        let vp = Arc::clone(&self.viewport);
        std::thread::spawn(move || {
            let mut n = 0;
            while let Ok(f) = rx.recv() {
                match f.kind {
                    FrameKind::Tile { coord } => {
                        vp.put(f.meta.mip_level, coord, Arc::new(f.data.into_owned()));
                        n += 1;
                    }
                    FrameKind::StreamDone => break,
                    _ => {}
                }
            }
            vp.mark_ready();
            Ok(n)
        })
    }
}

impl TileSink for WorkingSink {
    fn run(&self, rx) -> JoinHandle<Result<u32,Error>> {
        let store = Arc::clone(&self.store);
        std::thread::spawn(move || {
            // só serializa LE + std::fs::write — sem conversão!
            …
        })
    }
}
```

`WorkingWriter` perde `conv: Option<ColorConversion>`, `desc: Option<BufferDesc>`, `new_with_conversion`. Vira "serializa f16 RGBA premul → arquivo". Mais simples, sem `Option`.

### 4.6 Pipeline final em `tab.rs::open_image`

```rust
let rx = TileStreamNew::open(path, tile_size)?;
let mut rx_vec = tee(rx, 2);
let wk_rx = rx_vec.pop().unwrap();
let vp_rx = rx_vec.pop().unwrap();

// Ramo Viewport (display, sRGB u8)
let vp_rx = ColorConvertPipe::new(src_cs, ColorSpace::SRGB, src_desc.clone(), num_cpus::get()).pipe(vp_rx);
let vp_rx = MipPipe::new(tile_size, max_levels).pipe(vp_rx);

// Ramo Working (disk, ACEScg f16 premul)
let wk_rx = ColorConvertPipe::new(src_cs, ColorSpace::ACES_CG, src_desc.clone(), num_cpus::get()).pipe(wk_rx);
let wk_rx = MipPipe::new(tile_size, max_levels).pipe(wk_rx);

let vp_h = ViewportSink::new(viewport).run(vp_rx);
let wk_h = WorkingSink::new(store).run(wk_rx);

layer.disk_handle = Some(wk_h);
layer.vp_handle  = Some(vp_h);
```

### 4.7 Backpressure: `sync_channel(N)`

Hoje todos os canais são unbounded. Imagem 16K × 16K = 4096 tiles × 256KB = potencialmente 1GB em buffer.

**Fix**: `mpsc::sync_channel(64)`. `tee` lida com bloqueio naturalmente.

---

## 5. Limpezas independentes do refactor

### 5.1 Código morto

- `storage/writer.rs::DisplayWriter` — substituído por `Viewport`. Remover.
- `storage/writer.rs::FanoutWriter` — não usado fora de teste. Remover (e teste junto).
- `storage/writer.rs::WorkingWriter::new_with_subdir` — só `mip.rs` usa; `MipLevel::new` pode usar `new` com path já construído.
- `Frame::take_data` — não chamado em lugar algum. Remover.
- `Viewport::layer_id` — sempre 0; `LayerSlot` já tem `id: Uuid`. Remover do tuple-key.
- `is_generating_mips: AtomicBool`, `ensure_mip_level`, `ensure_mip_level_blocking`, `MipPyramid::generate_from_mip0`, `downsample_level_rayon` — todos morrem com §4.6.
- `pipe::flat_map`, `pipe::map`, `pipe::chain` — `grep` confirma não usado.

### 5.2 Logs ruidosos

`stream/mip.rs:168`: `tracing::debug!("mip_pipe: finished, tiles emitted: {:?}", &mip_tiles_emitted[..8]);` — array fixo 16 sempre printando 8. Remover ou condicional.

### 5.3 LayerSlot::Drop joina handles

```rust
impl Drop for LayerSlot {
    fn drop(&mut self) {
        if let Some(h) = self.disk_handle.take() { let _ = h.join(); }
    }
}
```

---

## 6. Plano de execução para a IA implementadora

Ordem importa. Cada step é deployable. Validar com `cargo test --workspace` + abrir PNG no UI.

**Step 1** — Bug fixes (sem mudança arquitetural):
- 2.1 (frame drop em MipPipe edge — `tx.send(frame)` sempre executa)
- 2.2 (tee resilient — `Vec<Option<Sender>>`)
- 2.3 (close_image / LayerSlot::Drop join handles)
- 2.6 (passar coord no canal de source)
- 2.7 (ColorConvertPipe usa coord.width/height)

**Step 2** — Limpeza morta:
- §5.1 (DisplayWriter, FanoutWriter, take_data, layer_id, dead helpers, dead pipe combinators)
- §5.2 (logs)

**Step 3** — Single recursive MipPipe:
- §4.3. Apaga `target_src_mip` e o `for mip in 0..num_levels` em `tab.rs`.
- Generalizar pra aceitar bpp (preparação pra Step 4).

**Step 4** — Working branch via pipe (não sink):
- `ColorConvertPipe` aceita destino f16 (já aceita; só passar `ACES_CG` em vez de `SRGB`).
- `WorkingWriter::write_tile<u8>` desaparece — vira `write_tile_f16` chamado direto pelo `WorkingSink`.
- `WorkingWriter` perde `conv`/`desc`.
- `tab.rs` ganha 2 ramos pós-tee (§4.6).

**Step 5** — `ensure_mip_level` morre:
- MIPs ACEScg prontas no load via Step 4.
- `tab.is_mip_ready` checa via `mip_pyramid.level(n).generated` setado pelo `WorkingSink`.
- Apaga `generate_from_mip0`, `downsample_level_rayon`, `is_generating_mips`, `ensure_mip_level*`.

**Step 6** — Paralelismo intra-pipe (`par_map`):
- ColorConvertPipe vira par-map com `num_cpus::get()`.
- Bench rápido com PNG 8K antes/depois.

**Step 7** — Backpressure:
- `sync_channel(64)`.

**Critérios de aceite**:
- Step 1-2: `cargo test` + PNG 4K renderiza sem buracos.
- Step 3: zoom-out em PNG 8K mostra MIPs corretas.
- Step 4-5: tempo "open PNG 8K → primeiro pixel" ≤ atual; zoom-out instantâneo (sem hitch).
- Step 6: `time` open PNG 16K cai ≥ 2× em 8-core.
- Step 7: stress PNG 32K não estoura RAM.

---

## 7. Pontos abertos / decisões a tomar

- **Cancelamento**: nenhum pipe escuta cancellation token. `close_image` mid-load → threads continuam até `StreamDone` natural; `JoinHandle` join no Drop bloqueia caller. Considerar `Arc<AtomicBool>` shutdown flag global.
- **Métricas**: nenhum pipe expõe contadores estruturados. Útil pra Phase 9 (devtools "3/12 tiles loaded"). Adicionar `Arc<AtomicU32> tiles_in / tiles_out` em cada pipe.
- **Compositor**: hoje lê de `WorkingWriter` (disk f16) via `sample()`. Não tocado neste review. Pós-Phase 8 candidato a virar `CompositeSink`.
- **`MipPipe` genérico em formato**: implementação atual hardcoded RGBA8 (`out[oi+3] = (a/div) as u8`). Generalizar com closure de average ou gerar 2 variantes monomorfizadas (u8/f16) — Step 3 precisa decidir.

---

## 8. Resumo executivo das decisões

| Decisão | Razão |
|---------|-------|
| Manter `Cow<[u8]>` no Frame | suficiente; `Bytes` é dependência sem ganho real |
| Manter `FrameKind::Tile { coord }` | tipagem mais segura — só tiles têm coord |
| Manter `ColorSpace` (sem novo `PixelLayout`) | complexidade prematura |
| Single recursive MipPipe | mata `target_src_mip` e chain; menos clones, menos threads |
| WorkingWriter sem `conv`/`desc` | sinks viram I/O puro; conversão paraleliza num pipe |
| 2 ramos pós-tee (display + working) | MIPs prontas no load, mata `generate_from_mip0` |
| `par_map` rayon em ColorConvert | usa todos os cores; PNG decode é único bottleneck serial inerente |
| `sync_channel(N)` | backpressure barata, evita OOM em imagens grandes |
| `tee` resiliente (Vec<Option<Sender>>) | um consumidor lento/quebrado não derruba os outros |

Tudo é pequeno em LOC, não muda APIs públicas (CLI/WS), desbloqueia caminho pra Phase 9 sem dívida arquitetural.
