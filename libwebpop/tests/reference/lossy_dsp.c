// Test-only adapters to the normally built pinned scalar encoder DSP.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "src/dsp/dsp.h"
#include "src/enc/vp8i_enc.h"

unsigned WebpopVP8DspTables(uint32_t* out) {
  unsigned i, n = 0;
  for (i = 0; i < 24; ++i) out[n++] = VP8DspScan[i];
  for (i = 0; i < 16; ++i) out[n++] = VP8Scan[i];
  for (i = 0; i < 4; ++i) out[n++] = VP8I16ModeOffsets[i];
  for (i = 0; i < 4; ++i) out[n++] = VP8UVModeOffsets[i];
  out[n++] = QFIX;
  out[n++] = MAX_LEVEL;
  return n;
}

unsigned WebpopVP8Quantize(const uint32_t* in, uint32_t* out) {
  const unsigned count = in[0] == 1 ? 32 : 16;
  VP8Matrix m;
  int16_t input[32], levels[32];
  unsigned i, n = 0;
  for (i = 0; i < 16; ++i) {
    m.q[i] = in[1 + i];
    m.iq[i] = in[17 + i];
    m.bias[i] = in[33 + i];
    m.zthresh[i] = in[49 + i];
    m.sharpen[i] = in[65 + i];
  }
  for (i = 0; i < count; ++i) {
    input[i] = (int16_t)(int32_t)in[81 + i];
    levels[i] = 12345;
  }
  VP8EncDspInit();
  out[n++] = in[0] == 1 ? VP8EncQuantize2Blocks(input, levels, &m) :
      in[0] == 2 ? VP8EncQuantizeBlockWHT(input, levels, &m) :
                   VP8EncQuantizeBlock(input, levels, &m);
  for (i = 0; i < count; ++i) out[n++] = (uint32_t)(int32_t)input[i];
  for (i = 0; i < count; ++i) out[n++] = (uint32_t)(int32_t)levels[i];
  return n;
}

static void Unpack(const uint32_t* words, unsigned w, unsigned h, uint8_t* dst) {
  unsigned x, y;
  for (y = 0; y < h; ++y) for (x = 0; x < w; ++x) dst[y * BPS + x] = words[y * w + x];
}

unsigned WebpopVP8Metric(const uint32_t* in, uint32_t* out) {
  const unsigned widths[] = {4, 8, 16, 16, 4, 16, 16, 4, 16};
  const unsigned heights[] = {4, 8, 8, 16, 4, 16, 4, 4, 8};
  const unsigned op = in[0], w = widths[op], h = heights[op];
  const int texture = op == 4 || op == 5;
  uint8_t a[16 * BPS], b[16 * BPS], dst[16 * BPS];
  uint16_t weights[16];
  unsigned i;
  memset(a, 0x5a, sizeof(a));
  memset(b, 0x3c, sizeof(b));
  memset(dst, 0xa5, sizeof(dst));
  if (texture) for (i = 0; i < 16; ++i) weights[i] = in[1 + i];
  Unpack(in + 1 + (texture ? 16 : 0), w, h, a);
  if (op <= 5) Unpack(in + 1 + (texture ? 16 : 0) + w * h, w, h, b);
  VP8EncDspInit();
  switch (op) {
    case 0: out[0] = VP8SSE4x4(a, b); break;
    case 1: out[0] = VP8SSE8x8(a, b); break;
    case 2: out[0] = VP8SSE16x8(a, b); break;
    case 3: out[0] = VP8SSE16x16(a, b); break;
    case 4: out[0] = VP8TDisto4x4(a, b, weights); break;
    case 5: out[0] = VP8TDisto16x16(a, b, weights); break;
    case 6: VP8Mean16x4(a, out); return 4;
    case 7: VP8Copy4x4(a, dst); break;
    case 8: VP8Copy16x8(a, dst); break;
    default: abort();
  }
  if (op >= 7) {
    for (i = 0; i < h * BPS; ++i) out[i] = dst[i];
    return h * BPS;
  }
  return 1;
}

unsigned WebpopVP8Histogram(const uint32_t* in, int collect, uint32_t* out) {
  VP8Histogram histo;
  histo.max_value = 999;
  histo.last_non_zero = 999;
  if (collect) {
    uint8_t a[16 * BPS], b[16 * BPS];
    memset(a, 0x5a, sizeof(a));
    memset(b, 0xa5, sizeof(b));
    Unpack(in + 2, 16, 16, a);
    Unpack(in + 258, 16, 16, b);
    VP8EncDspInit();
    VP8CollectHistogram(a, b, in[0], in[1], &histo);
  } else {
    int distribution[32];
    unsigned i;
    for (i = 0; i < 32; ++i) distribution[i] = (int32_t)in[i];
    VP8SetHistogramData(distribution, &histo);
  }
  out[0] = histo.max_value;
  out[1] = histo.last_non_zero;
  return 2;
}
