use crate::image::TileCoord;
use crate::color::ColorSpace;
use std::borrow::Cow;

/// A frame in the tile processing pipeline. Flows through Pipes.
/// `.data` is the raw pixel bytes. Pipes may modify it in-place via Cow.
#[derive(Clone)]
pub struct Frame {
    pub meta: FrameMeta,
    pub kind: FrameKind,
    pub data: Cow<'static, [u8]>,
}

/// Metadata attached to every frame — stable identity for consumers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameMeta {
    pub layer_id:  u32,
    pub mip_level: u32,
    pub image_w:   u32,
    pub image_h:   u32,
    pub color_space: ColorSpace,
    /// How many tiles total in this layer×mip level (0 = unknown).
    pub total_tiles: u32,
    /// Generation counter — invalidates stale frames when tab/zoom/op changes.
    pub generation: u64,
}

/// What kind of frame this is — never mutated by pipes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameKind {
    Tile { coord: TileCoord },
    Progress { done: u32, total: u32 },
    LayerDone,
    MipLevelDone { level: u32 },
    StreamDone,
}

impl Frame {
    pub fn new(meta: FrameMeta, kind: FrameKind, data: impl Into<Cow<'static, [u8]>>) -> Self {
        Self { meta, kind, data: data.into() }
    }

    pub fn is_tile(&self) -> bool { matches!(self.kind, FrameKind::Tile { .. }) }
    pub fn is_progress(&self) -> bool { matches!(self.kind, FrameKind::Progress { .. }) }
    pub fn is_terminal(&self) -> bool { matches!(self.kind, FrameKind::StreamDone) }
}
