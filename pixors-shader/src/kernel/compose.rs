use pixors_shader_macro::kernel;

#[kernel(
    source   = "shaders/compose.slang",
    entry    = "cs_compose",
    body_fn  = "cs_compose",
    includes("shaders"),
    specialize(U8Codec  => Rgba8, Rgb8, Gray8, GrayA8, Cmyk8),
    specialize(U16Codec => Rgba16, Rgb16, Gray16, GrayA16),
    specialize(F16Codec => RgbaF16, RgbF16),
    specialize(F32Codec => RgbaF32, RgbF32, GrayF32),
    inputs(layer_a, layer_b),
    output = "dst",
    workgroup(8, 8, 1),
    dispatch(PerPixel),
    class(PerPixel),
)]
#[repr(C)]
pub struct ComposeParams {
    pub width: u32,
    pub height: u32,
    pub opacity_a: f32,
    pub opacity_b: f32,
}
