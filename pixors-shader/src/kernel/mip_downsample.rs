use pixors_shader_macro::kernel;

#[kernel(
    source   = "shaders/mip_downsample.slang",
    entry    = "cs_mip_downsample",
    body_fn  = "cs_mip_downsample",
    includes("shaders"),
    specialize(U8Codec  => Rgba8, Rgb8, Gray8, GrayA8, Cmyk8),
    specialize(U16Codec => Rgba16, Rgb16, Gray16, GrayA16),
    specialize(F16Codec => RgbaF16, RgbF16),
    specialize(F32Codec => RgbaF32, RgbF32, GrayF32),
    inputs(src_tl, src_tr, src_bl, src_br),
    output = "dst",
    workgroup(8, 8, 1),
    dispatch(PerPixel),
    class(PerPixel),
)]
#[repr(C)]
pub struct MipParams {
    pub out_width: u32,
    pub out_height: u32,
    pub w0: u32,
    pub h0: u32,
    pub w1: u32,
    pub h1: u32,
    pub w2: u32,
    pub h2: u32,
    pub w3: u32,
    pub h3: u32,
}
