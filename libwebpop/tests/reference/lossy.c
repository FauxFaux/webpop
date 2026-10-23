// Test-only logical adapters. All encoder algorithms run in the normally built
// pinned scalar library; do not copy their formulas here.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "src/dsp/dsp.h"
#include "src/webp/encode.h"

size_t WebPEncodeRGBAWithAlphaQuality(const uint8_t*, int, int, int, float, int,
                                     uint8_t**);

// Float quality travels as its exact f32 bit pattern, including NaN/infinity.
static float ReadFloat(uint32_t word) {
  float value;
  typedef char FloatIs32Bits[(sizeof(float) == sizeof(uint32_t)) ? 1 : -1];
  (void)sizeof(FloatIs32Bits);
  memcpy(&value, &word, sizeof(value));
  return value;
}

unsigned WebpopLossyTransform(const uint32_t* in, uint32_t* out) {
  uint8_t src[4 * BPS], ref[4 * BPS];
  int16_t coeffs[16];
  unsigned i;
  // Different padding catches accidental reads outside the logical 4×4 block.
  memset(src, 0xa5, sizeof(src));
  memset(ref, 0x5a, sizeof(ref));
  for (i = 0; i < 16; ++i) {
    src[(i / 4) * BPS + i % 4] = in[i];
    ref[(i / 4) * BPS + i % 4] = in[16 + i];
  }
  VP8EncDspInit();
  VP8FTransform(src, ref, coeffs);
  for (i = 0; i < 16; ++i) out[i] = (uint32_t)(int32_t)coeffs[i];
  return 16;
}

unsigned WebpopLossyEncode(const uint32_t* in, unsigned n, int custom,
                           uint32_t* out) {
  const unsigned offset = custom ? 5 : 4;
  const unsigned length = n - offset;
  uint8_t* const allocation = (uint8_t*)malloc(length + 1);
  uint8_t* rgba;
  uint8_t* encoded = NULL;
  size_t size, i;
  if (allocation == NULL) abort();
  rgba = allocation + 1;  // Exercise unaligned input and padded row starts.
  for (i = 0; i < length; ++i) rgba[i] = in[offset + i];
  if (custom) {
    // A failure must also clear the output slot. Use a live non-owned sentinel.
    encoded = rgba;
    size = WebPEncodeRGBAWithAlphaQuality(rgba, in[0], in[1], in[2],
                                         ReadFloat(in[3]), (int32_t)in[4],
                                         &encoded);
    if (size == 0 && encoded != NULL) abort();
  } else {
    size = WebPEncodeRGBA(rgba, in[0], in[1], in[2], ReadFloat(in[3]), &encoded);
  }
  if (size > (1u << 20) || (size != 0 && encoded == NULL)) abort();
  for (i = 0; i < length; ++i) if (rgba[i] != in[offset + i]) abort();
  for (i = 0; i < size; ++i) out[i] = encoded[i];
  WebPFree(encoded);
  free(allocation);
  return (unsigned)size;
}

unsigned WebpopAlphaWrapperErrors(uint32_t* out) {
  const uint8_t rgba[4] = {1, 2, 3, 255};
  out[0] = WebPEncodeRGBAWithAlphaQuality(rgba, 1, 1, 4, 75.f, 100, NULL) == 0;
  return 1;
}
