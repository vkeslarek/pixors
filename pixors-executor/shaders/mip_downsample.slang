// mip_downsample.slang — 2×2 box filter downsample from 4 tile quadrants
//
// Uses IPixelCodec (same pattern as blur/color) to avoid uint2/float4 globals
// that naga cannot validate. All 4 source tiles + output use raw uint buffers.
//
// Coordinate model:
//   The "source space" for a given output tile is (2*out_width) × (2*out_height).
//   Each output pixel (gx, gy) reads the 2×2 block at source (gx*2, gy*2).
//   That source space is divided into 4 quadrants:
//     TL: src-x in [0, out_width),  src-y in [0, out_height)
//     TR: src-x in [out_width, ...), src-y in [0, out_height)
//     BL: src-x in [0, out_width),  src-y in [out_height, ...)
//     BR: src-x in [out_width, ...), src-y in [out_height, ...)
//
//   Within each quadrant tile, local pixel (lx, ly) is:
//     TL: lx = sx,           ly = sy
//     TR: lx = sx - out_width, ly = sy
//     BL: lx = sx,           ly = sy - out_height
//     BR: lx = sx - out_width, ly = sy - out_height
//
//   The row stride of each tile (physical tile width, NOT out_width):
//     stride_tl = w0  (= block.tiles[0].coord.width, typically 2*out_width for full tiles)
//     stride_tr = w1
//     stride_bl = w2
//     stride_br = w3
//
// Layout:
//   group 0, binding 0  → g_src_tl  (top-left  tile, raw uint)
//   group 0, binding 1  → g_src_tr  (top-right  tile, raw uint)
//   group 0, binding 2  → g_src_bl  (bot-left   tile, raw uint)
//   group 0, binding 3  → g_src_br  (bot-right  tile, raw uint)
//   group 0, binding 4  → g_dst     (output tile, raw uint)
//   group 1, binding 0  → g_params  (MipParams uniform)

import lib.color;  // IPixelCodec, U8Codec, U16Codec, F16Codec, F32Codec

struct MipParams {
    uint out_width;
    uint out_height;
    uint w0; uint h0;   // TL tile row stride (physical tile width/height)
    uint w1; uint h1;   // TR tile row stride
    uint w2; uint h2;   // BL tile row stride
    uint w3; uint h3;   // BR tile row stride
};

[[vk::binding(0, 0)]] RWStructuredBuffer<uint>   g_src_tl;
[[vk::binding(1, 0)]] RWStructuredBuffer<uint>   g_src_tr;
[[vk::binding(2, 0)]] RWStructuredBuffer<uint>   g_src_bl;
[[vk::binding(3, 0)]] RWStructuredBuffer<uint>   g_src_br;
[[vk::binding(4, 0)]] RWStructuredBuffer<uint>   g_dst;
[[vk::binding(0, 1)]] ConstantBuffer<MipParams>  g_params;

// ── Generic read from the correct quadrant buffer ─────────────────────────────
//
// sx, sy are in "source space" = combined width × combined height.
// Quadrant boundary is at the edge of the TL tile: (w0, h0).

float4 read_combined<C : IPixelCodec>(uint sx, uint sy)
{
    if (sy < g_params.h0) {
        if (sx < g_params.w0) {
            // TL quadrant — local (sx, sy), stride = w0
            uint lx = min(sx, g_params.w0 - 1u);
            uint ly = min(sy, g_params.h0 - 1u);
            return C.decode(g_src_tl, ly * g_params.w0 + lx, ChannelLayout::Rgba);
        } else {
            // TR quadrant — local (sx - w0, sy), stride = w1
            uint lx = min(sx - g_params.w0, g_params.w1 - 1u);
            uint ly = min(sy, g_params.h1 - 1u);
            return C.decode(g_src_tr, ly * g_params.w1 + lx, ChannelLayout::Rgba);
        }
    } else {
        if (sx < g_params.w0) {
            // BL quadrant — local (sx, sy - h0), stride = w2
            uint lx = min(sx, g_params.w2 - 1u);
            uint ly = min(sy - g_params.h0, g_params.h2 - 1u);
            return C.decode(g_src_bl, ly * g_params.w2 + lx, ChannelLayout::Rgba);
        } else {
            // BR quadrant — local (sx - w0, sy - h0), stride = w3
            uint lx = min(sx - g_params.w0, g_params.w3 - 1u);
            uint ly = min(sy - g_params.h0, g_params.h3 - 1u);
            return C.decode(g_src_br, ly * g_params.w3 + lx, ChannelLayout::Rgba);
        }
    }
}

// ── Generic kernel ────────────────────────────────────────────────────────────

void cs_mip_downsample_generic<C : IPixelCodec>(uint3 gid)
{
    if (gid.x >= g_params.out_width || gid.y >= g_params.out_height) return;

    uint sx = gid.x * 2u;
    uint sy = gid.y * 2u;

    float4 avg = (
        read_combined<C>(sx,      sy     ) +
        read_combined<C>(sx + 1u, sy     ) +
        read_combined<C>(sx,      sy + 1u) +
        read_combined<C>(sx + 1u, sy + 1u)
    ) * 0.25;

    C.encode(g_dst, gid.y * g_params.out_width + gid.x, avg, ChannelLayout::Rgba);
}

// ── Entry points ──────────────────────────────────────────────────────────────

[shader("compute")] [numthreads(8, 8, 1)]
void cs_mip_downsample_rgba8  (uint3 gid : SV_DispatchThreadID) { cs_mip_downsample_generic<U8Codec >(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_mip_downsample_rgba16 (uint3 gid : SV_DispatchThreadID) { cs_mip_downsample_generic<U16Codec>(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_mip_downsample_rgbaf16(uint3 gid : SV_DispatchThreadID) { cs_mip_downsample_generic<F16Codec>(gid); }

[shader("compute")] [numthreads(8, 8, 1)]
void cs_mip_downsample_rgbaf32(uint3 gid : SV_DispatchThreadID) { cs_mip_downsample_generic<F32Codec>(gid); }

// Legacy entry kept for compatibility (same as rgba8)
[shader("compute")] [numthreads(8, 8, 1)]
void cs_mip_downsample        (uint3 gid : SV_DispatchThreadID) { cs_mip_downsample_generic<U8Codec >(gid); }
