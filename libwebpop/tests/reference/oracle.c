// Test-only adapters to the pinned libwebp C implementation (also usable from
// C++). Algorithms live in upstream; no copies of their formulas belong here.
// Protocol: function-name, decimal word count, hexadecimal u32 input words.
// Response: decimal word count followed by hexadecimal u32 logical outputs.
// One request/response per line. No pointers or struct representations cross it.
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "src/dsp/lossless.h"
#include "src/dsp/lossless_common.h"
#include "src/utils/color_cache_utils.h"
#include "src/utils/utils.h"
#include "src/webp/encode.h"

#define LIMIT (1u << 20)
#define REQUIRE(x) do { if (!(x)) { fprintf(stderr, "invalid request: %s\n", name); return 1; } } while (0)
#define IS(x) (strcmp(name, #x) == 0)

int main(void) {
  char name[80];
  uint32_t *in = (uint32_t*)malloc(LIMIT * sizeof(*in));
  uint32_t *out = (uint32_t*)malloc(LIMIT * sizeof(*out));
  if (in == NULL || out == NULL) return 1;
  while (scanf("%79s", name) == 1) {
    unsigned n, count = 0, i;
    REQUIRE(scanf("%u", &n) == 1 && n <= LIMIT);
    for (i = 0; i < n; ++i) REQUIRE(scanf("%" SCNx32, &in[i]) == 1);
    if (IS(PutLE32)) {
      uint8_t bytes[4];
      REQUIRE(n == 1);
      PutLE32(bytes, in[0]);
      for (i = 0; i < 4; ++i) out[count++] = bytes[i];
    } else if (IS(BitsLog2Floor) || IS(BitsCtz)) {
      REQUIRE(n == 1 && in[0] != 0);
      out[count++] = IS(BitsCtz) ? BitsCtz(in[0]) : BitsLog2Floor(in[0]);
    } else if (IS(VP8LSubSampleSize)) {
      REQUIRE(n == 2 && in[1] < 31 && in[0] <= UINT32_MAX - ((1u << in[1]) - 1));
      out[count++] = VP8LSubSampleSize(in[0], in[1]);
    } else if (IS(VP8LNearLosslessBits)) {
      REQUIRE(n == 1 && in[0] <= 100);
      out[count++] = VP8LNearLosslessBits(in[0]);
    } else if (IS(VP8LAddPixels) || IS(VP8LSubPixels)) {
      REQUIRE(n == 2);
      out[count++] = IS(VP8LAddPixels) ? VP8LAddPixels(in[0], in[1]) : VP8LSubPixels(in[0], in[1]);
    } else if (IS(VP8LPrefixEncode) || IS(VP8LPrefixEncodeBits) ||
               IS(VP8LPrefixEncodeNoLUT) || IS(VP8LPrefixEncodeBitsNoLUT)) {
      int code, bits, value;
      REQUIRE(n == 1 && in[0] >= 1 && in[0] <= INT32_MAX);
      if (IS(VP8LPrefixEncode)) VP8LPrefixEncode((int)in[0], &code, &bits, &value);
      else if (IS(VP8LPrefixEncodeBits)) VP8LPrefixEncodeBits((int)in[0], &code, &bits);
      else {
        REQUIRE(in[0] >= 3);
        if (IS(VP8LPrefixEncodeNoLUT)) VP8LPrefixEncodeNoLUT((int)in[0], &code, &bits, &value);
        else VP8LPrefixEncodeBitsNoLUT((int)in[0], &code, &bits);
      }
      out[count++] = (uint32_t)code;
      out[count++] = (uint32_t)bits;
      if (IS(VP8LPrefixEncode) || IS(VP8LPrefixEncodeNoLUT)) out[count++] = (uint32_t)value;
    } else if (IS(VP8LHashPix)) {
      REQUIRE(n == 2 && in[1] < 32);
      out[count++] = (uint32_t)VP8LHashPix(in[0], in[1]);
    } else if (IS(VP8LColorCache)) {
      VP8LColorCache cache, copy;
      uint32_t size;
      REQUIRE(n >= 1 && in[0] >= 1 && in[0] <= 11 && (n - 1) % 3 == 0);
      REQUIRE(VP8LColorCacheInit(&cache, (int)in[0]));
      REQUIRE(VP8LColorCacheInit(&copy, (int)in[0]));
      size = 1u << in[0];
      for (i = 1; i < n; i += 3) {
        const uint32_t op = in[i], key = in[i + 1], pixel = in[i + 2];
        REQUIRE(count < LIMIT - 2 * size);
        if (op == 0) VP8LColorCacheInsert(&cache, pixel);
        else if (op == 1) out[count++] = (uint32_t)VP8LColorCacheContains(&cache, pixel);
        else if (op == 2) out[count++] = (uint32_t)VP8LColorCacheGetIndex(&cache, pixel);
        else if (op == 3) { REQUIRE(key < size); out[count++] = VP8LColorCacheLookup(&cache, key); }
        else if (op == 4) { REQUIRE(key < size); VP8LColorCacheSet(&cache, key, pixel); }
        else if (op == 5) VP8LColorCacheCopy(&cache, &copy);
        else if (op == 6) { REQUIRE(key < size); out[count++] = VP8LColorCacheLookup(&copy, key); }
        else REQUIRE(0);
      }
      // Return both complete logical tables to expose collisions and aliasing.
      for (i = 0; i < size; ++i) out[count++] = VP8LColorCacheLookup(&cache, i);
      for (i = 0; i < size; ++i) out[count++] = VP8LColorCacheLookup(&copy, i);
      VP8LColorCacheClear(&cache);
      VP8LColorCacheClear(&copy);
    } else if (IS(VP8LSubtractGreenFromBlueAndRed_C)) {
      VP8LSubtractGreenFromBlueAndRed_C(in, (int)n);
      memcpy(out, in, n * sizeof(*in));
      count = n;
    } else if (IS(VP8LTransformColor_C)) {
      VP8LMultipliers m;
      REQUIRE(n >= 3 && in[0] <= 255 && in[1] <= 255 && in[2] <= 255);
      m.green_to_red = (uint8_t)in[0];
      m.green_to_blue = (uint8_t)in[1];
      m.red_to_blue = (uint8_t)in[2];
      VP8LTransformColor_C(&m, in + 3, (int)n - 3);
      count = n - 3;
      memcpy(out, in + 3, count * sizeof(*in));
    } else if (IS(WebPEncodeLosslessRGBA)) {
      uint8_t *allocation, *rgba, *encoded = NULL;
      size_t size;
      uint64_t required;
      REQUIRE(n >= 3 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383);
      REQUIRE(in[2] >= in[0] * 4 && in[2] <= INT32_MAX);
      required = (uint64_t)(in[1] - 1) * in[2] + in[0] * 4;
      REQUIRE(required == n - 3);
      allocation = (uint8_t*)malloc(n - 3 + 1);
      REQUIRE(allocation != NULL);
      rgba = allocation + 1;  // Exercise the importer's unaligned input path.
      for (i = 3; i < n; ++i) { REQUIRE(in[i] <= 255); rgba[i - 3] = (uint8_t)in[i]; }
      size = WebPEncodeLosslessRGBA(rgba, (int)in[0], (int)in[1], (int)in[2], &encoded);
      REQUIRE(size > 0 && size <= LIMIT && encoded != NULL);
      for (i = 0; i < size; ++i) out[count++] = encoded[i];
      WebPFree(encoded);
      free(allocation);
    } else REQUIRE(0);
    printf("%u", count);
    for (i = 0; i < count; ++i) printf(" %08" PRIx32, out[i]);
    putchar('\n');
  }
  free(in);
  free(out);
  return ferror(stdin) || ferror(stdout) ? 1 : 0;
}
