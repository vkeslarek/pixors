fn unpack(p: u32) -> vec4<u32> {
    return vec4<u32>(p & 0xffu, (p >> 8u) & 0xffu, (p >> 16u) & 0xffu, (p >> 24u) & 0xffu);
}
fn pack(v: vec4<u32>) -> u32 {
    return (v.x & 0xffu) | ((v.y & 0xffu) << 8u) | ((v.z & 0xffu) << 16u) | ((v.w & 0xffu) << 24u);
}

fn blur_main(w: u32, h: u32, r: i32, gid_x: u32, gid_y: u32, src_ptr: ptr<storage, array<u32>, read>, dst_ptr: ptr<storage, array<u32>, read_write>) {
    if (gid_x >= w || gid_y >= h) { return; }
    var sum: vec4<u32> = vec4<u32>(0u);
    var count: u32 = 0u;
    let cx = i32(gid_x);
    let cy = i32(gid_y);
    for (var dy: i32 = -r; dy <= r; dy = dy + 1) {
        let yy = clamp(cy + dy, 0, i32(h) - 1);
        for (var dx: i32 = -r; dx <= r; dx = dx + 1) {
            let xx = clamp(cx + dx, 0, i32(w) - 1);
            var idx: u32 = u32(yy) * w + u32(xx);
            let p = unpack(src[idx]);
            sum += p;
            count += 1u;
        }
    }
    let out_idx: u32 = gid_y * w + gid_x;
    dst[out_idx] = pack(sum / vec4<u32>(count));
}

@compute @workgroup_size(8, 8, 1)
fn entry(@builtin(global_invocation_id) gid: vec3<u32>) {
    blur_main(params.width, params.height, i32(params.radius), gid.x, gid.y, &src, &dst);
}
