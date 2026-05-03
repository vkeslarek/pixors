// pixors-shader/shaders/blur.slang
// Box blur using pixors standard library.
// D3D layout: everything in register space 0 (single descriptor set).

import lib.pixel;
import lib.neighborhood;
import lib.convolution;

struct BlurParams { uint width; uint height; uint radius; uint _pad; };
ParameterBlock<BlurParams> params;

StructuredBuffer<uint> src;
RWStructuredBuffer<uint> dst;

float4 box_blur(Neighborhood nbhd, uint2 center_padded, uint radius)
{
    StencilResult r = stencil_sum(nbhd, center_padded, int(radius));
    return r.sum / float(r.count);
}

[numthreads(8, 8, 1)]
void cs_blur(uint3 gid : SV_DispatchThreadID)
{
    // params.width/height are **padded** dimensions (center + 2*radius).
    // Derive center (output) dimensions from them.
    Neighborhood nbhd = Neighborhood::bind(src, params.width, params.height, int(params.radius));

    // Bounds-check against the center (output) size, not padded.
    if (gid.x >= nbhd.center_size.x || gid.y >= nbhd.center_size.y) return;

    uint2 center = nbhd.center_offset + gid.xy;
    float4 result = box_blur(nbhd, center, params.radius);

    // Output buffer has center-sized stride.
    uint idx = gid.y * nbhd.center_size.x + gid.x;
    dst[idx] = rgba8_pack(result);
}
