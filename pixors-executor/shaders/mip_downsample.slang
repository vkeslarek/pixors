// mip_downsample.slang — 2×2 box filter downsample
// Reads 4 input tiles (top-left, top-right, bottom-left, bottom-right)
// and writes 1 output tile at half resolution, averaging 4→1 pixels.

struct MipParams { uint out_width; uint out_height; uint in00_width; uint in00_height; uint in01_width; uint in01_height; uint in10_width; uint in10_height; uint in11_width; uint in11_height; };
ParameterBlock<MipParams> params;

StructuredBuffer<uint> src00;
StructuredBuffer<uint> src01;
StructuredBuffer<uint> src10;
StructuredBuffer<uint> src11;
RWStructuredBuffer<uint> dst;

uint4 unpack_rgba8(uint packed)
{
    return uint4(packed & 0xFFu, (packed >> 8) & 0xFFu, (packed >> 16) & 0xFFu, (packed >> 24) & 0xFFu);
}

uint pack_rgba8(uint4 c)
{
    return c.r | (c.g << 8) | (c.b << 16) | (c.a << 24);
}

uint4 clamp_sample(StructuredBuffer<uint> src, uint x, uint y, uint w, uint h)
{
    x = min(x, w - 1u);
    y = min(y, h - 1u);
    return unpack_rgba8(src[y * w + x]);
}

[numthreads(8, 8, 1)]
void cs_mip_downsample(uint3 gid : SV_DispatchThreadID)
{
    if (gid.x >= params.out_width || gid.y >= params.out_height) return;

    uint ox = gid.x;
    uint oy = gid.y;
    uint sx = ox * 2u;
    uint sy = oy * 2u;

    uint4 c00 = clamp_sample(src00, sx, sy, params.in00_width, params.in00_height);
    uint4 c01 = clamp_sample(src01, sx, sy, params.in01_width, params.in01_height);
    uint4 c10 = clamp_sample(src10, sx, sy, params.in10_width, params.in10_height);
    uint4 c11 = clamp_sample(src11, sx, sy, params.in11_width, params.in11_height);

    uint4 avg = (c00 + c01 + c10 + c11) / 4u;
    dst[oy * params.out_width + ox] = pack_rgba8(avg);
}
