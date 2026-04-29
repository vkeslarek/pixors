use crate::error::Error;
use crate::pipeline::emitter::Emitter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

pub trait Operation: Send + 'static {
    type In: Send + 'static;
    type Out: Send + 'static;

    fn name(&self) -> &'static str;
    fn cost(&self) -> f32 { 1.0 }

    fn process(&mut self, item: Self::In, emit: &mut Emitter<Self::Out>) -> Result<(), Error>;

    fn finish(&mut self, _emit: &mut Emitter<Self::Out>) -> Result<(), Error> { Ok(()) }

    fn run(self, cancel: Arc<AtomicBool>) -> (mpsc::SyncSender<Self::In>, OperationHandle<Self::Out>)
    where Self: Sized {
        let (tx, rx_in) = mpsc::sync_channel(64);
        let (tx_out, rx) = mpsc::sync_channel(64);
        let mut this = self;
        let cancel_inner = cancel.clone();
        let handle = std::thread::spawn(move || {
            let mut emit = Emitter::new(tx_out);
            while let Ok(item) = rx_in.recv() {
                if cancel_inner.load(Ordering::Relaxed) { break; }
                if this.process(item, &mut emit).is_err() {
                    cancel_inner.store(true, Ordering::Release);
                    break;
                }
            }
            let _ = this.finish(&mut emit);
        });
        (tx, OperationHandle { rx, handle: Some(handle), cancel })
    }
}

pub struct OperationHandle<Out: Send + 'static> {
    pub rx: mpsc::Receiver<Out>,
    handle: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}

impl<Out: Send + 'static> OperationHandle<Out> {
    pub fn cancel(&self) { self.cancel.store(true, Ordering::Release); }
}

impl<Out: Send + 'static> Drop for OperationHandle<Out> {
    fn drop(&mut self) {
        self.cancel();
        if let Some(h) = self.handle.take() { let _ = h.join(); }
    }
}
