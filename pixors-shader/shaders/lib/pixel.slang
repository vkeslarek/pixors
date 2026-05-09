// pixors-shader/shaders/lib/pixel.slang
// RGBA8 pixel pack/unpack helpers

float4 rgba8_unpack(uint p)
{
    return float4(
        float(p & 0xFFu),
        float((p >> 8u) & 0xFFu),
        float((p >> 16u) & 0xFFu),
        float((p >> 24u) & 0xFFu)
    ) / 255.0;
}

uint rgba8_pack(float4 v)
{
    uint4 c = uint4(saturate(v) * 255.0 + 0.5);
    return c.x | (c.y << 8u) | (c.z << 16u) | (c.w << 24u);
}

// ── Generic Pixel Interface ─────────────────────────────────────────────────

interface IPixel {
    associatedtype StorageType;
    
    [mutating] void init(StorageType raw);
    StorageType getRawData();

    float4 toFloat4();
    [mutating] void fromFloat4(float4 color);
}

struct PixelRGBA8 : IPixel {
    typedef uint StorageType;
    uint packedValue;

    [mutating] void init(StorageType raw) { packedValue = raw; }
    StorageType getRawData() { return packedValue; }

    float4 toFloat4() {
        return rgba8_unpack(packedValue);
    }

    [mutating] void fromFloat4(float4 color) {
        packedValue = rgba8_pack(color);
    }
}

struct PixelRGBA16 : IPixel {
    typedef uint2 StorageType;
    uint2 packedValue;

    [mutating] void init(StorageType raw) { packedValue = raw; }
    StorageType getRawData() { return packedValue; }

    float4 toFloat4() {
        return float4(
            float(packedValue.x & 0xFFFFu),
            float(packedValue.x >> 16u),
            float(packedValue.y & 0xFFFFu),
            float(packedValue.y >> 16u)
        ) / 65535.0;
    }

    [mutating] void fromFloat4(float4 color) {
        uint4 c = uint4(saturate(color) * 65535.0 + 0.5);
        packedValue.x = c.x | (c.y << 16u);
        packedValue.y = c.z | (c.w << 16u);
    }
}

struct PixelRGBAF16 : IPixel {
    typedef uint2 StorageType;
    uint2 packedValue;

    [mutating] void init(StorageType raw) { packedValue = raw; }
    StorageType getRawData() { return packedValue; }

    float4 toFloat4() {
        return float4(
            f16tof32(packedValue.x & 0xFFFFu),
            f16tof32(packedValue.x >> 16u),
            f16tof32(packedValue.y & 0xFFFFu),
            f16tof32(packedValue.y >> 16u)
        );
    }

    [mutating] void fromFloat4(float4 color) {
        packedValue.x = (f32tof16(color.r) & 0xFFFFu) | (f32tof16(color.g) << 16u);
        packedValue.y = (f32tof16(color.b) & 0xFFFFu) | (f32tof16(color.a) << 16u);
    }
}

struct PixelRGBAF32 : IPixel {
    typedef float4 StorageType;
    float4 value;

    [mutating] void init(StorageType raw) { value = raw; }
    StorageType getRawData() { return value; }

    float4 toFloat4() { return value; }
    [mutating] void fromFloat4(float4 color) { value = color; }
}
