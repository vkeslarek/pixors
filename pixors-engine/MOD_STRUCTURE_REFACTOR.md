# MOD_STRUCTURE_REFACTOR — Avaliação e Propostas

## Estrutura Atual (snapshot)

```
src/
├── lib.rs
├── approx.rs            # (não-pub, módulo morto)
├── error.rs
├── utils.rs             # tracing init + DebugStopwatch + macro
├── color/               # primaries, transfer, matrix, conversion, …
├── pixel/               # rgba, rgb, gray, format, accumulator, pack, …
├── container/           # Tile, ScanLine, Neighborhood, meta, access
├── storage/             # Buffer { Cpu(Arc<Vec>), Gpu(GpuBuffer) }
├── gpu/                 # wgpu context + GpuBuffer + kernels/blur.wgsl
└── pipeline/
    ├── state/           # StateNode variants (FileImage, Blur, Export, …)
    ├── sgraph/          # StateGraph + actions + history + builder + compile
    ├── exec/            # ExecStage variants + runners (12 arquivos)
    └── egraph/          # ExecGraph + Stage trait + executor + emitter + item
```

---

## 1. O Que Ficou Bom

### 1.1 Umbrella `pipeline/`
Agrupar `state`, `sgraph`, `exec`, `egraph` sob `pipeline/` deu coesão de
domínio. Quem mexe em pipeline não precisa caçar arquivos por todo o crate.

### 1.2 Separação Variants ↔ Graph
`state/` (variantes user-facing) separado de `sgraph/` (grafo + compile +
history) é a divisão certa: cada arquivo de variante (ex: `state/blur.rs`)
ficou pequeno e focado. Idem `exec/` ↔ `egraph/`.

### 1.3 `enum_dispatch` para `StateNodeTrait` e `Stage`
**Decisão excelente.** O antigo `match` gigante em `compile.rs` e em
`stage.rs` virou trait com despacho automático. Adicionar nova variante
agora exige:
- 1 arquivo novo em `state/` ou `exec/`
- 1 linha no enum `StateNode` / `ExecStage`
- 1 linha no `mod.rs`

Sem `match` central pra manter sincronizado. Ganho real de manutenção.

### 1.4 Um arquivo por variante
`state/blur.rs`, `state/export.rs`, `exec/blur_kernel.rs`, etc. Diffs
limpos, ownership óbvio, fácil de reorganizar depois. Em vez de um
`stages.rs` de 1k linhas.

### 1.5 `gpu/` como capacidade horizontal
`gpu/` fora de `pipeline/` (sibling) — correto. wgpu device é runtime,
não pertence ao domínio de pipeline. Stages que precisam GPU importam.

---

## 2. Problemas Identificados

### 2.1 Acoplamento Backwards: `state/` ↔ `sgraph/`
`pipeline/sgraph/node.rs` define o trait `StateNodeTrait` e o enum
`StateNode`, mas precisa importar TODAS as variantes de `state/`:

```rust
// pipeline/sgraph/node.rs
use crate::pipeline::state::*;
#[enum_dispatch(StateNodeTrait)]
pub enum StateNode { FileImage, Blur, DiskCache, DisplayCache, Export }
```

E cada variante em `state/` importa o trait de volta:

```rust
// pipeline/state/blur.rs
impl crate::pipeline::sgraph::node::StateNodeTrait for Blur { … }
```

Isso é dependência circular conceitual. Quem é o "dono" do contrato?
Hoje fica no consumidor (`sgraph`), mas as variantes são naturalmente o
produtor.

**Mesmo problema**: `pipeline/exec/*.rs` impl `Stage` que vive em
`pipeline/egraph/stage.rs`.

### 2.2 Nomenclatura Inconsistente: `sgraph`/`egraph` vs `state`/`exec`
- Variantes: nomes por extenso (`state`, `exec`).
- Grafos: abreviações jargão (`sgraph`, `egraph`).

Ler o tree não revela imediatamente que `sgraph` = "state graph". Pra novo
contribuidor, perde tempo decifrando.

### 2.3 Star Re-export em `state/mod.rs` e Uso `use state::*`
```rust
// pipeline/state/mod.rs
pub use blur::Blur;
pub use disk_cache::DiskCache;
…
// pipeline/sgraph/node.rs
use crate::pipeline::state::*;
```

Glob import sai barato hoje (5 variantes), mas:
- Conflitos silenciosos quando crescer
- IDE "go to definition" piora
- Esconde de onde vem cada tipo

### 2.4 `storage::Buffer` Importa `gpu::GpuBuffer`
```rust
// storage/buffer.rs
pub use crate::gpu::buffer::GpuBuffer;
pub enum Buffer { Cpu(Arc<Vec<u8>>), Gpu(GpuBuffer) }
```

Conceitualmente `storage` é mais "baixo" que `gpu`, mas depende dele.
Inverte a ordem natural. Resultado: tirar `gpu/` quebra `storage/`. Se
amanhã quisermos `--no-default-features` sem GPU, não dá.

### 2.5 CPU + GPU no Mesmo Arquivo (`exec/blur_kernel.rs` = 448 linhas)
Hoje funciona porque ambas variantes são pequenas. Mas:
- `BlurKernel` (CPU) com helpers `box_blur_rgba8` + `blur_axis`
- `BlurKernelGpu` com pipeline wgpu, encoder batching, params struct

Convivem mas têm dependências bem diferentes (`half`/`wide` vs
`wgpu`/`bytemuck`). Quando o blur GPU ganhar variantes (separable,
gaussian), o arquivo explode.

### 2.6 Tests Espalhados Sem Padrão
- `pipeline/sgraph/tests.rs` (módulo-nível, todos asserts juntos)
- `gpu/tests.rs` (módulo-nível)
- Nada inline `#[cfg(test)] mod tests` dentro de cada `state/*.rs` ou
  `exec/*.rs`

Sem regra clara, futuro contribuidor improvisa.

### 2.7 `approx.rs` — Código Morto
`mod approx;` privado, trait `ApproximateEq` nunca usado. Já gera warning
clippy. Deletar ou ativar.

### 2.8 Sem `prelude` na API Pública
Caller externo precisa de paths longos:
```rust
use pixors_engine::pipeline::sgraph::compile::{compile, ExecutionMode};
use pixors_engine::pipeline::sgraph::builder::PathBuilder;
use pixors_engine::pipeline::state::{FileImage, Blur, Export};
```

Com 5+ módulos pra usar uma simples pipeline, a UX da API afasta.

### 2.9 `color/` e `pixel/` Flat Demais
9 arquivos em `color/`, 8 em `pixel/`. Não é problema agora, mas vai
inchar quando entrar mais espaço de cor / mais primitivos. Sub-grupos
(`color/space/`, `color/transfer/`, `pixel/primitive/`,
`pixel/sampler/`) ficariam bem.

### 2.10 `Stage` Trait Define Default Errors Pra Tudo
```rust
pub trait Stage {
    fn source_runner(&self) -> Result<…> { Err("not a source") }
    fn op_runner(&self) -> Result<…> { Err("not an operation") }
    fn sink_runner(&self) -> Result<…> { Err("not a sink") }
}
```

Funciona, mas o tipo não te diz se um stage é Source/Op/Sink antes de
chamar. O `Executor` faz tentativa-e-erro pra descobrir. Type-safer:
três traits separadas + tag enum.

---

## 3. Propostas Priorizadas

### P1 (alta prioridade — pequena, fix conceitual)

#### P1.1 — Mover trait pra junto das variantes
**Antes:**
```
pipeline/sgraph/node.rs    # define StateNodeTrait + enum StateNode
pipeline/state/blur.rs     # impl trait
```
**Depois:**
```
pipeline/state/mod.rs      # define StateNodeTrait + enum StateNode + ExpandCtx + ExpansionOption
pipeline/state/blur.rs     # impl trait
pipeline/sgraph/           # só lida com grafo: actions, history, builder, compile, graph, ports, cache
```

Mesma transformação para `exec/` ↔ `egraph/`:
```
pipeline/exec/mod.rs       # define Stage trait + enum ExecStage + Device
pipeline/exec/blur_kernel.rs
pipeline/egraph/           # só ExecGraph + Executor + Emitter + Item + runner traits
```

**Por quê:** elimina dependência circular conceitual. Variantes são donas
do trait; grafo só consome.

#### P1.2 — Renomear `sgraph` → `state_graph`, `egraph` → `exec_graph`
Por consistência com `state` / `exec`. Imports ficam um pouco mais
longos mas auto-documentam.

Alternativa: manter `sgraph`/`egraph` mas adicionar comment no `mod.rs`.

#### P1.3 — Deletar `approx.rs`
Já dá warning. Sem callers. Sem custo de remoção.

### P2 (média prioridade — refactor localizado)

#### P2.1 — `Buffer` migra pra `gpu/` (ou mais radical: `storage/` morre)
**Opção A** (conservadora): mover `Buffer` para `gpu/buffer.rs` ao lado
de `GpuBuffer`. `storage/` vira só re-export ou some.

**Opção B** (futura — feature gate): isolar GPU atrás de
`#[cfg(feature = "gpu")]`. Buffer fica:
```rust
pub enum Buffer {
    Cpu(Arc<Vec<u8>>),
    #[cfg(feature = "gpu")] Gpu(GpuBuffer),
}
```
Permite build CPU-only.

#### P2.2 — Splittar `exec/blur_kernel.rs`
```
pipeline/exec/blur_kernel/
├── mod.rs          # re-exports + (opcional) shared helpers
├── cpu.rs          # BlurKernel + box_blur_rgba8 + blur_axis
└── gpu.rs          # BlurKernelGpu + Params + encoder batching
```

Padrão replicável: stage com versões CPU+GPU vira diretório.

#### P2.3 — Sumir com star re-export
```rust
// pipeline/state/mod.rs
pub use blur::Blur;
pub use disk_cache::DiskCache;
…
```
Manter as exports nominais (já tá assim), mas no callsite trocar
`use crate::pipeline::state::*;` por imports nomeados:
```rust
use crate::pipeline::state::{Blur, DiskCache, DisplayCache, Export, FileImage};
```

Verboso, mas previne conflito futuro e melhora navegação.

#### P2.4 — Convenção de tests
Decidir e aplicar:
- **Testes unitários** de uma variante: `#[cfg(test)] mod tests` no
  fim do próprio arquivo da variante (ex: `state/blur.rs` testa Blur).
- **Testes de integração entre módulos** (compile, executor end-to-end):
  ficam em `pipeline/sgraph/tests.rs` ou criar `tests/` no nível do
  crate.

Documentar no CONTRIBUTING.md.

### P3 (baixa prioridade — qualidade de vida)

#### P3.1 — `prelude`
```rust
// lib.rs
pub mod prelude {
    pub use crate::pipeline::state::{FileImage, Blur, Export, DiskCache, DisplayCache};
    pub use crate::pipeline::sgraph::builder::PathBuilder;
    pub use crate::pipeline::sgraph::compile::ExecutionMode;
    pub use crate::pipeline::state::ExportFormat;
}
```

Caller faz `use pixors_engine::prelude::*;` e tem 80% do necessário.

#### P3.2 — Separar `Stage` em `SourceStage` / `OperationStage` / `SinkStage`
Hoje `Stage` tem 3 métodos opcionais; só um é válido por variante.
Type-safer:
```rust
pub trait SourceStage: Stage { fn source_runner(&self) -> Box<dyn SourceRunner>; }
pub trait OperationStage: Stage { fn op_runner(&self) -> Box<dyn OperationRunner>; }
pub trait SinkStage: Stage { fn sink_runner(&self) -> Box<dyn SinkRunner>; }
```

Mas `enum_dispatch` não ajuda com hierarquia de traits. Custo de
implementação alto. **Trade-off**: deixar como está, aceitar tentativa-
e-erro do Executor. Adicionar pelo menos:
```rust
pub fn role(&self) -> StageRole { Source | Operation | Sink }
```
no trait `Stage` pra Executor decidir uma vez sem `Result`-juggling.

#### P3.3 — Sub-grupos em `color/` e `pixel/`
Antes de o módulo dobrar de tamanho:
```
color/
├── space/         # primaries, chromaticity, cie
├── transfer/      # transfer functions
└── pipeline/      # conversion, matrix, sample, detect
```
Não urgente.

#### P3.4 — `gpu/kernels/` perto das stages?
Alternativa: mover `gpu/kernels/blur.rs` (WGSL + pipeline cache) pra
`pipeline/exec/blur_kernel/gpu_kernel.rs`. Pró: tudo de blur GPU num
lugar. Contra: kernels reusáveis viram acoplados a um stage.

**Recomendação**: deixar em `gpu/kernels/` enquanto compartilharem
lógica (pipeline caching via `OnceLock`, etc.). Mover só se ficar 1:1
com um stage específico.

---

## 4. Estrutura Proposta (após P1+P2)

```
src/
├── lib.rs                     # re-exports + pub mod prelude
├── error.rs
├── utils.rs                   # tracing + DebugStopwatch
├── color/
├── pixel/
├── container/
├── gpu/
│   ├── mod.rs
│   ├── context.rs             # GpuContext + try_init + adapter cascade
│   ├── buffer.rs              # GpuBuffer + Buffer enum  ← migrado de storage/
│   ├── kernels/
│   │   └── blur.rs            # WGSL + pipeline cache
│   └── tests.rs
└── pipeline/
    ├── mod.rs
    ├── state/                 # StateNode universe
    │   ├── mod.rs             # ← define StateNode enum + StateNodeTrait + ExpandCtx + ExpansionOption + ExportFormat
    │   ├── blur.rs
    │   ├── disk_cache.rs
    │   ├── display_cache.rs
    │   ├── export.rs
    │   └── file_image.rs
    ├── state_graph/           # ← antigo sgraph/
    │   ├── mod.rs
    │   ├── graph.rs
    │   ├── builder.rs
    │   ├── compile.rs
    │   ├── actions.rs
    │   ├── history.rs
    │   ├── cache.rs
    │   ├── ports.rs
    │   └── tests.rs           # integração compile/history/ports
    ├── exec/                  # ExecStage universe
    │   ├── mod.rs             # ← define ExecStage enum + Stage trait + Device + StageRole
    │   ├── blur_kernel/
    │   │   ├── mod.rs
    │   │   ├── cpu.rs
    │   │   └── gpu.rs
    │   ├── color_convert.rs
    │   ├── upload.rs
    │   ├── download.rs
    │   ├── neighborhood_agg.rs    # ← renomeado de to_neighborhood.rs
    │   ├── scanline_accumulator.rs # ← renomeado de to_tile.rs
    │   ├── tile_to_scanline.rs    # ← renomeado de to_scanline.rs
    │   ├── file_decoder.rs
    │   ├── png_encoder.rs
    │   ├── cache_reader.rs
    │   ├── cache_writer.rs
    │   └── display_sink.rs
    └── exec_graph/            # ← antigo egraph/
        ├── mod.rs
        ├── graph.rs
        ├── executor.rs
        ├── emitter.rs
        ├── item.rs
        └── runner.rs          # SourceRunner + OperationRunner + SinkRunner traits
```

**Removidos**: `storage/`, `approx.rs`.
**Renomeados**: `to_neighborhood.rs` → `neighborhood_agg.rs`, etc.
(nomes verb_de_ação atrapalham busca; `to_tile` esconde que é um
acumulador).

---

## 5. Decisões e Trade-offs

### Decisão A — Quem é dono do trait?
Trait fica **com as variantes**, não com o consumidor. Princípio:
*producers own contracts*. Quem implementa um trait normalmente também
o exporta. `pipeline/state/mod.rs` é o lugar natural pra
`StateNodeTrait`. Sgraph importa, não o contrário.

### Decisão B — `sgraph`/`egraph` ou `state_graph`/`exec_graph`?
Recomendado **renomear**. Custos: rename refactor (IDE faz), churn de
diff. Benefício: leitura inicial sem decodificação.

Alternativa aceitável: manter abreviação se for jargão estabelecido na
equipe. Documentar no `pipeline/mod.rs`:
```rust
//! - `state_graph` (sgraph): user-facing DAG of operations.
//! - `exec_graph` (egraph): low-level compiled DAG of executable stages.
```

### Decisão C — `Buffer` em `gpu/` ou `storage/`?
A `Buffer::Gpu` variante força dependência. Duas saídas honestas:
1. Aceitar dependência: `Buffer` mora em `gpu/buffer.rs`. `storage/`
   morre.
2. Esconder GPU atrás de feature flag.

A 1 é mais simples. A 2 abre porta pra build mínimo (CLI no-GPU). Opção
1 hoje, opção 2 quando demanda aparecer.

### Decisão D — Splittar arquivos CPU/GPU já?
Só vale a pena quando o arquivo passa de ~300 linhas OU quando CPU e
GPU divergem em deps. `blur_kernel.rs` (448 linhas) já passa do
threshold — splittar agora. Outros stages (`color_convert`, `upload`)
são pequenos — manter um arquivo cada.

### Decisão E — Tests inline vs `tests/` integration crate
Inline `#[cfg(test)] mod tests` é convenção Rust idiomática pra
unidades. `tests/` é pra integração ponta-a-ponta cruzando módulos.
Recomendação: **ambos**, regra clara:
- Cada `state/blur.rs` tem `#[cfg(test)] mod tests` testando Blur.
- `pipeline/state_graph/tests.rs` testa compile + history + roundtrips.
- `tests/integration.rs` (futuro) pra runs PathBuilder→Export reais.

### Decisão F — `prelude` agressivo ou minimal?
Minimal. Só o que 80% dos usuários precisam:
- `PathBuilder`
- variantes de `StateNode` (FileImage, Blur, …)
- `ExecutionMode`
- `ExportFormat`

Não exportar runners, ExecStage, Executor — uso interno.

### Decisão G — `enum_dispatch` resolve sempre?
Resolve enquanto traits forem flat (sem hierarquia, sem generics). Pra
type-safe `SourceStage`/`OperationStage`/`SinkStage` precisa abandonar
`enum_dispatch` ou usar 3 enums separados (UploadStage, OpStage,
SinkStage). Custo alto. **Aceitar**: trait `Stage` flat com `Result`-
juggling até virar problema de perf (não vai, é uma vez no Executor).

---

## 6. Plano de Migração Sugerido

Fazer em PRs pequenos, na ordem:

1. **PR-1** (P1.1): mover `StateNodeTrait` para `state/mod.rs`. Mover
   `Stage` trait + `ExecStage` enum para `exec/mod.rs`. `sgraph/node.rs`
   e `egraph/stage.rs` ficam vazios (deletar). Sem mudança de
   comportamento, só re-localização.

2. **PR-2** (P1.3): deletar `approx.rs` + import morto em `lib.rs`.

3. **PR-3** (P1.2): rename `sgraph` → `state_graph`, `egraph` →
   `exec_graph`. Rename file-level com `git mv`. Atualiza imports.

4. **PR-4** (P2.1): migrar `Buffer` pra `gpu/buffer.rs`. Deletar
   `storage/`. Atualizar callers.

5. **PR-5** (P2.2): splittar `exec/blur_kernel.rs` em
   `exec/blur_kernel/{mod,cpu,gpu}.rs`.

6. **PR-6** (P2.3): substituir `use state::*` por imports nomeados.

7. **PR-7** (P3.1): adicionar `prelude` em `lib.rs`.

8. **PR-8** (P3.4): rename `to_tile` → `scanline_accumulator`,
   `to_neighborhood` → `neighborhood_agg`, `to_scanline` →
   `tile_to_scanline`.

PRs 5/6/7/8 são opcionais e podem ficar pra outro sprint.

---

## 7. Resumo Executivo

**Veredito**: a nova estrutura é **um upgrade real** sobre a anterior. A
introdução de `enum_dispatch` + um arquivo por variante é a mudança mais
valiosa — elimina os matches gigantes que existiam em `compile.rs` e
`stage.rs`.

**Maior fraqueza**: dependência conceitual invertida entre `state/` e
`sgraph/` (e simétrico em `exec/` ↔ `egraph/`). Trait deveria viver com
o produtor. P1.1 endereça.

**Ganho rápido sem risco**: deletar `approx.rs` (P1.3), splittar
`blur_kernel.rs` (P2.2), criar `prelude` (P3.1).

**Ganho médio com refactor**: mover `Buffer` pra `gpu/` (P2.1), rename
`sgraph`/`egraph` (P1.2).

Implementar P1.1 → P1.3 cobre o que tem real impacto em manutenção. O
resto é polimento.
