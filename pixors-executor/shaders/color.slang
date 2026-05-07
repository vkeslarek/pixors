// pixors-shader/shaders/color.slang
//
// All resources declared at global scope with explicit Vulkan binding slots.
// Slang dead-strips unreferenced globals per entry point compilation.
//
// Layout (matches scheduler):
//   group 0, binding 0  → g_buf0  (input / in-place buffer)
//   group 0, binding 1  → g_buf1  (output buffer, out-of-place only)
//   group 1, binding 0  → g_params (uniform ConstantBuffer)

import lib.color;

[[vk::binding(0, 0)]] RWStructuredBuffer<uint> g_buf0;
[[vk::binding(1, 0)]] RWStructuredBuffer<uint> g_buf1;
[[vk::binding(0, 1)]] ConstantBuffer<ColorConvertParams> g_params;

// ── In-place kernels (only g_buf0 + g_params) ───────────────────────────────

void cc_kernel_ip<C : IPixelCodec>(uint3 id) {
    if (id.x >= g_params.width || id.y >= g_params.height) return;
    uint idx = id.y * g_params.width + id.x;
    float4 raw   = C.decode(g_buf0, idx, g_params.src_channels);
    float  extra = C.decode_extra(g_buf0, idx, g_params.src_channels);
    C.encode(g_buf0, idx, color_convert(raw, extra, g_params), g_params.dst_channels);
}

[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u8_u8_ip  (uint3 id : SV_DispatchThreadID) { cc_kernel_ip<U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u16_u16_ip(uint3 id : SV_DispatchThreadID) { cc_kernel_ip<U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f16_f16_ip(uint3 id : SV_DispatchThreadID) { cc_kernel_ip<F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f32_f32_ip(uint3 id : SV_DispatchThreadID) { cc_kernel_ip<F32Codec>(id); }

// ── Out-of-place kernels (g_buf0 → g_buf1 + g_params) ───────────────────────

void cc_kernel<SRC : IPixelCodec, DST : IPixelCodec>(uint3 id) {
    if (id.x >= g_params.width || id.y >= g_params.height) return;
    uint idx = id.y * g_params.width + id.x;
    float4 raw   = SRC.decode(g_buf0, idx, g_params.src_channels);
    float  extra = SRC.decode_extra(g_buf0, idx, g_params.src_channels);
    DST.encode(g_buf1, idx, color_convert(raw, extra, g_params), g_params.dst_channels);
}

[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u8_u8  (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u8_u16 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u8_f16 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u8_f32 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u16_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u16_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u16_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_u16_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f16_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f16_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f16_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f16_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f32_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f32_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f32_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_f32_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, F32Codec>(id); }

// ── 5-channel kernels (CMYKA — all src×dst precision combinations) ───────────
// decode_extra reads the 5th channel (alpha); CmykA → RGBA uses cc_kernel.
// 16 entries covering (U8/U16/F16/F32) × (U8/U16/F16/F32).

[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u8_u8  (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u8_u16 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u8_f16 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u8_f32 (uint3 id : SV_DispatchThreadID) { cc_kernel<U8Codec,  F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u16_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u16_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u16_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_u16_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<U16Codec, F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f16_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f16_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f16_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f16_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<F16Codec, F32Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f32_u8 (uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, U8Codec >(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f32_u16(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, U16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f32_f16(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, F16Codec>(id); }
[shader("compute")] [numthreads(8, 8, 1)]
void cs_cc_5ch_f32_f32(uint3 id : SV_DispatchThreadID) { cc_kernel<F32Codec, F32Codec>(id); }
