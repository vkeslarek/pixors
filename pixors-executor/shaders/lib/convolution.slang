// pixors-shader/shaders/lib/convolution.slang
// Generic convolution loop — iterates a stencil window.

import lib.neighborhood;

// Return type for stencil_sum: unnormalized sum + pixel count.
struct StencilResult { float4 sum; uint count; };

StencilResult stencil_sum(Neighborhood nbhd, uint2 center_padded, int radius)
{
    StencilResult r;
    r.count = 0u;
    r.sum = float4(0.0, 0.0, 0.0, 0.0);

    for (int dy = -radius; dy <= radius; dy++)
    {
        for (int dx = -radius; dx <= radius; dx++)
        {
            r.sum += nbhd.load(int2(center_padded) + int2(dx, dy));
            r.count++;
        }
    }

    return r;
}
