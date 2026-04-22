//! Approximate equality for floating-point types and structures.

/// Trait for approximate equality with a given tolerance.
pub trait ApproximateEq<Rhs = Self> {
    /// Returns `true` if `self` and `other` are approximately equal within `epsilon`.
    fn approx_eq(&self, other: &Rhs, epsilon: f32) -> bool;

    /// Returns `true` if `self` and `other` are approximately equal with a default epsilon.
    fn approx_eq_default(&self, other: &Rhs) -> bool {
        self.approx_eq(other, 1e-6)
    }
}

// Implement for f32
impl ApproximateEq for f32 {
    fn approx_eq(&self, other: &f32, epsilon: f32) -> bool {
        (self - other).abs() <= epsilon
    }
}

// Implement for f64
impl ApproximateEq for f64 {
    fn approx_eq(&self, other: &f64, epsilon: f32) -> bool {
        (self - other).abs() <= epsilon as f64
    }
}

// Implement for arrays
impl<T: ApproximateEq, const N: usize> ApproximateEq for [T; N] {
    fn approx_eq(&self, other: &Self, epsilon: f32) -> bool {
        self.iter()
            .zip(other.iter())
            .all(|(a, b)| a.approx_eq(b, epsilon))
    }
}

// Implement for tuples (up to 4 elements)
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

/// Macro for approximate equality assertion.
#[macro_export]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr) => {
        $crate::assert_approx_eq!($left, $right, 1e-6);
    };
    ($left:expr, $right:expr, $epsilon:expr) => {{
        use $crate::approx::ApproximateEq;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_approx_eq() {
        assert!(1.0f32.approx_eq(&1.0000001, 1e-6));
        assert!(!1.0f32.approx_eq(&1.1, 1e-6));
    }

    #[test]
    fn array_approx_eq() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0000001, 2.0000001, 3.0000001];
        assert!(a.approx_eq(&b, 1e-5));
    }

    #[test]
    fn tuple_approx_eq() {
        let a = (1.0, 2.0);
        let b = (1.0000001, 2.0000001);
        assert!(a.approx_eq(&b, 1e-5));
    }

    #[test]
    #[should_panic]
    fn macro_panics() {
        assert_approx_eq!(1.0, 1.1);
    }
}