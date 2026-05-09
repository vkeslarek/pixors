// lib/params.slang — Uniform parameters for color conversion
//
// Must match Rust definitions in color.rs.

import lib.transfer;

/// Channel layout IDs (must match Rust `channels()` in color.rs)
enum ChannelLayout : uint { Rgba = 0, Rgb = 1, Gray = 2, GrayA = 3, CmykA = 4 }

/// Alpha policy IDs (must match Rust `alpha_to_u32()` in color.rs)
enum AlphaPolicy : uint { Straight = 0, PremultiplyOnPack = 1, OpaqueDrop = 2 }

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
