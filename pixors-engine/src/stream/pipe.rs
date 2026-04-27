use crate::stream::Frame;
use std::sync::mpsc;

/// A transform on a stream of Frames. Runs in its own thread.
pub trait Pipe: Send + 'static {
    fn pipe(self, rx: mpsc::Receiver<Frame>) -> mpsc::Receiver<Frame>;
}

/// Tee: duplicate the stream to multiple consumers.
/// Returns one receiver per consumer. Spawns a fan-out thread.
/// When a consumer drops, only that slot is removed — others keep receiving.
pub fn tee(rx: mpsc::Receiver<Frame>, n: usize) -> Vec<mpsc::Receiver<Frame>> {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..n).map(|_| mpsc::sync_channel(64)).unzip();
    std::thread::spawn(move || {
        let mut txs: Vec<Option<mpsc::SyncSender<Frame>>> = txs.into_iter().map(Some).collect();
        while let Ok(frame) = rx.recv() {
            for slot in &mut txs {
                if let Some(tx) = slot
                    && tx.send(frame.clone()).is_err()
                {
                    *slot = None;
                }
            }
            if txs.iter().all(Option::is_none) { break; }
        }
    });
    rxs
}
