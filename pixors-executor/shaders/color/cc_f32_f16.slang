import lib.color_convert_shared;

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_cc_f32_f16(
    uint3                            id : SV_DispatchThreadID,
    uniform RWStructuredBuffer<uint> inputBuffer,
    uniform RWStructuredBuffer<uint> outputBuffer,
    uniform ColorConvertParams       p
) {
    cc_kernel<F32Codec, F16Codec>(id, inputBuffer, outputBuffer, p);
}
