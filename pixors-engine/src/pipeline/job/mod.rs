use crate::pipeline::emitter::Emitter;
use crate::pipeline::operation::Operation;
use crate::pipeline::sink::Sink;
use crate::pipeline::source::Source;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

/// Non-blocking orchestrator. Call [`join`](Job::join) to wait for completion,
/// or drop to cancel and join.
pub struct Job {
    progress: Arc<AtomicU32>,
    total: u32,
    cancel: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
}

impl Job {
    pub fn from_source<S: Source>(source: S) -> JobBuilder<S::Item> {
        let cancel = Arc::new(AtomicBool::new(false));
        let total = source.total().unwrap_or(0);
        let progress = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();

        let (tx, rx) = mpsc::sync_channel(64);
        let cancel_src = cancel.clone();
        handles.push(std::thread::spawn(move || {
            let mut emit = Emitter::new(tx);
            source.run(&mut emit, cancel_src);
        }));

        JobBuilder {
            cancel,
            progress,
            total,
            handles,
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

    /// Wait for all threads to finish. Consumes the Job.
    pub fn join(mut self) {
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
        std::mem::forget(self);
    }

    /// Wait for all given jobs to finish.
    pub fn join_all(jobs: impl IntoIterator<Item = Job>) {
        for job in jobs {
            job.join();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

// ── Tee — fan-out ──────────────────────────────────────────────────────────

pub fn tee<Item: Clone + Send + 'static>(
    rx: mpsc::Receiver<Item>,
    n: usize,
) -> Vec<mpsc::Receiver<Item>> {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..n).map(|_| mpsc::sync_channel(64)).unzip();
    std::thread::spawn(move || {
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
    });
    rxs
}

// ── JobBuilder — type-aware chaining ────────────────────────────────────────

pub struct JobBuilder<Item: Send + 'static> {
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
    total: u32,
    handles: Vec<JoinHandle<()>>,
    rx: mpsc::Receiver<Item>,
    _phantom: std::marker::PhantomData<Item>,
}

impl<Item: Send + 'static> JobBuilder<Item> {
    pub fn then<O: Operation<In = Item>>(mut self, mut op: O) -> JobBuilder<O::Out> {
        let (tx, next_rx) = mpsc::sync_channel(64);
        let cancel_inner = self.cancel.clone();
        let prog = Arc::clone(&self.progress);

        self.handles.push(std::thread::spawn(move || {
            let mut emit = Emitter::new(tx);
            while let Ok(item) = self.rx.recv() {
                if cancel_inner.load(Ordering::Relaxed) {
                    break;
                }
                if op.process(item, &mut emit).is_err() {
                    cancel_inner.store(true, Ordering::Release);
                    break;
                }
                prog.fetch_add(1, Ordering::Relaxed);
            }
            let _ = op.finish(&mut emit);
        }));

        JobBuilder {
            cancel: self.cancel,
            progress: self.progress,
            total: self.total,
            handles: self.handles,
            rx: next_rx,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn sink<K: Sink<Item = Item>>(mut self, sink: K) -> Job {
        let cancel_sink = self.cancel.clone();
        self.handles.push(std::thread::spawn(move || {
            while let Ok(item) = self.rx.recv() {
                if cancel_sink.load(Ordering::Relaxed) {
                    break;
                }
                let _ = sink.consume(item);
            }
            sink.finish();
        }));

        Job {
            progress: self.progress,
            total: self.total,
            cancel: self.cancel,
            handles: self.handles,
        }
    }
}

impl<Item: Clone + Send + 'static> JobBuilder<Item> {
    /// Split into N branches. Source handles move to a daemon thread.
    /// Each branch is independent — call `.join()` on all resulting Jobs.
    pub fn split(mut self, n: usize) -> Vec<JobBuilder<Item>> {
        // Daemon thread joins source handles
        let source_handles = std::mem::take(&mut self.handles);
        std::thread::spawn(move || {
            for h in source_handles {
                let _ = h.join();
            }
        });

        let rxs = tee(self.rx, n);
        rxs.into_iter()
            .map(|rx| JobBuilder {
                cancel: self.cancel.clone(),
                progress: Arc::clone(&self.progress),
                total: self.total,
                handles: Vec::new(),
                rx,
                _phantom: std::marker::PhantomData,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::operation::Operation;
    use crate::pipeline::sink::Sink;
    use crate::pipeline::source::Source;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    struct CounterSource {
        count: u32,
    }

    impl Source for CounterSource {
        type Item = u32;

        fn run(self, emit: &mut Emitter<u32>, cancel: Arc<AtomicBool>) {
            for i in 0..self.count {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                emit.emit(i);
            }
        }

        fn total(&self) -> Option<u32> {
            Some(self.count)
        }
    }

    struct DoubleOp;

    impl Operation for DoubleOp {
        type In = u32;
        type Out = u32;

        fn name(&self) -> &'static str {
            "double"
        }

        fn process(
            &mut self,
            item: u32,
            emit: &mut Emitter<u32>,
        ) -> Result<(), crate::error::Error> {
            emit.emit(item * 2);
            Ok(())
        }
    }

    struct ToStringOp;

    impl Operation for ToStringOp {
        type In = u32;
        type Out = String;

        fn name(&self) -> &'static str {
            "to_string"
        }

        fn process(
            &mut self,
            item: u32,
            emit: &mut Emitter<String>,
        ) -> Result<(), crate::error::Error> {
            emit.emit(format!("{}", item));
            Ok(())
        }
    }

    struct CollectSink<T: Send + 'static> {
        data: Arc<Mutex<Vec<T>>>,
    }

    impl<T: Send + 'static> Sink for CollectSink<T> {
        type Item = T;

        fn consume(&self, item: T) -> Result<(), crate::error::Error> {
            self.data.lock().unwrap().push(item);
            Ok(())
        }
    }

    #[test]
    fn source_op_sink() {
        let gathered = Arc::new(Mutex::new(Vec::new()));

        let job = Job::from_source(CounterSource { count: 10 })
            .then(DoubleOp)
            .sink(CollectSink {
                data: Arc::clone(&gathered),
            });

        job.join();

        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 10);
        for (i, val) in results.iter().enumerate() {
            assert_eq!(*val, i as u32 * 2);
        }
    }

    #[test]
    fn source_direct_to_sink() {
        let gathered = Arc::new(Mutex::new(Vec::new()));

        Job::from_source(CounterSource { count: 5 })
            .sink(CollectSink {
                data: Arc::clone(&gathered),
            })
            .join();

        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 5);
        assert_eq!(results.as_slice(), &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn type_changing_chain() {
        let gathered = Arc::new(Mutex::new(Vec::new()));

        Job::from_source(CounterSource { count: 3 })
            .then(DoubleOp)
            .then(ToStringOp)
            .sink(CollectSink {
                data: Arc::clone(&gathered),
            })
            .join();

        let results = gathered.lock().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results.as_slice(), &["0", "2", "4"]);
    }

    struct FileSource {
        path: std::path::PathBuf,
    }

    impl Source for FileSource {
        type Item = Vec<u8>;

        fn run(self, emit: &mut Emitter<Vec<u8>>, _cancel: Arc<AtomicBool>) {
            if let Ok(data) = std::fs::read(&self.path) {
                emit.emit(data);
            }
        }

        fn total(&self) -> Option<u32> {
            Some(1)
        }
    }

    struct FileSink {
        path: std::path::PathBuf,
    }

    impl Sink for FileSink {
        type Item = Vec<u8>;

        fn consume(&self, item: Vec<u8>) -> Result<(), crate::error::Error> {
            std::fs::write(&self.path, &item).map_err(|e| crate::error::Error::Internal(e.to_string()))
        }
    }

    struct ReverseOp;

    impl Operation for ReverseOp {
        type In = Vec<u8>;
        type Out = Vec<u8>;

        fn name(&self) -> &'static str {
            "reverse"
        }

        fn process(
            &mut self,
            mut item: Vec<u8>,
            emit: &mut Emitter<Vec<u8>>,
        ) -> Result<(), crate::error::Error> {
            item.reverse();
            emit.emit(item);
            Ok(())
        }
    }

    #[test]
    fn file_to_file_pipeline() {
        let src = std::env::temp_dir().join("pixors_core_test_src.bin");
        let dst = std::env::temp_dir().join("pixors_core_test_dst.bin");
        let original: Vec<u8> = (0..255u8).collect();
        std::fs::write(&src, &original).unwrap();

        Job::from_source(FileSource { path: src.clone() })
            .then(ReverseOp)
            .sink(FileSink { path: dst.clone() })
            .join();

        let result = std::fs::read(&dst).unwrap();
        let mut expected = original.clone();
        expected.reverse();
        assert_eq!(result, expected);

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn split_two_branches() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));

        let mut branches = Job::from_source(CounterSource { count: 5 }).split(2);
        let br1 = branches.remove(0);
        let br2 = branches.remove(0);

        let job_a = br1.then(DoubleOp).sink(CollectSink {
            data: Arc::clone(&a),
        });
        let job_b = br2.sink(CollectSink {
            data: Arc::clone(&b),
        });

        job_a.join();
        job_b.join();

        assert_eq!(a.lock().unwrap().as_slice(), &[0, 2, 4, 6, 8]);
        assert_eq!(b.lock().unwrap().as_slice(), &[0, 1, 2, 3, 4]);
    }
}
