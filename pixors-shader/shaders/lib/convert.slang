// lib/convert.slang — Color conversion kernel
//
// color_convert + cc_kernel template.
// Imported by color.slang (entry point).

import lib.transfer;
import lib.params;
import lib.codecs;

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
