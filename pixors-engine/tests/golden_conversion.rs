//! Golden bit-exact regression tests.
//!
//! Captures the output of the NEW conversion pipeline and verifies it matches
//! the pre-refactor reference. Every refactor step must produce byte-identical output.
//!
//! To regenerate golden files: `GOLDEN_WRITE=1 cargo test --test golden_conversion`
//! To verify: `cargo test --test golden_conversion`

use pixors_engine::color::ColorSpace;
use pixors_engine::pixel::AlphaPolicy;
use pixors_engine::image::{
    buffer::BufferDesc, AlphaMode, ImageBuffer,
};
use pixors_engine::pixel::Rgba;
use half::f16;
use std::path::PathBuf;
use std::io::Write;

// ---------------------------------------------------------------------------
// Deterministic input generation
// ---------------------------------------------------------------------------

struct DeterministicRng { state: u64 }

impl DeterministicRng {
    fn new(seed: u64) -> Self { Self { state: seed } }
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state
    }
    fn u8(&mut self) -> u8 { (self.next() >> 32) as u8 }
    fn u16(&mut self) -> u16 { (self.next() >> 32) as u16 }
}

fn make_srgb_u8_image() -> ImageBuffer {
    let w: u32 = 256;
    let h: u32 = 256;
    let desc = BufferDesc::rgba8_interleaved(w, h, ColorSpace::SRGB, AlphaMode::Straight);
    let mut buf = ImageBuffer::allocate(desc);
    let mut rng = DeterministicRng::new(0xDEAD_BEEF_CAFE_F00D);
    let row_stride = w as usize * 4;
    for y in 0..h {
        let row_off = y as usize * row_stride;
        for x in 0..w {
            let off = row_off + x as usize * 4;
            buf.data[off] = rng.u8();
            buf.data[off + 1] = rng.u8();
            buf.data[off + 2] = rng.u8();
            buf.data[off + 3] = rng.u8();
        }
    }
    buf
}

fn make_acescg_f16_pixels() -> Vec<Rgba<f16>> {
    let count = 256 * 256;
    let mut rng = DeterministicRng::new(0xBEEF_CAFE_DEAD_F00D);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let r = f16::from_f32((rng.u8() as f32) / 255.0);
        let g = f16::from_f32((rng.u8() as f32) / 255.0);
        let b = f16::from_f32((rng.u8() as f32) / 255.0);
        let a = f16::from_f32((rng.u8() as f32) / 255.0);
        out.push(Rgba::new(r, g, b, a));
    }
    out
}

fn make_rgba16_image() -> ImageBuffer {
    let w: u32 = 64;
    let h: u32 = 64;
    let desc = BufferDesc::rgba16_interleaved(w, h, ColorSpace::SRGB, AlphaMode::Straight);
    let mut buf = ImageBuffer::allocate(desc);
    let mut rng = DeterministicRng::new(0xCAFE_F00D_DEAD_BEEF);
    let row_bytes = w as usize * 8;
    for y in 0..h {
        let row_off = y as usize * row_bytes;
        for x in 0..w {
            let off = row_off + x as usize * 8;
            let r = rng.u16().to_ne_bytes();
            let g = rng.u16().to_ne_bytes();
            let b = rng.u16().to_ne_bytes();
            let a = rng.u16().to_ne_bytes();
            buf.data[off] = r[0];
            buf.data[off + 1] = r[1];
            buf.data[off + 2] = g[0];
            buf.data[off + 3] = g[1];
            buf.data[off + 4] = b[0];
            buf.data[off + 5] = b[1];
            buf.data[off + 6] = a[0];
            buf.data[off + 7] = a[1];
        }
    }
    buf
}

fn make_gray8_image() -> ImageBuffer {
    let w: u32 = 64;
    let h: u32 = 64;
    let desc = BufferDesc::gray8_interleaved(w, h, ColorSpace::SRGB, AlphaMode::Opaque);
    let mut buf = ImageBuffer::allocate(desc);
    let mut rng = DeterministicRng::new(0x1111_2222_3333_4444);
    for y in 0..h {
        let row_off = y as usize * w as usize;
        for x in 0..w {
            buf.data[row_off + x as usize] = rng.u8();
        }
    }
    buf
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

fn serialize_f16_pixels(pixels: &[Rgba<f16>]) -> Vec<u8> {
    pixels.iter().flat_map(|p| {
        let mut bytes = [0u8; 8];
        bytes[0..2].copy_from_slice(&p.r.to_le_bytes());
        bytes[2..4].copy_from_slice(&p.g.to_le_bytes());
        bytes[4..6].copy_from_slice(&p.b.to_le_bytes());
        bytes[6..8].copy_from_slice(&p.a.to_le_bytes());
        bytes.into_iter().collect::<Vec<_>>()
    }).collect()
}

// ---------------------------------------------------------------------------
// Golden file helpers
// ---------------------------------------------------------------------------

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

fn is_write_mode() -> bool {
    std::env::var("GOLDEN_WRITE").is_ok()
}

fn read_golden(name: &str) -> Option<Vec<u8>> {
    let path = golden_dir().join(name);
    if path.exists() {
        Some(std::fs::read(&path).unwrap())
    } else {
        None
    }
}

fn write_golden(name: &str, data: &[u8]) {
    let path = golden_dir().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(data).unwrap();
    eprintln!("Wrote golden: {}", path.display());
}

// ---------------------------------------------------------------------------
// Test 1: sRGB u8 ImageBuffer → ACEScg f16 premul (via new pipeline)
// ---------------------------------------------------------------------------

#[test]
fn srgb_u8_to_acescg_f16_bitexact() {
    let buf = make_srgb_u8_image();
    let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
    let pixels: Vec<Rgba<f16>> = conv.convert_buffer(&buf, AlphaPolicy::PremultiplyOnPack);
    let output = serialize_f16_pixels(&pixels);

    let golden_name = "srgb_u8_to_acescg.bin";
    if is_write_mode() {
        write_golden(golden_name, &output);
    }
    let expected = read_golden(golden_name).expect(
        "Golden file not found. Run with GOLDEN_WRITE=1 first."
    );
    assert_eq!(output.len(), expected.len(), "output length mismatch");
    assert!(output == expected, "byte-for-byte mismatch in sRGB u8 → ACEScg f16");
}

// ---------------------------------------------------------------------------
// Test 2: ACEScg f16 pixels → sRGB u8 (via new pipeline)
// ---------------------------------------------------------------------------

#[test]
fn acescg_f16_to_srgb_u8_bitexact() {
    let pixels = make_acescg_f16_pixels();
    let conv = ColorSpace::ACES_CG.converter_to(ColorSpace::SRGB).unwrap();
    let result: Vec<[u8; 4]> = conv.convert_pixels::<Rgba<f16>, [u8; 4]>(
        &pixels, AlphaPolicy::Straight,
    );
    let output: Vec<u8> = bytemuck::cast_slice::<[u8; 4], u8>(&result).to_vec();

    let golden_name = "acescg_f16_to_srgb_u8.bin";
    if is_write_mode() {
        write_golden(golden_name, &output);
    }
    let expected = read_golden(golden_name).expect(
        "Golden file not found. Run with GOLDEN_WRITE=1 first."
    );
    assert_eq!(output.len(), expected.len(), "output length mismatch");
    assert!(output == expected, "byte-for-byte mismatch in ACEScg f16 → sRGB u8");
}

// ---------------------------------------------------------------------------
// Test 3: RGBA16 native-endian ImageBuffer → ACEScg f16 (via new pipeline)
// ---------------------------------------------------------------------------

#[test]
fn rgba16_to_acescg_f16_bitexact() {
    let buf = make_rgba16_image();
    let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
    let pixels: Vec<Rgba<f16>> = conv.convert_buffer(&buf, AlphaPolicy::PremultiplyOnPack);
    let output = serialize_f16_pixels(&pixels);

    let golden_name = "rgba16_to_acescg.bin";
    if is_write_mode() {
        write_golden(golden_name, &output);
    }
    let expected = read_golden(golden_name).expect(
        "Golden file not found. Run with GOLDEN_WRITE=1 first."
    );
    assert_eq!(output.len(), expected.len(), "output length mismatch");
    assert!(output == expected, "byte-for-byte mismatch in RGBA16 → ACEScg f16");
}

// ---------------------------------------------------------------------------
// Test 4: Gray8 sRGB → ACEScg f16 (via new pipeline)
// ---------------------------------------------------------------------------

#[test]
fn gray8_to_acescg_f16_bitexact() {
    let buf = make_gray8_image();
    let conv = ColorSpace::SRGB.converter_to(ColorSpace::ACES_CG).unwrap();
    let pixels: Vec<Rgba<f16>> = conv.convert_buffer(&buf, AlphaPolicy::PremultiplyOnPack);
    let output = serialize_f16_pixels(&pixels);

    let golden_name = "gray8_to_acescg.bin";
    if is_write_mode() {
        write_golden(golden_name, &output);
    }
    let expected = read_golden(golden_name).expect(
        "Golden file not found. Run with GOLDEN_WRITE=1 first."
    );
    assert_eq!(output.len(), expected.len(), "output length mismatch");
    assert!(output == expected, "byte-for-byte mismatch in Gray8 → ACEScg f16");
}
