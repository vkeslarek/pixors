import lib.color_convert_shared;

[shader("compute")]
[numthreads(8, 8, 1)]
void cs_cc_u16_u8(
    uint3                            id : SV_DispatchThreadID,
    uniform RWStructuredBuffer<uint> inputBuffer,
    uniform RWStructuredBuffer<uint> outputBuffer,
    uniform ColorConvertParams       p
) {
    cc_kernel<U16Codec, U8Codec>(id, inputBuffer, outputBuffer, p);
}
