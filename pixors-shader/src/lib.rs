//! pixors-shader — compiled SPIR-V shader binaries.

pub const COLOR_SPV: &[u8] = include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/color.spv"));
pub const BLUR_SPV: &[u8] = include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/blur.spv"));
pub const MIP_DOWNSAMPLE_SPV: &[u8] =
    include_bytes!(concat!(env!("SHADER_OUT_DIR"), "/mip_downsample.spv"));
