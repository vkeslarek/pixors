//! Debugging utilities.

/// Guard that logs elapsed time on drop.
pub struct DebugStopwatch {
    name: String,
    start: std::time::Instant,
}

impl DebugStopwatch {
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
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
/// The name can be any expression implementing `Into<String>`, including format strings.
#[macro_export]
macro_rules! debug_stopwatch {
    ($name:expr) => {
        $crate::utils::DebugStopwatch::start($name)
    };
}
