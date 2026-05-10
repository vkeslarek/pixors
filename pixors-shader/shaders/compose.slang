// compose_body.slang — pairwise Porter-Duff alpha-over kernel body.
//
// Generic over IPixelCodec.  No entry-point attributes.

import lib.codecs;
import lib.params;

struct ComposeParams {
    uint width;
    uint height;
    float opacity_a;
    float opacity_b;
};

[[vk::binding(0, 0)]] RWStructuredBuffer<uint>   g_layer_a;
[[vk::binding(1, 0)]] RWStructuredBuffer<uint>   g_layer_b;
[[vk::binding(2, 0)]] RWStructuredBuffer<uint>   g_dst;
[[vk::binding(0, 1)]] ConstantBuffer<ComposeParams> g_params;

void cs_compose<C : IPixelCodec>(uint3 gid)
{
    uint idx = gid.y * g_params.width + gid.x;

    float4 bottom = C.decode(g_layer_a, idx, ChannelLayout::Rgba);
    float4 top    = C.decode(g_layer_b, idx, ChannelLayout::Rgba);

    bottom.a *= g_params.opacity_a;
    top.a    *= g_params.opacity_b;

    float3 top_premul = top.rgb * top.a;
    float3 bot_premul = bottom.rgb * bottom.a;
    float  inv_top_a  = 1.0 - top.a;
    float  result_a   = top.a + bottom.a * inv_top_a;

    float3 result_rgb;
    if (result_a > 0.0) {
        result_rgb = (top_premul + bot_premul * inv_top_a) / result_a;
    } else {
        result_rgb = float3(0.0, 0.0, 0.0);
    }

    float4 result = float4(result_rgb, result_a);
    C.encode(g_dst, idx, result, ChannelLayout::Rgba);
}
