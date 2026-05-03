# GPU Eliding — Phase 2: WGSL Codegen Module

## File to Create: `pixors-shader/src/codegen.rs`

This module generates WGSL source code for a fused N-pass blur chain.

### Purpose

Given `N` blur radii (e.g., `[8, 8]` for double-blur), produce a WGSL string
with:
- Shared utility functions (pack/unpack, stencil logic)
- N uniform bindings for per-pass params
- N+1 storage bindings (src, tmp_0, ..., tmp_{N-2}, dst)
- N entry points, each applying one blur pass

### Binding layout (N passes)

| binding | type | description |
|---|---|---|
| 0 | uniform | BlurParams for pass 0 |
| 1 | uniform | BlurParams for pass 1 |
| … | uniform | BlurParams for pass k |
| N | storage read | src |
| N+1 | storage r/w | tmp (pass 0 → pass 1) |
| … | storage r/w | tmp_{k} |
| 2N | storage r/w | dst (output of last pass) |

### Full code for `codegen.rs`

```rust
/// Input buffer name for pass `i`.
fn in_buf(i: usize, n: usize) -> String {
    if i == 0 {
        "src".to_string()
    } else if i == n {
        "dst".to_string()
    } else {
        format!("tmp_{}", i - 1)
    }
}

/// Output buffer name for pass `i`.
fn out_buf(i: usize, n: usize) -> String {
    if i == n - 1 {
        "dst".to_string()
    } else {
        format!("tmp_{}", i)
    }
}

pub struct FusedBlurShader {
    pub wgsl: String,
    /// Entry point names in dispatch order.
    pub entry_points: Vec<String>,
    /// Number of uniform bindings (one per pass).
    pub num_params: usize,
    /// Total number of storage bindings (N+1).
    pub num_buffers: usize,
}

/// Generate a fused WGSL module for `n` sequential blur passes.
/// All passes share the same workgroup topology and utility functions.
/// The `radii` slice must be non-empty.
pub fn gen_fused_blur(radii: &[u32]) -> FusedBlurShader {
    assert!(!radii.is_empty(), "need at least one pass");
    let n = radii.len();
    let mut src = String::new();

    // ── BlurParams struct (shared by all passes) ──────────────────────────
    src.push_str(
        "struct BlurParams {\n\
         @align(16) width:  u32,\n\
         @align(4)  height: u32,\n\
         @align(8)  radius: u32,\n\
         @align(4)  _pad:   u32,\n\
         };\n\n",
    );

    // ── Uniform bindings (one per pass) ───────────────────────────────────
    for i in 0..n {
        src.push_str(&format!(
            "@group(0) @binding({i}) var<uniform> params_{i}: BlurParams;\n"
        ));
    }
    src.push('\n');

    // ── Storage bindings: src, tmp_0..tmp_{n-2}, dst ──────────────────────
    // binding N = src (read-only)
    src.push_str(&format!(
        "@group(0) @binding({n}) var<storage, read> src: array<u32>;\n"
    ));
    // bindings N+1 .. 2N-1 = intermediates tmp_0 .. tmp_{n-2}
    for k in 0..(n - 1) {
        let b = n + 1 + k;
        src.push_str(&format!(
            "@group(0) @binding({b}) var<storage, read_write> tmp_{k}: array<u32>;\n"
        ));
    }
    // binding 2N = dst (output)
    let dst_binding = 2 * n;
    src.push_str(&format!(
        "@group(0) @binding({dst_binding}) var<storage, read_write> dst: array<u32>;\n\n"
    ));

    // ── Utility functions ─────────────────────────────────────────────────
    src.push_str(BLUR_UTILS);

    // ── Entry points ──────────────────────────────────────────────────────
    let mut entry_points = Vec::with_capacity(n);
    for i in 0..n {
        let ep = format!("cs_blur_{i}");
        let in_name = if i == 0 {
            "src".to_string()
        } else if i == 1 {
            "tmp_0".to_string()
        } else {
            format!("tmp_{}", i - 1)
        };
        let out_name = if i == n - 1 {
            "dst".to_string()
        } else {
            format!("tmp_{i}")
        };
        src.push_str(&gen_blur_entry(i, &ep, &in_name, &out_name));
        entry_points.push(ep);
    }

    FusedBlurShader {
        wgsl: src,
        entry_points,
        num_params: n,
        num_buffers: n + 1,
    }
}

fn gen_blur_entry(pass: usize, ep: &str, in_buf: &str, out_buf: &str) -> String {
    format!(
        "@compute\n\
         @workgroup_size(8, 8, 1)\n\
         fn {ep}(@builtin(global_invocation_id) gid: vec3<u32>) {{\n\
         \tlet p = params_{pass};\n\
         \tif gid.x >= p.width || gid.y >= p.height {{ return; }}\n\
         \tlet idx  = gid.y * p.width + gid.x;\n\
         \tlet r    = i32(p.radius);\n\
         \tlet pw   = p.width;\n\
         \tlet ph   = p.height;\n\
         \tvar sum  = vec4<f32>(0.0);\n\
         \tvar cnt  = 0u;\n\
         \tfor (var dy: i32 = -r; dy <= r; dy = dy + 1) {{\n\
         \t\tfor (var dx: i32 = -r; dx <= r; dx = dx + 1) {{\n\
         \t\t\tlet sx = i32(gid.x) + dx;\n\
         \t\t\tlet sy = i32(gid.y) + dy;\n\
         \t\t\tif sx >= 0 && sx < i32(pw) && sy >= 0 && sy < i32(ph) {{\n\
         \t\t\t\tsum = sum + rgba8_unpack({in_buf}[u32(sy) * pw + u32(sx)]);\n\
         \t\t\t\tcnt = cnt + 1u;\n\
         \t\t\t}}\n\
         \t\t}}\n\
         \t}}\n\
         \t{out_buf}[idx] = rgba8_pack(sum / vec4<f32>(f32(cnt)));\n\
         }}\n\n"
    )
}

const BLUR_UTILS: &str = r#"
fn rgba8_unpack(p: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(p & 255u),
        f32((p >> 8u) & 255u),
        f32((p >> 16u) & 255u),
        f32((p >> 24u) & 255u),
    ) / 255.0;
}

fn rgba8_pack(v: vec4<f32>) -> u32 {
    let c = vec4<u32>(clamp(v, vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0 + 0.5);
    return c.x | (c.y << 8u) | (c.z << 16u) | (c.w << 24u);
}

"#;
```

### Key design notes

- `gen_fused_blur(&[8, 8])` → WGSL with entry points `["cs_blur_0", "cs_blur_1"]`
- For N=2: 2 uniform bindings (0, 1), 3 storage bindings (2=src, 3=tmp_0, 4=dst)
- Box blur without the padded-neighborhood trick — reads within-bounds pixels
  only, clamps at image edge. This matches what the WGSL fallback already does.
- No intermediate neighborhood buffer — operates directly on the flat RGBA8
  packed buffer.

### Register codegen in `pixors-shader/src/lib.rs`

Add to `pixors-shader/src/lib.rs`:
```rust
pub mod codegen;
```

## Supporting changes to `pixors-shader/src/scheduler.rs`

The existing `build_pipeline` (line ~176) only handles SPIR-V (`sig.body`).
We need a path that accepts a WGSL string.

Add a new function after `build_pipeline`:

```rust
/// Build a compute pipeline from WGSL source (for runtime-generated shaders).
fn build_pipeline_wgsl(
    device: &wgpu::Device,
    wgsl: &str,
    entry_point: &str,
    bgl: Arc<wgpu::BindGroupLayout>,
) -> Result<CachedPipeline, String> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("fused_wgsl"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(wgsl)),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("fused_layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = Arc::new(device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("fused_blur"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    }));
    Ok(CachedPipeline { pipeline, bgl, has_params: true })
}
```

Also add a helper to build a BGL for the fused N-blur layout:

```rust
pub fn build_fused_blur_bgl(device: &wgpu::Device, n: usize) -> Arc<wgpu::BindGroupLayout> {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
    // N uniform bindings
    for b in 0..n {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: b as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    // N+1 storage bindings: src(read), tmp_0..tmp_{n-2}(r/w), dst(r/w)
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: n as u32,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    });
    for k in 0..n {  // intermediates + dst
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: (n + 1 + k) as u32,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    Arc::new(device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fused_blur_bgl"),
        entries: &entries,
    }))
}
```

## Verify

After writing the codegen module:
```bash
cargo check -p pixors-shader
```

Write a quick test to verify output:
```rust
#[test]
fn codegen_double_blur_bindings() {
    let s = pixors_shader::codegen::gen_fused_blur(&[8, 8]);
    assert_eq!(s.entry_points, ["cs_blur_0", "cs_blur_1"]);
    assert!(s.wgsl.contains("binding(0)") && s.wgsl.contains("binding(4)"));
    assert!(s.wgsl.contains("var<uniform> params_0"));
    assert!(s.wgsl.contains("var<storage, read> src"));
    assert!(s.wgsl.contains("tmp_0"));
    assert!(s.wgsl.contains("var<storage, read_write> dst"));
}
```
