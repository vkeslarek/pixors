struct CameraUniform {
    uv_offset: vec2<f32>,
    uv_scale: vec2<f32>,
    image_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var t_image: texture_2d<f32>;
@group(0) @binding(2) var s_image: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: VertexOutput;
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = camera.uv_offset + in.uv * camera.uv_scale;
    
    // Clip outside image bounds
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return vec4<f32>(0.0067, 0.0067, 0.0071, 1.0);
    }
    
    var final_color = textureSample(t_image, s_image, uv);
    
    // Pixel grid calculation
    let grid_uv = uv * camera.image_size;
    let fw = fwidth(grid_uv);
    let px_size = 1.0 / max(max(fw.x, fw.y), 0.0001);
    
    // Show grid when 1 image pixel is larger than 8 screen pixels
    if px_size > 8.0 {
        let grid_dist = fract(grid_uv);
        
        // Anti-aliased 1px line
        let line_x = smoothstep(fw.x * 1.0, fw.x * 0.0, grid_dist.x) + smoothstep(fw.x * 1.0, fw.x * 0.0, 1.0 - grid_dist.x);
        let line_y = smoothstep(fw.y * 1.0, fw.y * 0.0, grid_dist.y) + smoothstep(fw.y * 1.0, fw.y * 0.0, 1.0 - grid_dist.y);
        let line = clamp(line_x + line_y, 0.0, 1.0);
        
        if line > 0.0 {
            // Fade in between 8x and 12x zoom
            let alpha = clamp((px_size - 8.0) / 4.0, 0.0, 0.25) * line;
            
            // Contrast color: black on bright pixels, white on dark pixels
            let luma = dot(final_color.rgb, vec3<f32>(0.299, 0.587, 0.114));
            let grid_color = select(vec3<f32>(1.0), vec3<f32>(0.0), luma > 0.5);
            
            final_color = vec4<f32>(mix(final_color.rgb, grid_color, alpha), final_color.a);
        }
    }
    
    return final_color;
}
