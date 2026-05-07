// lib/color.slang
//
// Shared types, interfaces, transfer functions, and codec implementations for color conversions.

import lib.pixel;

// ── OETF/EOTF transfer functions ─────────────────────────────────────────────

enum TransferFn : uint { Linear = 0, Srgb = 1, Rec709 = 2, Gamma22 = 3, Gamma24 = 4, Gamma26 = 5, ProPhoto = 6, Pq = 7, Hlg = 8 }

float decode_tf(float x, TransferFn tf)
{
    if (tf == TransferFn::Srgb)
        return (x <= 0.04045) ? x / 12.92 : pow((x + 0.055) / 1.055, 2.4);
    if (tf == TransferFn::Rec709)
        return (x < 0.081) ? x / 4.5 : pow((x + 0.099) / 1.099, 1.0 / 0.45);
    if (tf == TransferFn::Gamma22)  return pow(max(x, 0.0), 2.2);
    if (tf == TransferFn::Gamma24)  return pow(max(x, 0.0), 2.4);
    if (tf == TransferFn::Gamma26)  return pow(max(x, 0.0), 2.6);
    if (tf == TransferFn::ProPhoto)
        return (x <= 1.0 / 32.0) ? x / 16.0 : pow(x, 1.8);
    if (tf == TransferFn::Pq) {
        const float M1 = 2610.0/16384.0, M2 = (2523.0/4096.0)*128.0;
        const float C1 = 3424.0/4096.0,  C2 = (2413.0/4096.0)*32.0, C3 = (2392.0/4096.0)*32.0;
        float xm2 = pow(max(x, 0.0), 1.0/M2);
        return pow(max(xm2 - C1, 0.0) / (C2 - C3*xm2), 1.0/M1);
    }
    if (tf == TransferFn::Hlg) {
        const float A = 0.17883277, B = 0.28466892, C = 0.5599107;
        return (x <= 0.5) ? (x*x)/3.0 : (exp((x - C)/A) + B)/12.0;
    }
    return x; // TransferFn::Linear
}

float encode_tf(float y, TransferFn tf)
{
    if (tf == TransferFn::Srgb)
        return (y <= 0.0031308) ? 12.92*y : 1.055*pow(max(y, 0.0), 1.0/2.4) - 0.055;
    if (tf == TransferFn::Rec709)
        return (y < 0.018) ? 4.5*y : 1.099*pow(max(y, 0.0), 0.45) - 0.099;
    if (tf == TransferFn::Gamma22)  return pow(max(y, 0.0), 1.0/2.2);
    if (tf == TransferFn::Gamma24)  return pow(max(y, 0.0), 1.0/2.4);
    if (tf == TransferFn::Gamma26)  return pow(max(y, 0.0), 1.0/2.6);
    if (tf == TransferFn::ProPhoto)
        return (y <= 0.001953125) ? 16.0*y : pow(max(y, 0.0), 1.0/1.8);
    if (tf == TransferFn::Pq) {
        const float M1 = 2610.0/16384.0, M2 = (2523.0/4096.0)*128.0;
        const float C1 = 3424.0/4096.0,  C2 = (2413.0/4096.0)*32.0, C3 = (2392.0/4096.0)*32.0;
        float ym1 = pow(max(y, 0.0), M1);
        return pow((C1 + C2*ym1) / (1.0 + C3*ym1), M2);
    }
    if (tf == TransferFn::Hlg) {
        const float A = 0.17883277, B = 0.28466892, C = 0.5599107;
        float yc = max(y, 0.0);
        return (yc <= 1.0/12.0) ? sqrt(3.0*yc) : A*log(12.0*yc - B) + C;
    }
    return y; // TransferFn::Linear
}


// ── Constants ────────────────────────────────────────────────────────────────

/// Channel layout IDs (must match Rust `channels()` in color.rs)
enum ChannelLayout : uint { Rgba = 0, Rgb = 1, Gray = 2, GrayA = 3, CmykA = 4 }

/// Alpha policy IDs (must match Rust `alpha_to_u32()` in color.rs)
enum AlphaPolicy : uint { Straight = 0, PremultiplyOnPack = 1, OpaqueDrop = 2 }

// ── Uniform parameters ───────────────────────────────────────────────────────

/// Color model transform (must match Rust `ColorModelTransform`).
enum ColorModel : uint { None = 0, CmykToRgb = 1, CmykAToRgb = 2, YCbCrToRgb = 3, LabToRgb = 4 }


struct ColorConvertParams
{
    uint  width;
    uint  height;
    TransferFn transfer_src;
    TransferFn transfer_dst;

    // 3×3 primary matrix (row-major), each row padded to float4 for alignment
    float m00; float m01; float m02; float _p0;
    float m10; float m11; float m12; float _p1;
    float m20; float m21; float m22; float _p2;

    AlphaPolicy  alpha_policy_src;
    AlphaPolicy  alpha_policy_dst;
    ColorModel   model_transform;
    ChannelLayout src_channels;
    ChannelLayout dst_channels;
};

// ── IPixelCodec interface ─────────────────────────────────────────────────────
//
// Implemented by U8Codec, U16Codec, F16Codec, F32Codec.
// Each specialised kernel declares SrcCodec : IPixelCodec and DstCodec : IPixelCodec
// so the compiler generates optimal code with no runtime format branching.

interface IPixelCodec
{
    /// Decode pixel at linear index `idx`. Returns float4 in [0,1].
    /// For CmykA: returns (C, M, Y, K) — alpha lives in decode_extra.
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout channels);

    /// Returns the 5th channel for CmykA layouts (alpha), 1.0 for all others.
    static float  decode_extra(RWStructuredBuffer<uint> buf, uint idx, ChannelLayout channels);

    /// Encode a float4 pixel to `buf` at linear index `idx`.
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

// ── Color conversion (transfer + matrix + alpha + encode) ─────────────────────

/// `extra_alpha`: 5th channel for CmykA (from decode_extra), 1.0 otherwise.
float4 color_convert(float4 raw, float extra_alpha, ColorConvertParams p)
{
    // ── Model decode (CMYK → RGB, YCbCr → RGB, Lab → linear sRGB) ─────────
    float r = raw.r;
    float g = raw.g;
    float b = raw.b;
    float a = raw.a;

    if (p.model_transform == ColorModel::CmykToRgb || p.model_transform == ColorModel::CmykAToRgb) {
        float scale = 1.0 - raw.a;  // raw.a = K
        r = (1.0 - raw.r) * scale;
        g = (1.0 - raw.g) * scale;
        b = (1.0 - raw.b) * scale;
        a = (p.model_transform == ColorModel::CmykAToRgb) ? extra_alpha : 1.0;
    } else if (p.model_transform == ColorModel::YCbCrToRgb) {
        float cb = raw.g - 0.5;  // raw.g = Cb
        float cr = raw.b - 0.5;  // raw.b = Cr
        r = raw.r + 1.402 * cr;
        g = raw.r - 0.344136 * cb - 0.714136 * cr;
        b = raw.r + 1.772 * cb;
        // a stays as raw.a (which is 1.0 from decoder for 3ch YCbCr)
    } else if (p.model_transform == ColorModel::LabToRgb) {
        // raw: L_norm=[0,1], a_norm=[-1,1], b_norm=[-1,1]
        // Encoding: L*=100*L_norm, a*=128*a_norm, b*=128*b_norm
        float L_star = raw.r * 100.0;
        float a_star = raw.g * 128.0;
        float b_star = raw.b * 128.0;
        // Lab → XYZ (D50 white: Xn=0.96422, Yn=1.0, Zn=0.82521)
        float fy = (L_star + 16.0) / 116.0;
        float fx = a_star / 500.0 + fy;
        float fz = fy - b_star / 200.0;
        const float DELTA  = 6.0 / 29.0;
        const float DELTA2 = 3.0 * DELTA * DELTA;
        float x = 0.96422 * (fx > DELTA ? fx*fx*fx : DELTA2*(fx - 4.0/29.0));
        float y =            (fy > DELTA ? fy*fy*fy : DELTA2*(fy - 4.0/29.0));
        float z = 0.82521 * (fz > DELTA ? fz*fz*fz : DELTA2*(fz - 4.0/29.0));
        // D50 XYZ → linear sRGB coordinates (Bradford D50→D65 + D65→sRGB combined).
        // No clamping: out-of-sRGB-gamut Lab values stay negative/above-1 so the
        // downstream primary matrix (sRGB→dst) can map them correctly. This is
        // mathematically equivalent to Lab→XYZ→dst with no intermediate loss.
        r =  3.1338561*x - 1.6168667*y - 0.4906146*z;
        g = -0.9787684*x + 1.9161415*y + 0.0334540*z;
        b =  0.0719453*x - 0.2289914*y + 1.4052427*z;
        a = 1.0;
    }

    // OETF → linear light
    r = decode_tf(r, p.transfer_src);
    g = decode_tf(g, p.transfer_src);
    b = decode_tf(b, p.transfer_src);

    // Un-premultiply source if needed
    if (p.alpha_policy_src == AlphaPolicy::PremultiplyOnPack) {
        float safe_a = max(a, 1e-8);
        r /= safe_a;
        g /= safe_a;
        b /= safe_a;
    }

    // Primary matrix
    float ro = p.m00 * r + p.m01 * g + p.m02 * b;
    float go = p.m10 * r + p.m11 * g + p.m12 * b;
    float bo = p.m20 * r + p.m21 * g + p.m22 * b;

    // Apply destination alpha policy (premultiply for opaque/premul)
    if (p.alpha_policy_dst == AlphaPolicy::PremultiplyOnPack || p.alpha_policy_dst == AlphaPolicy::OpaqueDrop) {
        ro *= a; go *= a; bo *= a;
    }

    // Linear light → OETF
    return float4(encode_tf(ro, p.transfer_dst),
                  encode_tf(go, p.transfer_dst),
                  encode_tf(bo, p.transfer_dst), a);
}

// ── Generic kernel template ───────────────────────────────────────────────────
//
// All cc_<src>_<dst>.slang files call this with the appropriate codec pair.

void cc_kernel<S : IPixelCodec, D : IPixelCodec>(
    uint3                            id,
    RWStructuredBuffer<uint>         inputBuffer,
    RWStructuredBuffer<uint>         outputBuffer,
    ColorConvertParams               p
) {
    if (id.x >= p.width || id.y >= p.height) return;
    uint idx = id.y * p.width + id.x;
    float4 raw   = S.decode(inputBuffer, idx, p.src_channels);
    float  extra = S.decode_extra(inputBuffer, idx, p.src_channels);
    D.encode(outputBuffer, idx, color_convert(raw, extra, p), p.dst_channels);
}
