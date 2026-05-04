// mip_downsample.slang — 2×2 box filter downsample from 4 tile quadrants
//
// Input:  4 buffers (TL, TR, BL, BR) forming a 2×2 tile block.
// Output: 1 buffer at half resolution.
//
// Each output pixel (ox, oy) averages a 2×2 region from the combined
// source image. The quadrant boundary is at (w0, h0) — the TL tile size.

struct MipParams {
    uint out_width;
    uint out_height;
    uint w0; uint h0;   // TL tile dimensions (= quadrant boundary)
    uint w1; uint h1;   // TR tile dimensions
    uint w2; uint h2;   // BL tile dimensions
    uint w3; uint h3;   // BR tile dimensions
};
ParameterBlock<MipParams> params;

StructuredBuffer<uint> src_tl;
StructuredBuffer<uint> src_tr;
StructuredBuffer<uint> src_bl;
StructuredBuffer<uint> src_br;
RWStructuredBuffer<uint> dst;

uint4 unpack_rgba8(uint packed)
{
    return uint4(packed & 0xFFu, (packed >> 8) & 0xFFu, (packed >> 16) & 0xFFu, (packed >> 24) & 0xFFu);
}

uint pack_rgba8(uint4 c)
{
    return c.r | (c.g << 8) | (c.b << 16) | (c.a << 24);
}

/// Read a pixel from the combined 2×2 block at absolute coordinates (sx, sy).
/// Routes to the correct tile quadrant and clamps within tile bounds.
uint4 read_combined(uint sx, uint sy)
{
    if (sy < params.h0) {
        if (sx < params.w0) {
            uint cx = min(sx, params.w0 - 1u);
            uint cy = min(sy, params.h0 - 1u);
            return unpack_rgba8(src_tl[cy * params.w0 + cx]);
        } else {
            uint cx = min(sx - params.w0, params.w1 - 1u);
            uint cy = min(sy, params.h1 - 1u);
            return unpack_rgba8(src_tr[cy * params.w1 + cx]);
        }
    } else {
        if (sx < params.w0) {
            uint cx = min(sx, params.w2 - 1u);
            uint cy = min(sy - params.h0, params.h2 - 1u);
            return unpack_rgba8(src_bl[cy * params.w2 + cx]);
        } else {
            uint cx = min(sx - params.w0, params.w3 - 1u);
            uint cy = min(sy - params.h0, params.h3 - 1u);
            return unpack_rgba8(src_br[cy * params.w3 + cx]);
        }
    }
}

[numthreads(8, 8, 1)]
void cs_mip_downsample(uint3 gid : SV_DispatchThreadID)
{
    if (gid.x >= params.out_width || gid.y >= params.out_height) return;

    uint sx = gid.x * 2u;
    uint sy = gid.y * 2u;

    uint4 c00 = read_combined(sx,     sy    );
    uint4 c10 = read_combined(sx + 1u, sy    );
    uint4 c01 = read_combined(sx,     sy + 1u);
    uint4 c11 = read_combined(sx + 1u, sy + 1u);

    uint4 avg = (c00 + c10 + c01 + c11) / 4u;
    dst[gid.y * params.out_width + gid.x] = pack_rgba8(avg);
}
