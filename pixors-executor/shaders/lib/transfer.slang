// lib/transfer.slang — OETF/EOTF transfer functions

#define TF_LINEAR    0u
#define TF_SRGB      1u
#define TF_REC709    2u
#define TF_GAMMA22   3u
#define TF_GAMMA24   4u
#define TF_GAMMA26   5u
#define TF_PROPHOTO  6u
#define TF_PQ        7u
#define TF_HLG       8u

float decode_tf(float x, uint tf)
{
    if (tf == TF_SRGB)
        return (x <= 0.04045) ? x / 12.92 : pow((x + 0.055) / 1.055, 2.4);
    if (tf == TF_REC709)
        return (x < 0.081) ? x / 4.5 : pow((x + 0.099) / 1.099, 1.0 / 0.45);
    if (tf == TF_GAMMA22)  return pow(max(x, 0.0), 2.2);
    if (tf == TF_GAMMA24)  return pow(max(x, 0.0), 2.4);
    if (tf == TF_GAMMA26)  return pow(max(x, 0.0), 2.6);
    if (tf == TF_PROPHOTO)
        return (x <= 1.0 / 32.0) ? x / 16.0 : pow(x, 1.8);
    if (tf == TF_PQ) {
        const float M1 = 2610.0/16384.0, M2 = (2523.0/4096.0)*128.0;
        const float C1 = 3424.0/4096.0,  C2 = (2413.0/4096.0)*32.0, C3 = (2392.0/4096.0)*32.0;
        float xm2 = pow(max(x, 0.0), 1.0/M2);
        return pow(max(xm2 - C1, 0.0) / (C2 - C3*xm2), 1.0/M1);
    }
    if (tf == TF_HLG) {
        const float A = 0.17883277, B = 0.28466892, C = 0.5599107;
        return (x <= 0.5) ? (x*x)/3.0 : (exp((x - C)/A) + B)/12.0;
    }
    return x; // TF_LINEAR
}

float encode_tf(float y, uint tf)
{
    if (tf == TF_SRGB)
        return (y <= 0.0031308) ? 12.92*y : 1.055*pow(max(y, 0.0), 1.0/2.4) - 0.055;
    if (tf == TF_REC709)
        return (y < 0.018) ? 4.5*y : 1.099*pow(max(y, 0.0), 0.45) - 0.099;
    if (tf == TF_GAMMA22)  return pow(max(y, 0.0), 1.0/2.2);
    if (tf == TF_GAMMA24)  return pow(max(y, 0.0), 1.0/2.4);
    if (tf == TF_GAMMA26)  return pow(max(y, 0.0), 1.0/2.6);
    if (tf == TF_PROPHOTO)
        return (y <= 0.001953125) ? 16.0*y : pow(max(y, 0.0), 1.0/1.8);
    if (tf == TF_PQ) {
        const float M1 = 2610.0/16384.0, M2 = (2523.0/4096.0)*128.0;
        const float C1 = 3424.0/4096.0,  C2 = (2413.0/4096.0)*32.0, C3 = (2392.0/4096.0)*32.0;
        float ym1 = pow(max(y, 0.0), M1);
        return pow((C1 + C2*ym1) / (1.0 + C3*ym1), M2);
    }
    if (tf == TF_HLG) {
        const float A = 0.17883277, B = 0.28466892, C = 0.5599107;
        float yc = max(y, 0.0);
        return (yc <= 1.0/12.0) ? sqrt(3.0*yc) : A*log(12.0*yc - B) + C;
    }
    return y; // TF_LINEAR
}
