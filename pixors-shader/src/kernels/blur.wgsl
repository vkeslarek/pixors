struct BlurParams {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> params: BlurParams;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> dst: array<u32>;

fn unpack(p: u32) -> vec4<u32> {
    return vec4(p & 0xFFu, (p >> 8u) & 0xFFu, (p >> 16u) & 0xFFu, (p >> 24u) & 0xFFu);
}
fn pack(v: vec4<u32>) -> u32 {
    return (v.x & 0xFFu) | ((v.y & 0xFFu) << 8u) | ((v.z & 0xFFu) << 16u) | ((v.w & 0xFFu) << 24u);
}

@compute @workgroup_size(8, 8, 1)
fn entry(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.width;
    let h = params.height;
    let r = i32(params.radius);
    if (gid.x >= w || gid.y >= h) { return; }
    var sum: vec4<u32> = vec4(0u);
    var count: u32 = 0u;
    let cx = i32(gid.x);
    let cy = i32(gid.y);
    for (var dy: i32 = -r; dy <= r; dy++) {
        let yy = clamp(cy + dy, 0, i32(h) - 1);
        for (var dx: i32 = -r; dx <= r; dx++) {
            let xx = clamp(cx + dx, 0, i32(w) - 1);
            let idx: u32 = u32(yy) * w + u32(xx);
            sum += unpack(src[idx]);
            count += 1u;
        }
    }
    let out_idx: u32 = gid.y * w + gid.x;
    dst[out_idx] = pack(sum / vec4(count));
}
