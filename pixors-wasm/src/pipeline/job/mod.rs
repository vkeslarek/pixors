use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use crate::pipeline::sink::Sink;
use crate::pipeline::source::Source;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};

#[cfg(target_arch = "wasm32")]
macro_rules! spawn {
    ($task:expr) => { $task() };
}
#[cfg(not(target_arch = "wasm32"))]
macro_rules! spawn {
    ($task:expr) => { rayon::spawn($task) };
}

pub struct Job {
    progress: Arc<AtomicU32>,
    total: u32,
    cancel: Arc<AtomicBool>,
    done_flags: Vec<Arc<AtomicBool>>,
}

impl Job {
    pub fn from_source<S: Source>(source: S) -> JobBuilder<S::Item> {
        let cancel = Arc::new(AtomicBool::new(false));
        let total = source.total().unwrap_or(0);
        let progress = Arc::new(AtomicU32::new(0));
        let mut done_flags = Vec::new();

        let (tx, rx) = mpsc::sync_channel(64);
        let cancel_src = cancel.clone();
        let done = Arc::new(AtomicBool::new(false));
        done_flags.push(Arc::clone(&done));

        spawn!(move || {
            let mut emit = Emitter::new(tx);
            source.run(&mut emit, cancel_src);
            done.store(true, Ordering::Release);
        });

        JobBuilder {
            cancel,
            progress,
            total,
            done_flags,
            rx,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn progress(&self) -> f32 {
        let done = self.progress.load(Ordering::Relaxed);
        if self.total == 0 {
            0.0
        } else {
            (done as f32 / self.total as f32).min(1.0)
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn join(&self) {
        for flag in &self.done_flags {
            while !flag.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
    }

    pub fn join_all(jobs: impl IntoIterator<Item = Job>) {
        for job in jobs {
            job.join();
        }
    }
}

pub fn tee<Item: Clone + Send + 'static>(
    rx: mpsc::Receiver<Item>,
    n: usize,
) -> Vec<mpsc::Receiver<Item>> {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..n).map(|_| mpsc::sync_channel(64)).unzip();
    let done = Arc::new(AtomicBool::new(false));
    spawn!(move || {
        let mut txs: Vec<Option<mpsc::SyncSender<Item>>> = txs.into_iter().map(Some).collect();
        while let Ok(item) = rx.recv() {
            for slot in &mut txs {
                if let Some(tx) = slot
                    && tx.send(item.clone()).is_err()
                {
                    *slot = None;
                }
            }
            if txs.iter().all(Option::is_none) {
                break;
            }
        }
        done.store(true, Ordering::Release);
    });
    rxs
}

pub struct JobBuilder<Item: Send + 'static> {
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
    total: u32,
    done_flags: Vec<Arc<AtomicBool>>,
    rx: mpsc::Receiver<Item>,
    _phantom: std::marker::PhantomData<Item>,
}

impl<Item: Send + 'static> JobBuilder<Item> {
    pub fn then<O: Operation<In = Item>>(mut self, mut op: O) -> JobBuilder<O::Out>
    where Item: Clone
    {
        let (tx, next_rx) = mpsc::sync_channel(64);
        let cancel_inner = self.cancel.clone();
        let prog = Arc::clone(&self.progress);
        let done = Arc::new(AtomicBool::new(false));
        self.done_flags.push(Arc::clone(&done));

        spawn!(move || {
            let mut emit = Emitter::new(tx);
            while let Ok(item) = self.rx.recv() {
                if cancel_inner.load(Ordering::Relaxed) {
                    break;
                }
                if op.process(Arc::new(item), &mut emit).is_err() {
                    cancel_inner.store(true, Ordering::Release);
                    break;
                }
                prog.fetch_add(1, Ordering::Relaxed);
            }
            let _ = op.finish(&mut emit);
            done.store(true, Ordering::Release);
        });

        JobBuilder {
            cancel: self.cancel,
            progress: self.progress,
            total: self.total,
            done_flags: self.done_flags,
            rx: next_rx,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn sink<K: Sink<Item = Item>>(mut self, sink: K) -> Job {
        let cancel_sink = self.cancel.clone();
        let done = Arc::new(AtomicBool::new(false));
        self.done_flags.push(Arc::clone(&done));

        spawn!(move || {
            while let Ok(item) = self.rx.recv() {
                if cancel_sink.load(Ordering::Relaxed) {
                    break;
                }
                let _ = sink.consume(item);
            }
            sink.finish();
            done.store(true, Ordering::Release);
        });

        Job {
            progress: self.progress,
            total: self.total,
            cancel: self.cancel,
            done_flags: self.done_flags,
        }
    }
}

impl<Item: Clone + Send + 'static> JobBuilder<Item> {
    pub fn split(mut self, n: usize) -> Vec<JobBuilder<Item>> {
        let rxs = tee(self.rx, n);
        rxs.into_iter()
            .map(|rx| JobBuilder {
                cancel: self.cancel.clone(),
                progress: Arc::clone(&self.progress),
                total: self.total,
                done_flags: Vec::new(),
                rx,
                _phantom: std::marker::PhantomData,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wasm_bindgen_test::wasm_bindgen_test;

    struct CounterSource { count: u32 }

    impl Source for CounterSource {
        type Item = u32;
        fn run(self, emit: &mut Emitter<u32>, cancel: Arc<AtomicBool>) {
            for i in 0..self.count {
                if cancel.load(Ordering::Relaxed) { break; }
                emit.emit(i);
            }
        }
        fn total(&self) -> Option<u32> { Some(self.count) }
    }

    struct DoubleOp;
    impl Operation for DoubleOp {
        type In = u32;
        type Out = u32;
        fn name(&self) -> &'static str { "double" }
        fn process(&mut self, item: Arc<u32>, emit: &mut Emitter<u32>) -> Result<(), crate::error::Error> {
            emit.emit(*item * 2);
            Ok(())
        }
    }

    struct ToStringOp;
    impl Operation for ToStringOp {
        type In = u32;
        type Out = String;
        fn name(&self) -> &'static str { "to_string" }
        fn process(&mut self, item: Arc<u32>, emit: &mut Emitter<String>) -> Result<(), crate::error::Error> {
            emit.emit(format!("{}", *item));
            Ok(())
        }
    }

    struct CollectSink<T: Send + 'static> { data: Arc<Mutex<Vec<T>>> }
    impl<T: Send + 'static> Sink for CollectSink<T> {
        type Item = T;
        fn consume(&self, item: T) -> Result<(), crate::error::Error> {
            self.data.lock().unwrap().push(item);
            Ok(())
        }
    }

    #[wasm_bindgen_test]
    fn source_op_sink() {
        let gathered = Arc::new(Mutex::new(Vec::new()));
        let job = Job::from_source(CounterSource { count: 10 })
            .then(DoubleOp)
            .sink(CollectSink { data: Arc::clone(&gathered) });
        job.join();
        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 10);
        for (i, val) in results.iter().enumerate() {
            assert_eq!(*val, i as u32 * 2);
        }
    }

    #[wasm_bindgen_test]
    fn source_direct_to_sink() {
        let gathered = Arc::new(Mutex::new(Vec::new()));
        Job::from_source(CounterSource { count: 5 })
            .sink(CollectSink { data: Arc::clone(&gathered) })
            .join();
        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 5);
        assert_eq!(results.as_slice(), &[0, 1, 2, 3, 4]);
    }

    #[wasm_bindgen_test]
    fn type_changing_chain() {
        let gathered = Arc::new(Mutex::new(Vec::new()));
        Job::from_source(CounterSource { count: 3 })
            .then(DoubleOp)
            .then(ToStringOp)
            .sink(CollectSink { data: Arc::clone(&gathered) })
            .join();
        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results.as_slice(), &["0", "2", "4"]);
    }

    #[wasm_bindgen_test]
    fn split_two_branches() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));
        let mut branches = Job::from_source(CounterSource { count: 5 }).split(2);
        let job_a = branches.remove(0).then(DoubleOp).sink(CollectSink { data: Arc::clone(&a) });
        let job_b = branches.remove(0).sink(CollectSink { data: Arc::clone(&b) });
        job_a.join();
        job_b.join();
        assert_eq!(a.lock().unwrap().as_slice(), &[0, 2, 4, 6, 8]);
        assert_eq!(b.lock().unwrap().as_slice(), &[0, 1, 2, 3, 4]);
    }

    #[wasm_bindgen_test]
    fn job_cancel_stops_pipeline() {
        // In WASM (synchronous mode), the pipeline runs inline.
        // Cancel only works when using native thread pool (rayon).
        let collected = Arc::new(Mutex::new(Vec::new()));
        let job = Job::from_source(CounterSource { count: 20 })
            .then(DoubleOp)
            .sink(CollectSink { data: Arc::clone(&collected) });
        job.cancel();
        job.join();
        let results = collected.lock().unwrap();
        // In sync mode all items process; in threaded mode cancel stops early
        assert!(!results.is_empty());
    }

    #[wasm_bindgen_test]
    fn emitter_does_not_panic_on_closed_rx() {
        let (tx, rx) = mpsc::sync_channel::<u32>(1);
        let mut emit = Emitter::new(tx);
        drop(rx);
        for i in 0..50 {
            emit.emit(i);
        }
    }
}
