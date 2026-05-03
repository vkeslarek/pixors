//! Pipeline — two-layer graph compiler and runtime.
//!
//! ## Architecture
//!
//! ```text
//! state_graph (sgraph) ──compiles-to──▶ exec_graph (egraph)
//!      │                                      │
//!   StateNode                           ExecNode
//!   (user ops: Blur, FileImage...)      (runnable stages: BlurKernel, Upload...)
//! ```
//!
//! - **state/** — user-facing operation types ([`StateNode`]) and the
//!   [`StateNodeTrait`]. Each file is one variant (e.g. `blur.rs`).
//! - **state_graph/** — graph infrastructure: [`StateGraph`], [`compile`],
//!   [`History`], [`PathBuilder`], ports.
//! - **exec/** — runtime stage types ([`ExecNode`]) and the [`Stage`] trait.
//!   Each file is one stage plus its runner. CPU and GPU variants live together
//!   (e.g. `blur_kernel.rs` contains `BlurKernel` + `BlurKernelGpu`).
//! - **exec_graph/** — execution graph infrastructure: [`ExecGraph`],
//!   [`Executor`], [`Emitter`], runner traits.
//!
//! ## Adding a new operation
//!
//! 1. Create `state/my_op.rs` — `impl StateNodeTrait` with `expand()`.
//! 2. Create `exec/my_op_stage.rs` — `impl Stage` + runner.
//! 3. Add the struct to the `StateNode` enum in `state/mod.rs`.
//! 4. Add the struct to the `ExecNode` enum in `exec/mod.rs`.
//!
//! [`StateNode`]: state::StateNode
//! [`StateNodeTrait`]: state::StateNodeTrait
//! [`StateGraph`]: state_graph::graph::StateGraph
//! [`compile`]: state_graph::compile::compile
//! [`History`]: state_graph::history::History
//! [`PathBuilder`]: state_graph::builder::PathBuilder
//! [`ExecNode`]: exec::ExecNode
//! [`Stage`]: exec::Stage
//! [`ExecGraph`]: exec_graph::graph::ExecGraph
//! [`Executor`]: exec_graph::executor::Executor
//! [`Emitter`]: exec_graph::emitter::Emitter

pub mod exec;
pub mod exec_graph;
pub mod state;
pub mod state_graph;
