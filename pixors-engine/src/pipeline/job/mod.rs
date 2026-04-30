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
    pub fn then<O: Operation<In = Item>>(mut self, mut op: O) -> JobBuilder<O::Out>
    where Item: Clone
    {
        let (tx, next_rx) = mpsc::sync_channel(64);
        let cancel_inner = self.cancel.clone();
        let prog = Arc::clone(&self.progress);

        self.handles.push(std::thread::spawn(move || {
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
    use crate::image::{Tile, TileCoord};
    use crate::pipeline::operation::Operation;
    use crate::pipeline::sink::Sink;
    use crate::pipeline::source::Source;
    use crate::pixel::{AlphaPolicy, Rgba};
    use half::f16;
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
            item: Arc<u32>,
            emit: &mut Emitter<u32>,
        ) -> Result<(), crate::error::Error> {
            emit.emit(*item * 2);
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
            item: Arc<u32>,
            emit: &mut Emitter<String>,
        ) -> Result<(), crate::error::Error> {
            emit.emit(format!("{}", *item));
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
            item: Arc<Vec<u8>>,
            emit: &mut Emitter<Vec<u8>>,
        ) -> Result<(), crate::error::Error> {
            let mut v = (*item).clone();
            v.reverse();
            emit.emit(v);
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

    // ── Tile-pipeline helpers ────────────────────────────────────────────

    struct TestTileSource {
        tiles: Vec<Tile<Rgba<f16>>>,
    }

    impl Source for TestTileSource {
        type Item = Tile<Rgba<f16>>;

        fn run(self, emit: &mut Emitter<Tile<Rgba<f16>>>, cancel: Arc<AtomicBool>) {
            for tile in self.tiles {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                emit.emit(tile);
            }
        }

        fn total(&self) -> Option<u32> {
            Some(self.tiles.len() as u32)
        }
    }

    struct TileCollectSink {
        collected: Arc<Mutex<Vec<Tile<Rgba<f16>>>>>,
    }

    impl Sink for TileCollectSink {
        type Item = Tile<Rgba<f16>>;

        fn consume(&self, item: Tile<Rgba<f16>>) -> Result<(), crate::error::Error> {
            self.collected.lock().unwrap().push(item);
            Ok(())
        }
    }

    struct U8CollectSink {
        collected: Arc<Mutex<Vec<u8>>>,
    }

    impl Sink for U8CollectSink {
        type Item = Tile<Rgba<f16>>;

        fn consume(&self, item: Tile<Rgba<f16>>) -> Result<(), crate::error::Error> {
            let raw = bytemuck::cast_slice::<Rgba<f16>, u8>(&item.data).to_vec();
            self.collected.lock().unwrap().extend(raw);
            Ok(())
        }

        fn finish(&self) {
            // signal done
        }
    }

    fn make_test_tile(r: f32, g: f32, b: f32, a: f32, w: u32, h: u32) -> Tile<Rgba<f16>> {
        let coord = TileCoord::from_xywh(0, 0, 0, w, h);
        let pixels = vec![Rgba {
            r: f16::from_f32(r),
            g: f16::from_f32(g),
            b: f16::from_f32(b),
            a: f16::from_f32(a),
        }; (w * h) as usize];
        Tile::new(coord, pixels)
    }

    fn make_test_tiles(
        count: u32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Vec<Tile<Rgba<f16>>> {
        (0..count)
            .map(|i| make_test_tile(r * (i as f32 + 1.0) / count as f32, g, b, a, 4, 4))
            .collect()
    }

    // ── Split tests ──────────────────────────────────────────────────────

    #[test]
    fn split_tile_branches_receive_same_data() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));

        let mut branches = Job::from_source(TestTileSource {
            tiles: make_test_tiles(5, 1.0, 0.5, 0.2, 1.0),
        })
        .split(2);

        let br1 = branches.remove(0);
        let br2 = branches.remove(0);

        let job_a = br1.sink(TileCollectSink {
            collected: Arc::clone(&a),
        });
        let job_b = br2.sink(TileCollectSink {
            collected: Arc::clone(&b),
        });

        job_a.join();
        job_b.join();

        let a_data = a.lock().unwrap();
        let b_data = b.lock().unwrap();
        assert_eq!(a_data.len(), 5);
        assert_eq!(b_data.len(), 5);

        for i in 0..5 {
            let pa = &a_data[i].data[0];
            let pb = &b_data[i].data[0];
            assert_eq!(pa.r.to_bits(), pb.r.to_bits(), "tile {i} r mismatch");
            assert_eq!(pa.g.to_bits(), pb.g.to_bits(), "tile {i} g mismatch");
            assert_eq!(pa.b.to_bits(), pb.b.to_bits(), "tile {i} b mismatch");
            assert_eq!(pa.a.to_bits(), pb.a.to_bits(), "tile {i} a mismatch");
        }
    }

    #[test]
    fn split_three_branches_each_gets_all_tiles() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));
        let c = Arc::new(Mutex::new(Vec::new()));

        let mut branches = Job::from_source(TestTileSource {
            tiles: make_test_tiles(3, 0.8, 0.2, 0.5, 1.0),
        })
        .split(3);

        let j_a = branches.remove(0).sink(TileCollectSink { collected: Arc::clone(&a) });
        let j_b = branches.remove(0).sink(TileCollectSink { collected: Arc::clone(&b) });
        let j_c = branches.remove(0).sink(TileCollectSink { collected: Arc::clone(&c) });

        j_a.join();
        j_b.join();
        j_c.join();

        assert_eq!(a.lock().unwrap().len(), 3);
        assert_eq!(b.lock().unwrap().len(), 3);
        assert_eq!(c.lock().unwrap().len(), 3);
    }

    // ── Full pipeline: ColorConvertOp + ViewportSink ──────────────────────

    #[test]
    fn full_pipeline_viewport_path() {
        use crate::pipeline::operation::color::ColorConvertOperation;
        use crate::pipeline::sink::viewport::{Viewport, ViewportSink};

        let vp = Arc::new(Viewport::new());
        let conv = crate::color::ColorSpace::SRGB
            .converter_to(crate::color::ColorSpace::ACES_CG)
            .unwrap();
        let srgb_conv = crate::color::ColorSpace::ACES_CG
            .converter_to(crate::color::ColorSpace::SRGB)
            .unwrap();

        let mut branches = Job::from_source(TestTileSource {
            tiles: make_test_tiles(2, 0.5, 0.3, 0.7, 1.0),
        })
        .then(ColorConvertOperation::with_conv(conv, AlphaPolicy::PremultiplyOnPack))
        .split(2);

        let vp_sink = ViewportSink::new(Arc::clone(&vp), srgb_conv);
        let vp_job = branches.remove(0).sink(vp_sink);

        let wk_data = Arc::new(Mutex::new(Vec::new()));
        let wk_job = branches.remove(0).sink(U8CollectSink {
            collected: Arc::clone(&wk_data),
        });

        vp_job.join();
        wk_job.join();

        assert!(vp.is_ready(), "viewport should be marked ready by finish()");

        let coord = TileCoord::from_xywh(0, 0, 0, 4, 4);
        let stored = vp.get(0, coord).expect("tile should be in viewport");
        assert_eq!(stored.len(), 64, "4x4 tile = 64 bytes RGBA8");
    }

    // ── Full pipeline: ColorConvertOp + WorkingSink ───────────────────────

    #[test]
    fn full_pipeline_working_path() {
        use crate::pipeline::operation::color::ColorConvertOperation;
        use crate::pipeline::sink::working::WorkingSink;
        use crate::storage::WorkingWriter;

        let dir = std::env::temp_dir().join("pixors_test_full_wk");
        std::fs::create_dir_all(&dir).unwrap();

        let writer = Arc::new(WorkingWriter::new(dir.clone(), 4, 8, 8).unwrap());
        let wk_conv = crate::color::ColorSpace::ACES_CG
            .converter_to(crate::color::ColorSpace::ACES_CG)
            .unwrap();

        let conv = crate::color::ColorSpace::SRGB
            .converter_to(crate::color::ColorSpace::ACES_CG)
            .unwrap();

        Job::from_source(TestTileSource {
            tiles: make_test_tiles(2, 0.5, 0.5, 0.5, 1.0),
        })
        .then(ColorConvertOperation::with_conv(conv, AlphaPolicy::PremultiplyOnPack))
        .sink(WorkingSink::new(Arc::clone(&writer), wk_conv))
        .join();

        let read = writer.read_tile(TileCoord::from_xywh(0, 0, 0, 4, 4)).unwrap().unwrap();
        assert_eq!(read.data.len(), 16, "4x4 tile should have 16 pixels");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Cancel tests ─────────────────────────────────────────────────────

    #[test]
    fn job_cancel_stops_pipeline() {
        let collected = Arc::new(Mutex::new(Vec::new()));

        let job = Job::from_source(CounterSource { count: 1000 })
            .then(DoubleOp)
            .sink(CollectSink {
                data: Arc::clone(&collected),
            });

        job.cancel();
        job.join();

        let results = collected.lock().unwrap();
        assert!(results.len() < 1000, "cancel should stop the pipeline early: got {}", results.len());
    }

    #[test]
    fn emitter_does_not_panic_on_closed_rx() {
        let (tx, rx) = mpsc::sync_channel::<u32>(1);
        let mut emit = Emitter::new(tx);
        drop(rx);
        for i in 0..50 {
            emit.emit(i);
        }
    }
}
