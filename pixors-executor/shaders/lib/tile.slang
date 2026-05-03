// pixors-shader/shaders/lib/tile.slang
// Tile<T> — rectangular pixel region within a flat buffer

import lib.pixel;

struct Tile
{
    StructuredBuffer<uint> data;
    uint2 origin;        // (px, py) in image space
    uint tile_width;     // width in pixels
    uint tile_height;    // height in pixels
    uint image_stride;   // full width of the buffer (padded)
}

extension Tile
{
    float4 load(uint2 local_xy)
    {
        uint idx = (origin.y + local_xy.y) * image_stride + (origin.x + local_xy.x);
        return RGBA8::unpack(data[idx]);
    }

    void store(uint2 local_xy, float4 color)
    {
        uint idx = (origin.y + local_xy.y) * image_stride + (origin.x + local_xy.x);
        // NOTE: store not supported on read-only StructuredBuffer
    }
}

// RW variant for output tiles
struct RWTile
{
    RWStructuredBuffer<uint> data;
    uint2 origin;
    uint tile_width;
    uint tile_height;
    uint image_stride;
}

extension RWTile
{
    float4 load(uint2 local_xy)
    {
        uint idx = (origin.y + local_xy.y) * image_stride + (origin.x + local_xy.x);
        return RGBA8::unpack(data[idx]);
    }

    void store(uint2 local_xy, float4 color)
    {
        uint idx = (origin.y + local_xy.y) * image_stride + (origin.x + local_xy.x);
        data[idx] = RGBA8::pack(color);
    }
}
