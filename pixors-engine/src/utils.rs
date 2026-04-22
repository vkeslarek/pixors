//! Debugging utilities.

/// Guard that logs elapsed time on drop.
pub struct DebugStopwatch {
    name: &'static str,
    start: std::time::Instant,
}

impl DebugStopwatch {
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            start: std::time::Instant::now(),
        }
    }
}

impl Drop for DebugStopwatch {
    fn drop(&mut self) {
        tracing::debug!("{} took {:?}", self.name, self.start.elapsed());
    }
}

/// Creates a stopwatch that logs when the scope ends.
/// Usage: `let _sw = debug_stopwatch!("my step");`
#[macro_export]
macro_rules! debug_stopwatch {
    ($name:expr) => {
        $crate::utils::DebugStopwatch::start($name)
    };
}
