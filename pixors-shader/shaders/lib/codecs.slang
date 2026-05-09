// lib/codecs.slang — Pixel codec implementations
//
// IPixelCodec interface + U8, U16, F16, F32 codecs.
// Used by color.slang, blur.slang, mip_downsample.slang.

import lib.pixel;
import lib.params;

// ── IPixelCodec interface ─────────────────────────────────────────────────────

interface IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout channels);
    static float  decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout channels);
    static void   encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, ChannelLayout channels);
};

// ── Byte-level helpers (shared across codecs) ─────────────────────────────────

uint byte_buf(uint bo, RWStructuredBuffer<uint> buf)
{
    uint wi = bo / 4u;
    uint shift = (bo % 4u) * 8u;
    return (buf[wi] >> shift) & 0xFFu;
}

float _u16_norm(RWStructuredBuffer<uint> buf, uint bo)
{
    uint wi  = bo / 4u;
    uint rem = bo % 4u;
    uint w   = buf[wi];
    uint val;
    if      (rem == 0u) val = w & 0xFFFFu;
    else if (rem == 2u) val = (w >> 16u) & 0xFFFFu;
    else                val = ((w >> (rem * 8u)) | (buf[wi + 1u] << (32u - rem * 8u))) & 0xFFFFu;
    return float(val) / 65535.0;
}

void _write_byte(RWStructuredBuffer<uint> buf, uint bo, uint v)
{
    uint shift = (bo % 4u) * 8u;
    uint mask = ~(0xFFu << shift);
    uint old = buf[bo / 4u];
    buf[bo / 4u] = (old & mask) | ((v & 0xFFu) << shift);
}

void _write_u16(RWStructuredBuffer<uint> buf, uint bo, uint v)
{
    uint rem = bo % 4u;
    if (rem == 0u) {
        uint old = buf[bo / 4u];
        buf[bo / 4u] = (old & 0xFFFF0000u) | (v & 0xFFFFu);
    } else if (rem == 2u) {
        uint old = buf[bo / 4u];
        buf[bo / 4u] = (old & 0x0000FFFFu) | ((v & 0xFFFFu) << 16u);
    } else {
        _write_byte(buf, bo,      v & 0xFFu);
        _write_byte(buf, bo + 1u, (v >> 8u) & 0xFFu);
    }
}

// ── F16 Software Decoder/Encoder ─────────────────────────────────────────────
// Manual bit-twiddling avoids SPIR-V OpTypeInt 16 emitting,
// which satisfies Naga/WGPU strict validation natively without shaderInt16.

float _half_to_float(uint h) {
    uint s = (h & 0x8000u) << 16u;
    uint em = h & 0x7FFFu;
    if (em == 0u) return asfloat(s);

    uint e = em >> 10u;
    uint m = em & 0x03FFu;

    if (e == 0u) {
        m <<= 1u;
        e = 0u;
        while ((m & 0x0400u) == 0u) { m <<= 1u; e++; }
        m &= 0x03FFu;
        return asfloat(s | ((127u - 15u - e) << 23u) | (m << 13u));
    }
    if (e == 31u) return asfloat(s | 0x7F800000u | (m << 13u));
    return asfloat(s | ((e - 15u + 127u) << 23u) | (m << 13u));
}

uint _float_to_half(float f) {
    uint u = asuint(f);
    uint s = (u >> 16u) & 0x8000u;
    uint e = (u >> 23u) & 0xFFu;
    uint m = u & 0x007FFFFFu;

    if (e == 0u) return s;
    if (e == 255u) {
        if (m == 0u) return s | 0x7C00u;
        return s | 0x7C00u | (m >> 13u);
    }

    int i = int(e) - 127 + 15;
    if (i <= 0) {
        uint sub_m = (0x00800000u | m) >> uint(1 - i);
        return s | (sub_m >> 13u);
    }
    if (i >= 31) return s | 0x7C00u;
    return s | (uint(i) << 10u) | (m >> 13u);
}

/// Read one f16 from byte offset `bo` (handles any alignment).
float _f16_at(RWStructuredBuffer<uint> buf, uint bo)
{
    uint wi  = bo / 4u;
    uint rem = bo % 4u;
    uint w   = buf[wi];
    uint val;
    if      (rem == 0u) val = w & 0xFFFFu;
    else if (rem == 2u) val = (w >> 16u) & 0xFFFFu;
    else                val = ((w >> (rem * 8u)) | (buf[wi + 1u] << (32u - rem * 8u))) & 0xFFFFu;
    return _half_to_float(val);
}

/// Write one f16 to byte offset `bo` (handles any alignment).
void _write_f16(RWStructuredBuffer<uint> buf, uint bo, float v)
{
    uint h   = _float_to_half(v);
    uint rem = bo % 4u;
    if (rem == 0u) {
        uint old = buf[bo / 4u];
        buf[bo / 4u] = (old & 0xFFFF0000u) | (h & 0xFFFFu);
    } else if (rem == 2u) {
        uint old = buf[bo / 4u];
        buf[bo / 4u] = (old & 0x0000FFFFu) | (h << 16u);
    } else {
        _write_byte(buf, bo,      h & 0xFFu);
        _write_byte(buf, bo + 1u, (h >> 8u) & 0xFFu);
    }
}

// ── U8Codec ──────────────────────────────────────────────────────────────────

struct U8Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        if (ch == ChannelLayout::Rgba)
            return rgba8_unpack(buf[idx]);

        if (ch == ChannelLayout::Rgb) {
            uint b  = idx * 3u;
            uint wi = b / 4u;
            uint w0 = buf[wi], w1 = buf[wi + 1u];
            uint s0 = (b % 4u) * 8u;
            uint s1 = ((b + 1u) % 4u) * 8u;  uint wb1 = (b + 1u) / 4u == wi ? w0 : w1;
            uint s2 = ((b + 2u) % 4u) * 8u;  uint wb2 = (b + 2u) / 4u == wi ? w0 : w1;
            return float4(
                float((w0  >> s0) & 0xFFu) / 255.0,
                float((wb1 >> s1) & 0xFFu) / 255.0,
                float((wb2 >> s2) & 0xFFu) / 255.0,
                1.0
            );
        }

        if (ch == ChannelLayout::Gray) {
            float v = float((buf[idx / 4u] >> ((idx % 4u) * 8u)) & 0xFFu) / 255.0;
            return float4(v, v, v, 1.0);
        }

        if (ch == ChannelLayout::CmykA) {
            uint b = idx * 5u;
            return float4(
                float(byte_buf(b    , buf)) / 255.0,
                float(byte_buf(b + 1u, buf)) / 255.0,
                float(byte_buf(b + 2u, buf)) / 255.0,
                float(byte_buf(b + 3u, buf)) / 255.0
            );
        }

        // ChannelLayout::GrayA
        uint b = idx * 2u;
        uint w = buf[b / 4u];
        float v = float((w >> ( b      % 4u) * 8u) & 0xFFu) / 255.0;
        float a = float((w >> ((b + 1u)% 4u) * 8u) & 0xFFu) / 255.0;
        return float4(v, v, v, a);
    }


    static float decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout ch)
    {
        if (ch == ChannelLayout::CmykA)
            return float(byte_buf(idx * 5u + 4u, buf)) / 255.0;
        return 1.0;
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        if (ch == ChannelLayout::Rgba) { buf[idx] = rgba8_pack(c); return; }

        if (ch == ChannelLayout::Rgb) {
            uint b = idx * 3u;
            _write_byte(buf, b,      uint(saturate(c.r) * 255.0 + 0.5));
            _write_byte(buf, b + 1u, uint(saturate(c.g) * 255.0 + 0.5));
            _write_byte(buf, b + 2u, uint(saturate(c.b) * 255.0 + 0.5));
            return;
        }

        if (ch == ChannelLayout::Gray) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_byte(buf, idx, uint(v * 255.0 + 0.5));
            return;
        }

        // ChannelLayout::GrayA
        float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
        _write_byte(buf, idx * 2u,      uint(v             * 255.0 + 0.5));
        _write_byte(buf, idx * 2u + 1u, uint(saturate(c.a) * 255.0 + 0.5));
    }
};

// ── U16Codec ─────────────────────────────────────────────────────────────────

struct U16Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = _u16_norm(buf, idx * 2u);
            return float4(v, v, v, 1.0);
        }
        if (ch == ChannelLayout::GrayA) {
            float v = _u16_norm(buf, idx * 4u);
            float a = _u16_norm(buf, idx * 4u + 2u);
            return float4(v, v, v, a);
        }
        if (ch == ChannelLayout::CmykA) {
            uint b = idx * 10u;
            return float4(
                _u16_norm(buf, b),
                _u16_norm(buf, b + 2u),
                _u16_norm(buf, b + 4u),
                _u16_norm(buf, b + 6u)
            );
        }
        uint b = idx * (ch == ChannelLayout::Rgba ? 8u : 6u);
        return float4(
            _u16_norm(buf, b),
            _u16_norm(buf, b + 2u),
            _u16_norm(buf, b + 4u),
            (ch == ChannelLayout::Rgba) ? _u16_norm(buf, b + 6u) : 1.0
        );
    }

    static float decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout ch)
    {
        if (ch == ChannelLayout::CmykA)
            return _u16_norm(buf, idx * 10u + 8u);
        return 1.0;
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_u16(buf, idx * 2u, uint(v * 65535.0 + 0.5));
            return;
        }
        if (ch == ChannelLayout::GrayA) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_u16(buf, idx * 4u,      uint(v             * 65535.0 + 0.5));
            _write_u16(buf, idx * 4u + 2u, uint(saturate(c.a) * 65535.0 + 0.5));
            return;
        }
        uint b = idx * (ch == ChannelLayout::Rgba ? 8u : 6u);
        _write_u16(buf, b,      uint(saturate(c.r) * 65535.0 + 0.5));
        _write_u16(buf, b + 2u, uint(saturate(c.g) * 65535.0 + 0.5));
        _write_u16(buf, b + 4u, uint(saturate(c.b) * 65535.0 + 0.5));
        if (ch == ChannelLayout::Rgba)
            _write_u16(buf, b + 6u, uint(saturate(c.a) * 65535.0 + 0.5));
    }
};

// ── F16Codec ─────────────────────────────────────────────────────────────────
// Uses _f16_at / _write_f16 for correct byte-level access at any alignment.

struct F16Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = _f16_at(buf, idx * 2u);
            return float4(v, v, v, 1.0);
        }
        if (ch == ChannelLayout::GrayA) {
            float v = _f16_at(buf, idx * 4u);
            float a = _f16_at(buf, idx * 4u + 2u);
            return float4(v, v, v, a);
        }
        if (ch == ChannelLayout::CmykA) {
            uint b = idx * 10u;
            return float4(
                _f16_at(buf, b),
                _f16_at(buf, b + 2u),
                _f16_at(buf, b + 4u),
                _f16_at(buf, b + 6u)
            );
        }
        uint stride = (ch == ChannelLayout::Rgba) ? 8u : 6u;
        uint b = idx * stride;
        return float4(
            _f16_at(buf, b),
            _f16_at(buf, b + 2u),
            _f16_at(buf, b + 4u),
            (ch == ChannelLayout::Rgba) ? _f16_at(buf, b + 6u) : 1.0
        );
    }

    static float decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout ch)
    {
        if (ch == ChannelLayout::CmykA)
            return _f16_at(buf, idx * 10u + 8u);
        return 1.0;
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_f16(buf, idx * 2u, v);
            return;
        }
        if (ch == ChannelLayout::GrayA) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_f16(buf, idx * 4u,      v);
            _write_f16(buf, idx * 4u + 2u, saturate(c.a));
            return;
        }
        uint stride = (ch == ChannelLayout::Rgba) ? 8u : 6u;
        uint b = idx * stride;
        _write_f16(buf, b,      c.r);
        _write_f16(buf, b + 2u, c.g);
        _write_f16(buf, b + 4u, c.b);
        if (ch == ChannelLayout::Rgba)
            _write_f16(buf, b + 6u, c.a);
    }
};

// ── F32Codec ─────────────────────────────────────────────────────────────────

struct F32Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = asfloat(buf[idx]);
            return float4(v, v, v, 1.0);
        }
        if (ch == ChannelLayout::GrayA) {
            float v = asfloat(buf[idx * 2u]);
            float a = asfloat(buf[idx * 2u + 1u]);
            return float4(v, v, v, a);
        }
        if (ch == ChannelLayout::CmykA) {
            uint wi = idx * 5u;
            return float4(
                asfloat(buf[wi]),
                asfloat(buf[wi + 1u]),
                asfloat(buf[wi + 2u]),
                asfloat(buf[wi + 3u])
            );
        }
        uint stride = (ch == ChannelLayout::Rgba) ? 4u : 3u;
        uint wi     = idx * stride;
        return float4(
            asfloat(buf[wi]),
            asfloat(buf[wi + 1u]),
            asfloat(buf[wi + 2u]),
            (ch == ChannelLayout::Rgba) ? asfloat(buf[wi + 3u]) : 1.0
        );
    }

    static float decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout ch)
    {
        if (ch == ChannelLayout::CmykA)
            return asfloat(buf[idx * 5u + 4u]);
        return 1.0;
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        if (ch == ChannelLayout::Gray) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            buf[idx] = asuint(v);
            return;
        }
        if (ch == ChannelLayout::GrayA) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            buf[idx * 2u]      = asuint(v);
            buf[idx * 2u + 1u] = asuint(saturate(c.a));
            return;
        }
        uint stride = (ch == ChannelLayout::Rgba) ? 4u : 3u;
        uint wi     = idx * stride;
        buf[wi]      = asuint(c.r);
        buf[wi + 1u] = asuint(c.g);
        buf[wi + 2u] = asuint(c.b);
        if (ch == ChannelLayout::Rgba) buf[wi + 3u] = asuint(c.a);
    }
};
