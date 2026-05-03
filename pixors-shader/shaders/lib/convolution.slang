// pixors-shader/shaders/lib/convolution.slang
// Generic convolution loop — iterates a stencil window.
// Specific kernels (box, gaussian, etc.) live in their own .slang files.

import lib.neighborhood;

// Sum all samples in an N×N window centered at `center_padded`.
// Returns unnormalized sum + count. Caller normalizes per its kernel.
float4 stencil_sum(Neighborhood nbhd, uint2 center_padded, int radius, out uint count)
{
    count = 0u;
    float4 sum = float4(0.0, 0.0, 0.0, 0.0);

    for (int dy = -radius; dy <= radius; dy++)
    {
        for (int dx = -radius; dx <= radius; dx++)
        {
            sum += nbhd.load(int2(center_padded) + int2(dx, dy));
            count++;
        }
    }

    return sum;
}
