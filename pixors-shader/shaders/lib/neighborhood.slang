// pixors-shader/shaders/lib/neighborhood.slang

import lib.pixel;

struct Neighborhood
{
    StructuredBuffer<uint> data;
    uint  padded_width;
    uint  padded_height;
    uint2 center_offset;
    uint2 center_size;
    int   radius;

    // Construct from a padded source buffer + dimensions.
    // width  = center_w + 2*radius, padded rows in the buffer.
    // height = center_h + 2*radius, padded rows in the buffer.
    // radius = padding in pixels.
    static Neighborhood bind(
        StructuredBuffer<uint> src,
        uint width,
        uint height,
        int radius)
    {
        Neighborhood n;
        n.data = src;
        n.padded_width = width;
        n.padded_height = height;
        n.center_offset = uint2(uint(radius), uint(radius));
        n.center_size = uint2(width - 2u * uint(radius), height - 2u * uint(radius));
        n.radius = radius;
        return n;
    }

    float4 load(int2 padded_xy)
    {
        int2 clamped_xy = clamp(padded_xy,
            int2(0, 0),
            int2(int(padded_width) - 1, int(padded_height) - 1));
        uint idx = uint(clamped_xy.y) * padded_width + uint(clamped_xy.x);
        return rgba8_unpack(data[idx]);
    }
}
