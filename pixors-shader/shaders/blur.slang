// shaders/blur.slang
// Slang box-blur kernel. Compiles to WGSL via slangc.
// entry_point: cs_blur

struct BlurParams {
    uint width;
    uint height;
    int radius;
    uint _pad;
};

ParameterBlock<BlurParams> params;
StructuredBuffer<uint> src;
RWStructuredBuffer<uint> dst;

uint4 unpack(uint p) {
    return uint4(p & 0xffu, (p >> 8u) & 0xffu, (p >> 16u) & 0xffu, (p >> 24u) & 0xffu);
}
uint pack(uint4 v) {
    return (v.x & 0xffu) | ((v.y & 0xffu) << 8u) | ((v.z & 0xffu) << 16u) | ((v.w & 0xffu) << 24u);
}

[numthreads(8, 8, 1)]
void cs_blur(uint3 gid : SV_DispatchThreadID) {
    uint w = params.width;
    uint h = params.height;
    int r = params.radius;
    if (gid.x >= w || gid.y >= h) return;

    uint4 sum = uint4(0u);
    uint count = 0u;
    int cx = int(gid.x);
    int cy = int(gid.y);

    for (int dy = -r; dy <= r; dy++) {
        int yy = clamp(cy + dy, 0, int(h) - 1);
        for (int dx = -r; dx <= r; dx++) {
            int xx = clamp(cx + dx, 0, int(w) - 1);
            uint idx = uint(yy) * w + uint(xx);
            sum += unpack(src[idx]);
            count++;
        }
    }
    uint oi = gid.y * w + gid.x;
    dst[oi] = pack(sum / uint4(count));
}
