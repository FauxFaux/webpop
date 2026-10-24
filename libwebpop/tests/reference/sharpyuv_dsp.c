// Test-only logical adapters to the original scalar DSP dispatch functions.
#include <stdint.h>
#include <stdlib.h>
#include "sharpyuv/sharpyuv_dsp.h"
#include "sharpyuv/sharpyuv_gamma.h"

int WebpopSharpYuvDsp(const uint32_t* in, unsigned n, uint32_t* out, unsigned* count) {
  unsigned i, k, len, op, depth, used, at = 0;
  uint16_t a[1536], b[1536], dst[514];
  int16_t sa[1536], sd[514];
  int matrix[12];
  const uint32_t* p;
  if (n < 16 || in[0] > 7 || in[0] == 6 || in[1] < 8 || in[1] > 14 || in[2] > 128 ||
      (int32_t)in[3] < -2 || (int32_t)in[3] > 2) return 0;
  op = in[0]; depth = in[1]; len = in[2]; p = in + 16;
  used = op < 2 ? 3*len : op == 2 ? 4*len+2 : op == 3 ? len+3*((len+1)/2) :
         op == 4 || op == 5 ? 3*len : 12*len;
  if (n != 16 + used || ((op == 5 || op == 7) && len == 0)) return 0;
  for (i = 0; i < 12; ++i) matrix[i] = (int32_t)in[4+i];
  for (i = 0; i < used; ++i) { a[i] = p[i]; sa[i] = (int16_t)p[i]; }
  for (i = 0; i < 514; ++i) { dst[i] = 0xa5a5; sd[i] = (int16_t)0xa55a; }
  SharpYuvInitGammaTables(); SharpYuvInitDsp();
  if (op == 0) {
    uint64_t diff;
    for (i = 0; i < len; ++i) dst[i+1] = a[2*len+i];
    diff = SharpYuvUpdateY(a, a+len, dst+1, len, depth);
    out[at++] = diff; out[at++] = diff >> 32;
    for (i = 0; i < len+2; ++i) out[at++] = dst[i];
  } else if (op == 1) {
    for (i = 0; i < len; ++i) sd[i+1] = sa[2*len+i];
    SharpYuvUpdateRGB(sa, sa+len, sd+1, len);
    for (i = 0; i < len+2; ++i) out[at++] = (int32_t)sd[i];
  } else if (op == 2) {
    SharpYuvFilterRow(sa, sa+len+1, len, a+2*(len+1), dst+1, depth);
    for (i = 0; i < 2*len+2; ++i) out[at++] = dst[i];
  } else if (op == 3) {
    SharpYuvConvertRowY(a, sa+len, len, (len+1)/2, matrix, (int32_t)in[3], depth, dst+1);
    // The dispatch stores bytes at depth 8, logical uint16 samples otherwise.
    out[at++] = dst[0];
    for (i = 0; i < len; ++i) out[at++] = depth == 8 ? ((uint8_t*)(dst+1))[i] : dst[1+i];
    out[at++] = depth == 8 ? ((uint8_t*)(dst+1))[len] : dst[len+1];
  } else if (op == 4) {
    for (i = 0; i < 514; ++i) b[i] = 0xa5a5;
    SharpYuvConvertRowUV(sa, len, matrix+4, matrix+8, (int32_t)in[3], depth, dst+1, b+1);
    for (k = 0; k < 2; ++k) {
      const uint16_t* row = k ? b : dst;
      out[at++] = row[0];
      for (i = 0; i < len; ++i) out[at++] = depth == 8 ? ((const uint8_t*)(row+1))[i] : row[i+1];
      out[at++] = depth == 8 ? ((const uint8_t*)(row+1))[len] : row[len+1];
    }
  } else if (op == 5) {
    for (i = 0; i < used; ++i) if (a[i] >= (1u << depth)) return 0;
    SharpYuvUpdateWSrgb(a, dst+1, len, depth);
    for (i = 0; i < len+2; ++i) out[at++] = dst[i];
  } else {
    for (i = 0; i < used; ++i) if (a[i] >= (1u << depth)) return 0;
    SharpYuvUpdateChromaSrgb(a, a+6*len, sd+1, len, depth);
    for (i = 0; i < 3*len+2; ++i) out[at++] = (int32_t)sd[i];
  }
  *count = at;
  return 1;
}
