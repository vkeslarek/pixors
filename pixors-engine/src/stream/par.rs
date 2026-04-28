use crate::stream::{Frame, Pipe};
use std::sync::mpsc;

/// Parallel pipe — dispatches tiles to N worker threads for concurrent processing.
///
/// # Flow
/// ```text
/// rx → [Dispatcher] → [Worker 0] →
///            │         [Worker 1] → tx → out
///            │            ⋮
///            └── non-tile frames direct to tx
/// ```
///
/// # Design
/// - Dispatcher (1 thread): round-robin to workers for `FrameKind::Tile`,
///   sends non-tile frames directly to output (preserving order).
/// - Workers (N threads): each has its own `sync_channel`, processes one tile
///   at a time, sends result to shared output channel.
/// - Zero Mutex. Zero lock. Only channels.
/// - Workers default to `2 × available_parallelism` (I/O-bound mix).
pub struct ParPipe<F> {
    processor: F,
    num_workers: usize,
}

impl<F> ParPipe<F>
where
    F: Fn(Frame) -> Frame + Send + 'static + Clone,
{
    pub fn new(processor: F, num_workers: usize) -> Self {
        Self { processor, num_workers: num_workers.max(1) }
    }

    pub fn with_default_workers(processor: F) -> Self {
        let n = std::thread::available_parallelism()
            .map(|p| p.get() * 2)
            .unwrap_or(4);
        Self::new(processor, n)
    }
}

impl<F> Pipe for ParPipe<F>
where
    F: Fn(Frame) -> Frame + Send + 'static + Clone,
{
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame> {
        let (tx, out) = mpsc::sync_channel(64);
        let (worker_txs, worker_rxs): (Vec<_>, Vec<_>) =
            (0..self.num_workers).map(|_| mpsc::sync_channel::<Frame>(64)).unzip();

        // Spawn workers
        let processor = self.processor;
        for worker_rx in worker_rxs {
            let tx = tx.clone();
            let p = processor.clone();
            std::thread::spawn(move || {
                while let Ok(mut frame) = worker_rx.recv() {
                    frame = p(frame);
                    let is_terminal = frame.is_terminal();
                    if tx.send(frame).is_err() { break; }
                    if is_terminal { break; }
                }
            });
        }

        // Spawn dispatcher
        std::thread::spawn(move || {
            let mut next = 0usize;
            while let Ok(frame) = rx.recv() {
                if frame.is_tile() {
                    let idx = next % self.num_workers;
                    next = next.wrapping_add(1);
                    if worker_txs[idx % self.num_workers].send(frame).is_err() {
                        break; // worker died
                    }
                } else {
                    let is_terminal = frame.is_terminal();
                    if tx.send(frame).is_err() { break; }
                    if is_terminal { break; }
                }
            }
        });

        out
    }
}
