// lib/color_convert_shared.slang
//
// Shared types, interfaces and codec implementations for all cc_<src>_<dst> kernels.

import lib.transfer;
import lib.pixel;

// ── Constants ────────────────────────────────────────────────────────────────

/// Channel layout IDs (must match Rust `channels()` in color.rs)
#define CH_RGBA  0u   // 4-channel: R G B A
#define CH_RGB   1u   // 3-channel: R G B  (alpha = 1.0)
#define CH_GRAY  2u   // 1-channel: V → RGB  (alpha = 1.0)
#define CH_GRAYA 3u   // 2-channel: V A → RGB A

/// Alpha policy IDs (must match Rust `alpha_policy` in color.rs)
#define AP_STRAIGHT    0u
#define AP_PREMULTIPLY 1u

// ── Uniform parameters ───────────────────────────────────────────────────────

struct ColorConvertParams
{
    uint  width;
    uint  height;
    uint  transfer_src;
    uint  transfer_dst;

    // 3×3 primary matrix (row-major), each row padded to float4 for alignment
    float m00; float m01; float m02; float _p0;
    float m10; float m11; float m12; float _p1;
    float m20; float m21; float m22; float _p2;

    uint  alpha_policy;
    uint  src_channels;   // CH_*
    uint  dst_channels;   // CH_*
    uint  _p3;
};

// ── IPixelCodec interface ─────────────────────────────────────────────────────
//
// Implemented by U8Codec, U16Codec, F16Codec, F32Codec.
// Each specialised kernel declares SrcCodec : IPixelCodec and DstCodec : IPixelCodec
// so the compiler generates optimal code with no runtime format branching.

interface IPixelCodec
{
    /// Decode pixel at linear index `idx` from a raw uint32 buffer.
    /// Returns normalised float4 in [0,1] (or scene-linear for floating-point codecs).
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint channels);

    /// Encode a float4 pixel to `buf` at linear index `idx`.
    static void   encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint channels);
};

// ── Byte-level helpers (shared across codecs) ─────────────────────────────────

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
    InterlockedAnd(buf[bo / 4u], ~(0xFFu << shift));
    InterlockedOr (buf[bo / 4u],  (v & 0xFFu) << shift);
}

void _write_u16(RWStructuredBuffer<uint> buf, uint bo, uint v)
{
    uint rem = bo % 4u;
    if (rem == 0u) {
        InterlockedAnd(buf[bo / 4u], 0xFFFF0000u);
        InterlockedOr (buf[bo / 4u], v & 0xFFFFu);
    } else if (rem == 2u) {
        InterlockedAnd(buf[bo / 4u], 0x0000FFFFu);
        InterlockedOr (buf[bo / 4u], (v & 0xFFFFu) << 16u);
    } else {
        _write_byte(buf, bo,      v & 0xFFu);
        _write_byte(buf, bo + 1u, (v >> 8u) & 0xFFu);
    }
}

// ── U8Codec ──────────────────────────────────────────────────────────────────

struct U8Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        if (ch == CH_RGBA)
            return rgba8_unpack(buf[idx]);

        if (ch == CH_RGB) {
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

        if (ch == CH_GRAY) {
            float v = float((buf[idx / 4u] >> ((idx % 4u) * 8u)) & 0xFFu) / 255.0;
            return float4(v, v, v, 1.0);
        }

        // CH_GRAYA
        uint b = idx * 2u;
        uint w = buf[b / 4u];
        float v = float((w >> ( b      % 4u) * 8u) & 0xFFu) / 255.0;
        float a = float((w >> ((b + 1u)% 4u) * 8u) & 0xFFu) / 255.0;
        return float4(v, v, v, a);
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        if (ch == CH_RGBA) { buf[idx] = rgba8_pack(c); return; }

        if (ch == CH_RGB) {
            uint b = idx * 3u;
            _write_byte(buf, b,      uint(saturate(c.r) * 255.0 + 0.5));
            _write_byte(buf, b + 1u, uint(saturate(c.g) * 255.0 + 0.5));
            _write_byte(buf, b + 2u, uint(saturate(c.b) * 255.0 + 0.5));
            return;
        }

        if (ch == CH_GRAY) {
            float v = saturate(0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b);
            _write_byte(buf, idx, uint(v * 255.0 + 0.5));
            return;
        }

        // CH_GRAYA
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
        uint b = idx * (ch == CH_RGBA ? 8u : 6u);
        return float4(
            _u16_norm(buf, b),
            _u16_norm(buf, b + 2u),
            _u16_norm(buf, b + 4u),
            (ch == CH_RGBA) ? _u16_norm(buf, b + 6u) : 1.0
        );
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        uint b = idx * (ch == CH_RGBA ? 8u : 6u);
        _write_u16(buf, b,      uint(saturate(c.r) * 65535.0 + 0.5));
        _write_u16(buf, b + 2u, uint(saturate(c.g) * 65535.0 + 0.5));
        _write_u16(buf, b + 4u, uint(saturate(c.b) * 65535.0 + 0.5));
        if (ch == CH_RGBA)
            _write_u16(buf, b + 6u, uint(saturate(c.a) * 65535.0 + 0.5));
    }
};

// ── F16Codec ─────────────────────────────────────────────────────────────────
// F16 is stored as packed pairs in uint32: { lo = ch[0], hi = ch[1] }

struct F16Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        uint b  = idx * (ch == CH_RGBA ? 8u : 6u);
        uint w0 = buf[b / 4u];
        uint w1 = buf[b / 4u + 1u];
        return float4(
            f16tof32(w0 & 0xFFFFu),
            f16tof32(w0 >> 16u),
            f16tof32(w1 & 0xFFFFu),
            (ch == CH_RGBA) ? f16tof32(w1 >> 16u) : 1.0
        );
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        uint b  = idx * (ch == CH_RGBA ? 8u : 6u);
        uint wi = b / 4u;
        buf[wi]      = (f32tof16(c.r) & 0xFFFFu) | (f32tof16(c.g) << 16u);
        buf[wi + 1u] = (f32tof16(c.b) & 0xFFFFu) | ((ch == CH_RGBA ? f32tof16(c.a) : 0u) << 16u);
    }
};

// ── F32Codec ─────────────────────────────────────────────────────────────────

struct F32Codec : IPixelCodec
{
    static float4 decode(RWStructuredBuffer<uint> buf, uint idx, uint ch)
    {
        uint stride = (ch == CH_RGBA) ? 4u : 3u;
        uint wi     = idx * stride;
        return float4(
            asfloat(buf[wi]),
            asfloat(buf[wi + 1u]),
            asfloat(buf[wi + 2u]),
            (ch == CH_RGBA) ? asfloat(buf[wi + 3u]) : 1.0
        );
    }

    static void encode(RWStructuredBuffer<uint> buf, uint idx, float4 c, uint ch)
    {
        uint stride = (ch == CH_RGBA) ? 4u : 3u;
        uint wi     = idx * stride;
        buf[wi]      = asuint(c.r);
        buf[wi + 1u] = asuint(c.g);
        buf[wi + 2u] = asuint(c.b);
        if (ch == CH_RGBA) buf[wi + 3u] = asuint(c.a);
    }
};

// ── Color conversion (transfer + matrix + alpha + encode) ─────────────────────

float4 color_convert(float4 raw, ColorConvertParams p)
{
    // OETF → linear light
    float r = decode_tf(raw.r, p.transfer_src);
    float g = decode_tf(raw.g, p.transfer_src);
    float b = decode_tf(raw.b, p.transfer_src);
    float a = raw.a;

    // Primary matrix
    float ro = p.m00 * r + p.m01 * g + p.m02 * b;
    float go = p.m10 * r + p.m11 * g + p.m12 * b;
    float bo = p.m20 * r + p.m21 * g + p.m22 * b;

    if (p.alpha_policy == AP_PREMULTIPLY) { ro *= a; go *= a; bo *= a; }

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
    D.encode(outputBuffer, idx, color_convert(S.decode(inputBuffer, idx, p.src_channels), p), p.dst_channels);
}
