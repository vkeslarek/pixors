#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    U8,
    U16Le,
    U16Be,
    F32Le,
    F32Be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Straight,
    Premultiplied,
    Opaque,
}
