// Test-only access to the pinned transfer helpers. No formulas are copied.
#define SharpYuvInitGammaTables WebpopIncludedSharpYuvInitGammaTables
#define SharpYuvGammaToLinear WebpopIncludedSharpYuvGammaToLinear
#define SharpYuvLinearToGamma WebpopIncludedSharpYuvLinearToGamma
#define kSharpYuvGammaToLinearTabS WebpopIncludedSharpYuvGammaToLinearTabS
#define kSharpYuvLinearToGammaTabS WebpopIncludedSharpYuvLinearToGammaTabS
#include "sharpyuv/sharpyuv_gamma.c"
#include <string.h>
#include "sharpyuv/sharpyuv_csp.h"
#include "sharpyuv/sharpyuv_dsp.h"

static float FromWord(uint32_t v) {
  float f;
  memcpy(&f, &v, sizeof(f));
  return f;
}

int WebpopSharpYuvLeaves(const char* name, const uint32_t* in, unsigned n,
                        uint32_t* out, unsigned* count) {
  unsigned i, at = 0;
  if (!strcmp(name, "SharpYuvTables")) {
    if (n) return 0;
    SharpYuvInitGammaTables();
    for (i = 0; i < 1026; ++i) out[at++] = kSharpYuvGammaToLinearTabS[i];
    for (i = 0; i < 514; ++i) out[at++] = kSharpYuvLinearToGammaTabS[i];
  } else if (!strcmp(name, "SharpYuvGamma")) {
    if (n != 5 || in[0] > 1 || in[1] < 8 || in[1] > 16 ||
        in[2] < 1 || in[2] == 2 || in[2] == 3 || in[2] > 18 ||
        (uint64_t)in[3] + in[4] > (in[0] ? (in[2] == 17 && in[1] <= 14 ? 71502 : 65537) : (1u << in[1]))) return 0;
    SharpYuvInitGammaTables();
    for (i = 0; i < in[4]; ++i) {
      out[at++] = in[0] ? SharpYuvLinearToGamma(in[3] + i, in[1], in[2])
                       : SharpYuvGammaToLinear(in[3] + i, in[1], in[2]);
    }
  } else if (!strcmp(name, "SharpYuvMatrix")) {
    SharpYuvConversionMatrix computed;
    const SharpYuvConversionMatrix* matrix;
    if (n == 1 && in[0] < 5) {
      matrix = SharpYuvGetConversionMatrix(in[0]);
    } else if (n == 5 && in[0] == 5 &&
               (in[3] == 8 || in[3] == 10 || in[3] == 12) && in[4] < 2) {
      SharpYuvColorSpace csp;
      csp.kr = FromWord(in[1]); csp.kb = FromWord(in[2]);
      csp.bit_depth = in[3]; csp.range = in[4];
      if (!(csp.kr >= 0 && csp.kb >= 0 && csp.kr + csp.kb < 1)) return 0;
      SharpYuvComputeConversionMatrix(&csp, &computed);
      matrix = &computed;
    } else return 0;
    for (i = 0; i < 4; ++i) out[at++] = matrix->rgb_to_y[i];
    for (i = 0; i < 4; ++i) out[at++] = matrix->rgb_to_u[i];
    for (i = 0; i < 4; ++i) out[at++] = matrix->rgb_to_v[i];
  } else if (!strcmp(name, "SharpYuvMath")) {
    int coeffs[4];
    if (n != 10 || in[3] > 65535 || (int32_t)in[5] < -2 || (int32_t)in[5] > 2) return 0;
    for (i=0;i<3;++i) if ((int32_t)in[i] < -100000 || (int32_t)in[i] > 100000) return 0;
    for (i=0;i<4;++i) if ((int32_t)in[6+i] < -100000000 || (int32_t)in[6+i] > 100000000) return 0;
    for (i = 0; i < 4; ++i) coeffs[i] = (int32_t)in[6 + i];
    out[at++] = SharpYuvClip16((int32_t)in[4], in[3]);
    out[at++] = SharpYuvRGBToGray((int32_t)in[0], (int32_t)in[1], (int32_t)in[2]);
    out[at++] = SharpYuvConvertComponent((int32_t)in[0], (int32_t)in[1], (int32_t)in[2], coeffs, (int32_t)in[5]);
  } else if (!strcmp(name, "SharpYuvFloat")) {
    float x, y, value;
    if (n != 3 || in[0] > 24) return 0;
    x = FromWord(in[1]); y = FromWord(in[2]);
    switch (in[0]) {
      case 0: value = ToLinear709(x); break;
      case 1: value = FromLinear709(x); break;
      case 2: value = ToLinear470M(x); break;
      case 3: value = FromLinear470M(x); break;
      case 4: value = ToLinear470Bg(x); break;
      case 5: value = FromLinear470Bg(x); break;
      case 6: value = ToLinearSmpte240(x); break;
      case 7: value = FromLinearSmpte240(x); break;
      case 8: value = ToLinearLog100(x); break;
      case 9: value = FromLinearLog100(x); break;
      case 10: value = ToLinearLog100Sqrt10(x); break;
      case 11: value = FromLinearLog100Sqrt10(x); break;
      case 12: value = ToLinearIec61966(x); break;
      case 13: value = FromLinearIec61966(x); break;
      case 14: value = ToLinearBt1361(x); break;
      case 15: value = FromLinearBt1361(x); break;
      case 16: value = ToLinearPq(x); break;
      case 17: value = FromLinearPq(x); break;
      case 18: value = ToLinearSmpte428(x); break;
      case 19: value = FromLinearSmpte428(x); break;
      case 20: value = ToLinearHlg(x); break;
      case 21: value = FromLinearHlg(x); break;
      case 22: value = Roundf(x); break;
      case 23: value = Powf(x, y); break;
      default: value = Log10f(x); break;
    }
    // Compare NaN classification without depending on a platform's payload.
    if (isnan(value)) out[at++] = 0x7fc00000;
    else { memcpy(out, &value, sizeof(value)); ++at; }
  } else return 0;
  *count = at;
  return 1;
}
