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
