use crate::stream::{Frame, FrameKind};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

/// Drains Progress frames and calls callback (typically to emit event).
pub struct ProgressSink {
    callback: Arc<dyn Fn(u8) + Send + Sync>,
}

impl ProgressSink {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(u8) + Send + Sync + 'static,
    {
        Self { callback: Arc::new(callback) }
    }

    pub fn run(&self, rx: mpsc::Receiver<Frame>) -> JoinHandle<()> {
        let cb = Arc::clone(&self.callback);
        std::thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                if let FrameKind::Progress { done, total } = frame.kind {
                    let percent = if total > 0 {
                        ((done as f32 / total as f32) * 100.0) as u8
                    } else {
                        0
                    };
                    cb(percent);
                }
                if frame.is_terminal() {
                    break;
                }
            }
        })
    }
}
