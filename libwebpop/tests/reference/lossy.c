// Test-only logical adapters. All encoder algorithms run in the normally built
// pinned scalar library; do not copy their formulas here.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "src/dsp/dsp.h"
#include "src/enc/vp8i_enc.h"
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

unsigned WebpopLossyTransform2(const uint32_t* in, uint32_t* out) {
  uint8_t src[4 * BPS], ref[4 * BPS];
  int16_t coeffs[32];
  unsigned i;
  memset(src, 0xa5, sizeof(src));
  memset(ref, 0x5a, sizeof(ref));
  for (i = 0; i < 32; ++i) {
    src[(i / 8) * BPS + i % 8] = in[i];
    ref[(i / 8) * BPS + i % 8] = in[32 + i];
  }
  VP8EncDspInit();
  VP8FTransform2(src, ref, coeffs);
  for (i = 0; i < 32; ++i) out[i] = (uint32_t)(int32_t)coeffs[i];
  return 32;
}

unsigned WebpopLossyInverseTransform(const uint32_t* in, uint32_t* out) {
  const unsigned width = in[0] ? 8 : 4;
  const unsigned count = in[0] ? 32 : 16;
  uint8_t ref[4 * BPS], dst[4 * BPS];
  int16_t coeffs[32];
  unsigned i;
  memset(ref, 0x5a, sizeof(ref));
  memset(dst, 0xa5, sizeof(dst));
  for (i = 0; i < count; ++i) {
    ref[(i / width) * BPS + i % width] = in[1 + i];
    coeffs[i] = (int16_t)(int32_t)in[1 + count + i];
  }
  VP8EncDspInit();
  VP8ITransform(ref, coeffs, dst, in[0]);
  // Include row padding and storage after the last row to expose stray writes.
  for (i = 0; i < sizeof(dst); ++i) out[i] = dst[i];
  return sizeof(dst);
}

unsigned WebpopLossyWHT(const uint32_t* in, int inverse, uint32_t* out) {
  int16_t input[256], output[256];
  const unsigned input_size = inverse ? 16 : 256;
  const unsigned output_size = inverse ? 256 : 16;
  unsigned i;
  for (i = 0; i < input_size; ++i) input[i] = (int16_t)(int32_t)in[i];
  if (inverse) {
    for (i = 0; i < 256; ++i) output[i] = (int16_t)(int32_t)in[16 + i];
    VP8DspInit();
    VP8TransformWHT(input, output);
  } else {
    VP8EncDspInit();
    VP8FTransformWHT(input, output);
  }
  for (i = 0; i < output_size; ++i) out[i] = (uint32_t)(int32_t)output[i];
  return output_size;
}

unsigned WebpopLossyPredictions(const char* name, const uint32_t* in,
                                 uint32_t* out) {
  uint8_t dst[PRED_SIZE_ENC], left[25], top[16];
  unsigned i;
  VP8EncDspInit();
  if (strcmp(name, "VP8EncPredLuma4") == 0) {
    for (i = 0; i < 13; ++i) top[i] = in[1 + i];
    memset(dst, in[0], sizeof(dst));
    VP8EncPredLuma4(dst, top + 5);
  } else {
    const int chroma = strcmp(name, "VP8EncPredChroma8") == 0;
    const unsigned left_size = chroma ? 25 : 17;
    const uint8_t* const left_ptr = (in[0] & 1) ? left + 1 : NULL;
    const uint8_t* const top_ptr = (in[0] & 2) ? top : NULL;
    for (i = 0; i < left_size; ++i) left[i] = in[2 + i];
    for (i = 0; i < 16; ++i) top[i] = in[2 + left_size + i];
    memset(dst, in[1], sizeof(dst));
    if (chroma) VP8EncPredChroma8(dst, left_ptr, top_ptr);
    else VP8EncPredLuma16(dst, left_ptr, top_ptr);
  }
  for (i = 0; i < sizeof(dst); ++i) out[i] = dst[i];
  return sizeof(dst);
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
