// lib/pixel_io.slang — Pixel format decode/encode for compute shaders.
//
// Pixel format IDs (must match Rust fmt_to_u32):
#define FMT_RGBA8    0u
#define FMT_RGB8     1u
#define FMT_GRAY8    2u
#define FMT_GRAYA8   3u
#define FMT_RGBA16   4u
#define FMT_RGB16    5u
#define FMT_RGBA_F16 6u
#define FMT_RGB_F16  7u
#define FMT_RGBA_F32 8u
#define FMT_RGB_F32  9u

import lib.pixel; // rgba8_pack / rgba8_unpack

// ── Low-level byte helpers ──────────────────────────────────────────────────

float read_u16_norm(RWStructuredBuffer<uint> buf, uint byte_offset)
{
    uint wi = byte_offset / 4u;
    uint bo = byte_offset % 4u;
    uint w0 = buf[wi];
    uint val;
    if      (bo == 0u) val = w0 & 0xFFFFu;
    else if (bo == 2u) val = (w0 >> 16u) & 0xFFFFu;
    else {
        uint w1 = buf[wi + 1u];
        val = ((w0 >> (bo * 8u)) | (w1 << (32u - bo * 8u))) & 0xFFFFu;
    }
    return float(val) / 65535.0;
}

float read_f32_at(RWStructuredBuffer<uint> buf, uint byte_offset)
{
    return asfloat(buf[byte_offset / 4u]);
}

void write_byte(RWStructuredBuffer<uint> buf, uint byte_offset, uint val)
{
    uint wi    = byte_offset / 4u;
    uint shift = (byte_offset % 4u) * 8u;
    InterlockedAnd(buf[wi], ~(0xFFu << shift));
    InterlockedOr (buf[wi],  (val & 0xFFu) << shift);
}

void write_u16_at(RWStructuredBuffer<uint> buf, uint byte_offset, uint val)
{
    uint wi = byte_offset / 4u;
    uint bo = byte_offset % 4u;
    if (bo == 0u) {
        InterlockedAnd(buf[wi],  0xFFFF0000u);
        InterlockedOr (buf[wi],  val & 0xFFFFu);
    } else if (bo == 2u) {
        InterlockedAnd(buf[wi],  0x0000FFFFu);
        InterlockedOr (buf[wi], (val & 0xFFFFu) << 16u);
    } else {
        write_byte(buf, byte_offset,     val & 0xFFu);
        write_byte(buf, byte_offset + 1u, (val >> 8u) & 0xFFu);
    }
}

// ── Decode: raw buffer → float4 [0,1] (before transfer decode) ─────────────

float4 decode_pixel(RWStructuredBuffer<uint> buf, uint px_idx, uint fmt)
{
    if (fmt == FMT_RGBA8)
        return rgba8_unpack(buf[px_idx]);

    if (fmt == FMT_RGB8) {
        uint b  = px_idx * 3u;
        uint wi = b / 4u;
        uint w0 = buf[wi], w1 = buf[wi + 1u];
        uint s0 = (b % 4u) * 8u;
        uint s1 = ((b + 1u) % 4u) * 8u; uint wb1 = (b+1u)/4u == wi ? w0 : w1;
        uint s2 = ((b + 2u) % 4u) * 8u; uint wb2 = (b+2u)/4u == wi ? w0 : w1;
        return float4(float((w0  >> s0) & 0xFFu) / 255.0,
                      float((wb1 >> s1) & 0xFFu) / 255.0,
                      float((wb2 >> s2) & 0xFFu) / 255.0, 1.0);
    }

    if (fmt == FMT_GRAY8) {
        uint b = px_idx;
        float v = float((buf[b/4u] >> ((b%4u)*8u)) & 0xFFu) / 255.0;
        return float4(v, v, v, 1.0);
    }

    if (fmt == FMT_GRAYA8) {
        uint b = px_idx * 2u;
        uint w = buf[b / 4u];
        float v = float((w >> ( b    % 4u) * 8u) & 0xFFu) / 255.0;
        float a = float((w >> ((b+1u)%4u) * 8u) & 0xFFu) / 255.0;
        return float4(v, v, v, a);
    }

    if (fmt == FMT_RGBA16) {
        uint b = px_idx * 8u;
        return float4(read_u16_norm(buf, b),      read_u16_norm(buf, b+2u),
                      read_u16_norm(buf, b+4u),   read_u16_norm(buf, b+6u));
    }

    if (fmt == FMT_RGB16) {
        uint b = px_idx * 6u;
        return float4(read_u16_norm(buf, b), read_u16_norm(buf, b+2u),
                      read_u16_norm(buf, b+4u), 1.0);
    }

    if (fmt == FMT_RGBA_F16) {
        uint b  = px_idx * 8u;
        uint w0 = buf[b/4u], w1 = buf[b/4u + 1u];
        return float4(f16tof32(w0 & 0xFFFFu), f16tof32(w0 >> 16u),
                      f16tof32(w1 & 0xFFFFu), f16tof32(w1 >> 16u));
    }

    if (fmt == FMT_RGB_F16) {
        uint b  = px_idx * 6u;
        uint w0 = buf[b/4u], w1 = buf[b/4u + 1u];
        return float4(f16tof32(w0 & 0xFFFFu), f16tof32(w0 >> 16u),
                      f16tof32(w1 & 0xFFFFu), 1.0);
    }

    if (fmt == FMT_RGBA_F32) {
        uint b = px_idx * 16u;
        return float4(read_f32_at(buf, b),    read_f32_at(buf, b+4u),
                      read_f32_at(buf, b+8u), read_f32_at(buf, b+12u));
    }

    // FMT_RGB_F32
    uint b = px_idx * 12u;
    return float4(read_f32_at(buf, b), read_f32_at(buf, b+4u),
                  read_f32_at(buf, b+8u), 1.0);
}

// ── Encode: float4 [0,1] → raw buffer (after transfer encode) ──────────────

void encode_pixel(RWStructuredBuffer<uint> buf, uint px_idx, float4 c, uint fmt)
{
    if (fmt == FMT_RGBA8) {
        buf[px_idx] = rgba8_pack(c);
        return;
    }

    if (fmt == FMT_RGB8) {
        uint b = px_idx * 3u;
        write_byte(buf, b,     uint(saturate(c.r) * 255.0 + 0.5));
        write_byte(buf, b+1u,  uint(saturate(c.g) * 255.0 + 0.5));
        write_byte(buf, b+2u,  uint(saturate(c.b) * 255.0 + 0.5));
        return;
    }

    if (fmt == FMT_GRAY8) {
        float v = saturate(0.2126*c.r + 0.7152*c.g + 0.0722*c.b);
        write_byte(buf, px_idx, uint(v * 255.0 + 0.5));
        return;
    }

    if (fmt == FMT_GRAYA8) {
        float v = saturate(0.2126*c.r + 0.7152*c.g + 0.0722*c.b);
        write_byte(buf, px_idx*2u,    uint(v          * 255.0 + 0.5));
        write_byte(buf, px_idx*2u+1u, uint(saturate(c.a) * 255.0 + 0.5));
        return;
    }

    if (fmt == FMT_RGBA16) {
        uint b = px_idx * 8u;
        write_u16_at(buf, b,    uint(saturate(c.r) * 65535.0 + 0.5));
        write_u16_at(buf, b+2u, uint(saturate(c.g) * 65535.0 + 0.5));
        write_u16_at(buf, b+4u, uint(saturate(c.b) * 65535.0 + 0.5));
        write_u16_at(buf, b+6u, uint(saturate(c.a) * 65535.0 + 0.5));
        return;
    }

    if (fmt == FMT_RGB16) {
        uint b = px_idx * 6u;
        write_u16_at(buf, b,    uint(saturate(c.r) * 65535.0 + 0.5));
        write_u16_at(buf, b+2u, uint(saturate(c.g) * 65535.0 + 0.5));
        write_u16_at(buf, b+4u, uint(saturate(c.b) * 65535.0 + 0.5));
        return;
    }

    if (fmt == FMT_RGBA_F16) {
        uint b  = px_idx * 8u;
        uint wi = b / 4u;
        buf[wi]     = (f32tof16(c.r) & 0xFFFFu) | (f32tof16(c.g) << 16u);
        buf[wi + 1u] = (f32tof16(c.b) & 0xFFFFu) | (f32tof16(c.a) << 16u);
        return;
    }

    if (fmt == FMT_RGB_F16) {
        uint b  = px_idx * 6u;
        uint wi = b / 4u;
        buf[wi] = (f32tof16(c.r) & 0xFFFFu) | (f32tof16(c.g) << 16u);
        write_u16_at(buf, b + 4u, f32tof16(c.b));
        return;
    }

    if (fmt == FMT_RGBA_F32) {
        uint b  = px_idx * 16u;
        uint wi = b / 4u;
        buf[wi]     = asuint(c.r);  buf[wi+1u] = asuint(c.g);
        buf[wi+2u] = asuint(c.b); buf[wi+3u] = asuint(c.a);
        return;
    }

    // FMT_RGB_F32
    uint b  = px_idx * 12u;
    uint wi = b / 4u;
    buf[wi] = asuint(c.r); buf[wi+1u] = asuint(c.g); buf[wi+2u] = asuint(c.b);
}
