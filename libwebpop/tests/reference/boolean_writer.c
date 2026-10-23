// Test-only logical VP8 boolean-writer traces. All coding calls use the pinned
// library; capacities, pointers and allocator details never cross the protocol.
#include <stdint.h>
#include <stdlib.h>
#include "src/utils/bit_writer_utils.h"

static unsigned Inspect(const VP8BitWriter* bw, uint32_t result, uint32_t* out) {
  const uint64_t pos = VP8BitWriterPos(bw);
  uint32_t hash = 2166136261u;
  size_t i;
  for (i = 0; i < VP8BitWriterSize(bw); ++i) {
    hash = (hash ^ VP8BitWriterBuf(bw)[i]) * 16777619u;
  }
  out[0] = result;
  out[1] = (uint32_t)bw->range;
  out[2] = (uint32_t)bw->value;
  out[3] = (uint32_t)bw->run;
  out[4] = (uint32_t)bw->nb_bits;
  out[5] = (uint32_t)pos;
  out[6] = (uint32_t)(pos >> 32);
  out[7] = (uint32_t)VP8BitWriterSize(bw);
  out[8] = hash;
  return 9;
}

int WebpopBooleanWriter(const uint32_t* in, unsigned n, uint32_t* out,
                        unsigned* count) {
  VP8BitWriter bw;
  unsigned at = 2, used = 0, operations = 0;
  int live = 1;
  uint32_t result = 0;
  if (n < 2 || in[0] > 16384 || in[1] > 1) return 0;
  if (!VP8BitWriterInit(&bw, in[0])) abort();
  used += Inspect(&bw, 0, out + used);
  while (at < n) {
    uint32_t op, value, bits;
    if (n - at < 3 || ++operations > 8192) goto invalid;
    op = in[at++]; value = in[at++]; bits = in[at++];
    result = 0;
    if (op <= 5 && !live) goto invalid;
    switch (op) {
      case 0:
        if (bits > 255) goto invalid;
        result = (uint32_t)VP8PutBit(&bw, (int32_t)value, bits);
        break;
      case 1:
        result = (uint32_t)VP8PutBitUniform(&bw, (int32_t)value);
        break;
      case 2:
        if (bits < 1 || bits > 31) goto invalid;
        VP8PutBits(&bw, value, bits);
        break;
      case 3: {
        const int64_t v = (int32_t)value;
        if (bits > 30 || (v < 0 ? -v : v) >= (1LL << bits)) goto invalid;
        VP8PutSignedBits(&bw, (int)v, bits);
        break;
      }
      case 4: VP8BitWriterFinish(&bw); break;
      case 5: {
        uint8_t data[16384];
        unsigned i;
        if (value > sizeof(data) || value > n - at) goto invalid;
        // Keep memcpy's destination valid even for a zero-size request.
        if (value == 0 && bw.buf == NULL) goto invalid;
        for (i = 0; i < value; ++i) {
          if (in[at + i] > 255) goto invalid;
          data[i] = (uint8_t)in[at + i];
        }
        at += value;
        result = VP8BitWriterAppend(&bw, data, value);
        break;
      }
      case 6: VP8BitWriterWipeOut(&bw); live = 0; break;
      case 7:
        if (value > 16384) goto invalid;
        VP8BitWriterWipeOut(&bw);
        if (!VP8BitWriterInit(&bw, value)) abort();
        live = 1;
        break;
      default: goto invalid;
    }
    if (bw.error) abort();
    if (in[1]) used += Inspect(&bw, result, out + used);
  }
  if (!in[1]) used += Inspect(&bw, result, out + used);
  if (used + 1 + VP8BitWriterSize(&bw) > (1u << 20)) goto invalid;
  out[used++] = (uint32_t)VP8BitWriterSize(&bw);
  {
    size_t i;
    for (i = 0; i < VP8BitWriterSize(&bw); ++i) out[used++] = VP8BitWriterBuf(&bw)[i];
  }
  VP8BitWriterWipeOut(&bw);
  *count = used;
  return 1;
invalid:
  VP8BitWriterWipeOut(&bw);
  return 0;
}
