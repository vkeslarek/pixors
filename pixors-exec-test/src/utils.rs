//! Debugging and assertion utilities.

pub fn init_tracing() {
    tracing_subscriber::fmt::init();
}

// ── Approximate equality ────────────────────────────────────────────────────

pub trait ApproximateEq<Rhs = Self> {
    fn approx_eq(&self, other: &Rhs, epsilon: f32) -> bool;
}

impl ApproximateEq for f32 {
    fn approx_eq(&self, other: &f32, epsilon: f32) -> bool {
        (self - other).abs() <= epsilon
    }
}

impl ApproximateEq for f64 {
    fn approx_eq(&self, other: &f64, epsilon: f32) -> bool {
        (self - other).abs() <= epsilon as f64
    }
}

impl<T: ApproximateEq, const N: usize> ApproximateEq for [T; N] {
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.iter()
            .zip(other.iter())
            .all(|(a, b)| a.approx_eq(b, epsilon))
    }
}

impl<T1: ApproximateEq, T2: ApproximateEq> ApproximateEq for (T1, T2) {
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.0.approx_eq(&other.0, epsilon) && self.1.approx_eq(&other.1, epsilon)
    }
}

impl<T1: ApproximateEq, T2: ApproximateEq, T3: ApproximateEq> ApproximateEq for (T1, T2, T3) {
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.0.approx_eq(&other.0, epsilon)
            && self.1.approx_eq(&other.1, epsilon)
            && self.2.approx_eq(&other.2, epsilon)
    }
}

impl<T1: ApproximateEq, T2: ApproximateEq, T3: ApproximateEq, T4: ApproximateEq> ApproximateEq
    for (T1, T2, T3, T4)
{
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.0.approx_eq(&other.0, epsilon)
            && self.1.approx_eq(&other.1, epsilon)
            && self.2.approx_eq(&other.2, epsilon)
            && self.3.approx_eq(&other.3, epsilon)
    }
}

#[macro_export]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr) => {
        $crate::assert_approx_eq!($left, $right, 1e-6);
    };
    ($left:expr, $right:expr, $epsilon:expr) => {{
        use $crate::utils::ApproximateEq;
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !left_val.approx_eq(right_val, $epsilon) {
                    panic!(
                        "assertion failed: `(left ≈ right)`\n  left: `{:?}`\n right: `{:?}`\n epsilon: `{}`",
                        left_val, right_val, $epsilon
                    );
                }
            }
        }
    }};
}

// ── Debug stopwatch ─────────────────────────────────────────────────────────
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
