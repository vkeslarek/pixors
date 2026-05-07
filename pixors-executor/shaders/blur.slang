// pixors-shader/shaders/blur.slang
//
// Generic box blur using the IPixelCodec interface (same pattern as color.slang).
// A single pair of raw uint buffers is used for all pixel formats — no dead-strip
// issues, no uint2/float4 naga compatibility problems.
//
// Layout:
//   group 0, binding 0  → g_src  (padded neighborhood, raw uint)
//   group 0, binding 1  → g_dst  (center-only output, raw uint)
//   group 1, binding 0  → g_params (BlurParams uniform)
//
// BlurParams.width/height are the PADDED dimensions (center + 2*radius).
// g_dst receives only the center tile: (width-2r) × (height-2r).

import lib.color;  // IPixelCodec, U8Codec, U16Codec, F16Codec, F32Codec, ChannelLayout

struct BlurParams { uint width; uint height; uint radius; uint _pad; };

[[vk::binding(0, 0)]] RWStructuredBuffer<uint>    g_src;
[[vk::binding(1, 0)]] RWStructuredBuffer<uint>    g_dst;
[[vk::binding(0, 1)]] ConstantBuffer<BlurParams>  g_params;

// ── Generic kernel ────────────────────────────────────────────────────────────
//
// Reads a (2r+1)×(2r+1) neighbourhood from g_src (padded buffer) using codec C,
// averages in float4 space, and writes the result to g_dst (centre buffer).

void cs_blur_generic<C : IPixelCodec>(uint3 gid)
{
    uint pad_w    = g_params.width;
    uint pad_h    = g_params.height;
    uint r        = g_params.radius;
    uint center_w = pad_w - 2u * r;
    uint center_h = pad_h - 2u * r;

    if (gid.x >= center_w || gid.y >= center_h) return;

    // Centre pixel coordinates in the padded buffer.
    int px = int(gid.x + r);
    int py = int(gid.y + r);
    int ir = int(r);

    float4 sum = float4(0.0, 0.0, 0.0, 0.0);
    uint count = 0u;

    for (int dy = -ir; dy <= ir; dy++) {
        for (int dx = -ir; dx <= ir; dx++) {
            int sx = clamp(px + dx, 0, int(pad_w) - 1);
            int sy = clamp(py + dy, 0, int(pad_h) - 1);
            uint idx = uint(sy) * pad_w + uint(sx);
            sum += C.decode(g_src, idx, ChannelLayout::Rgba);
            count += 1u;
        }
    }
    float4 result = sum / float(count);
    uint center_idx = gid.y * center_w + gid.x;
    C.encode(g_dst, center_idx, result, ChannelLayout::Rgba);
}

// ── Entry points ──────────────────────────────────────────────────────────────

[shader("compute")] [numthreads(8, 8, 1)]
void cs_blur_rgba8  (uint3 gid : SV_DispatchThreadID) { cs_blur_generic<U8Codec >(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_blur_rgba16 (uint3 gid : SV_DispatchThreadID) { cs_blur_generic<U16Codec>(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_blur_rgbaf16(uint3 gid : SV_DispatchThreadID) { cs_blur_generic<F16Codec>(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_blur_rgbaf32(uint3 gid : SV_DispatchThreadID) { cs_blur_generic<F32Codec>(gid); }
