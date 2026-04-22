//! Sample type and layout (abstract image metadata).

/// Numeric type of a single sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    /// Unsigned 8‑bit integer.
    U8,
    /// Unsigned 16‑bit integer.
    U16,
    /// Unsigned 32‑bit integer (rare; not supported in Phase 1).
    U32,
    /// 16‑bit floating‑point (IEEE‑754 binary16).
    F16,
    /// 32‑bit floating‑point (IEEE‑754 binary32).
    F32,
}

impl SampleType {
    /// Size in bytes of one sample.
    pub fn size_bytes(self) -> usize {
        match self {
            SampleType::U8 => 1,
            SampleType::U16 => 2,
            SampleType::U32 => 4,
            SampleType::F16 => 2,
            SampleType::F32 => 4,
        }
    }

    /// Returns `true` if the sample type is integer.
    pub fn is_integer(self) -> bool {
        matches!(self, SampleType::U8 | SampleType::U16 | SampleType::U32)
    }

    /// Returns `true` if the sample type is floating‑point.
    pub fn is_float(self) -> bool {
        matches!(self, SampleType::F16 | SampleType::F32)
    }

    /// Maximum representable value for integer types (2^bits - 1).
    /// For float types returns `1.0`.
    pub fn max_value(self) -> f32 {
        match self {
            SampleType::U8 => 255.0,
            SampleType::U16 => 65535.0,
            SampleType::U32 => 4294967295.0,
            SampleType::F16 => 1.0,
            SampleType::F32 => 1.0,
        }
    }

    /// Scaling factor to convert integer sample to `[0, 1]` floating‑point.
    /// For float types returns `1.0`.
    pub fn scale_to_f32(self) -> f32 {
        match self {
            SampleType::U8 => 1.0 / 255.0,
            SampleType::U16 => 1.0 / 65535.0,
            SampleType::U32 => 1.0 / 4294967295.0,
            SampleType::F16 | SampleType::F32 => 1.0,
        }
    }

    /// Scaling factor to convert `[0, 1]` floating‑point to integer sample.
    /// For float types returns `1.0`.
    pub fn scale_from_f32(self) -> f32 {
        match self {
            SampleType::U8 => 255.0,
            SampleType::U16 => 65535.0,
            SampleType::U32 => 4294967295.0,
            SampleType::F16 | SampleType::F32 => 1.0,
        }
    }
}

/// Arrangement of samples within a pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleLayout {
    /// Channels interleaved per pixel (`[R G B A R G B A …]`).
    Interleaved,
    /// Channels planar (`[R R R …, G G G …, B B B …, A A A …]`).
    Planar,
}

impl SampleLayout {
    /// Returns `true` if interleaved.
    pub fn is_interleaved(self) -> bool {
        matches!(self, SampleLayout::Interleaved)
    }

    /// Returns `true` if planar.
    pub fn is_planar(self) -> bool {
        matches!(self, SampleLayout::Planar)
    }
}