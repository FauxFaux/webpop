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
#include "src/utils/bit_writer_utils.h"
#include "src/utils/huffman_encode_utils.h"
#include "src/utils/utils.h"
#include "src/utils/palette.h"
#include "src/webp/encode.h"

#define LIMIT (1u << 20)
#define REQUIRE(x) do { if (!(x)) { fprintf(stderr, "invalid request: %s\n", name); return 1; } } while (0)
#define IS(x) (strcmp(name, #x) == 0)

void WebpopStoreHuffmanCode(VP8LBitWriter*, const HuffmanTreeCode*);
void WebpopClearHuffmanTree(HuffmanTreeCode*);
void WebpopWriteHuffmanCodeWithExtraBits(VP8LBitWriter*, const HuffmanTreeCode*, int, int, int);
unsigned WebpopTokenStream(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopImageHeaders(const uint32_t*, uint32_t*);
unsigned WebpopPaletteTransform(const uint32_t*, uint32_t*);
unsigned WebpopAuxiliary(const uint32_t*, uint32_t*);
unsigned WebpopMainImage(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopCosts(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopTrace(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopGetRefs(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopMatches(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopRefs(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopPopulationCost(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopHistogram(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopHistogramSet(const uint32_t*, uint32_t*);
unsigned WebpopClustering(const uint32_t*, unsigned, uint32_t*);
extern void VP8LOptimizeSampling(uint32_t*, int, int, int, int, int*);
extern int VP8LDistanceToPlaneCode(int, int);
int WebpopNearLossless(const char*, const uint32_t*, unsigned, uint32_t*, unsigned*);

static unsigned InspectWriter(const VP8LBitWriter* bw, uint32_t* out) {
  VP8LBitWriter copy;
  size_t i, size;
  if (!VP8LBitWriterInit(&copy, 0) || !VP8LBitWriterClone(bw, &copy)) abort();
  out[0] = (uint32_t)VP8LBitWriterNumBytes(bw);
  VP8LBitWriterFinish(&copy);
  if (copy.error) abort();
  size = VP8LBitWriterNumBytes(&copy);
  out[1] = (uint32_t)size;
  for (i = 0; i < size; ++i) out[2 + i] = copy.buf[i];
  VP8LBitWriterWipeOut(&copy);
  return (unsigned)size + 2;
}

static uint64_t Read64(const uint32_t* words) {
  return words[0] | ((uint64_t)words[1] << 32);
}
static void Write64(uint32_t* out, uint64_t v) {
  out[0] = (uint32_t)v;
  out[1] = (uint32_t)(v >> 32);
}
static unsigned WriteEntropy(uint32_t* out, const VP8LBitEntropy* e) {
  Write64(out, e->entropy);
  out[2] = e->sum;
  out[3] = (uint32_t)e->nonzeros;
  out[4] = e->max_val;
  out[5] = e->nonzero_code;
  return 6;
}

int main(void) {
  char name[80];
  uint32_t *in = (uint32_t*)malloc(LIMIT * sizeof(*in));
  uint32_t *out = (uint32_t*)malloc(LIMIT * sizeof(*out));
  if (in == NULL || out == NULL) return 1;
  VP8LDspInit();
  VP8LEncDspInit();
  while (scanf("%79s", name) == 1) {
    unsigned n, count = 0, i;
    REQUIRE(scanf("%u", &n) == 1 && n <= LIMIT);
    for (i = 0; i < n; ++i) REQUIRE(scanf("%" SCNx32, &in[i]) == 1);
    if (IS(EncodeImageInternal)) {
      REQUIRE(n >= 11 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383 && in[2] <= 100 && in[3] <= 1 && in[4] <= 10 && in[5] >= 2 && in[5] <= 9 && in[6] <= 97 && in[7] >= 1 && in[7] <= 2);
      REQUIRE((uint64_t)in[0] * in[1] == n - 8 - 2 * in[7] && n < 20000);
      count = WebpopMainImage(in, n, out);
    } else if (IS(PaletteTransform)) {
      REQUIRE(n >= 8 && in[0] > 0 && in[0] <= 256);
      REQUIRE(in[1] > 0 && in[2] > 0 && in[3] >= in[1] && in[4] <= 1 && in[5] <= 97);
      REQUIRE(n == 6 + in[0] + (in[2] - 1) * in[3] + in[1]);
      count = WebpopPaletteTransform(in, out);
    } else if (IS(PaletteSpatial)) {
      WebPPicture pic;
      uint32_t palette[256];
      unsigned i, size;
      REQUIRE(n >= 7 && in[0] <= 4 && in[1] > 0 && in[2] > 0 && in[3] >= in[1]);
      size = in[4];
      REQUIRE(size > 0 && size <= 256);
      REQUIRE(n == 5 + size + (in[2] - 1) * in[3] + in[1]);
      WebPPictureInit(&pic);
      pic.use_argb = 1; pic.width = in[1]; pic.height = in[2];
      pic.argb_stride = in[3]; pic.argb = in + 5 + size;
      REQUIRE(PaletteSort((PaletteSorting)in[0], &pic, in + 5, size, palette));
      for (i = 0; i < size; ++i) out[count++] = palette[i];
    } else if (IS(PaletteLeaves)) {
      uint32_t palette[256], sorted[256], map[256];
      unsigned size = n - 1, i;
      REQUIRE(n > 1);
      if (in[0] == 3) {
        WebPPicture pic;
        REQUIRE(n >= 5 && in[1] > 0 && in[2] > 0 && in[3] >= in[1]);
        REQUIRE(n == 4 + (in[2] - 1) * in[3] + in[1]);
        WebPPictureInit(&pic);
        pic.use_argb = 1; pic.width = in[1]; pic.height = in[2];
        pic.argb_stride = in[3]; pic.argb = in + 4;
        size = GetColorPalette(&pic, palette);
        out[count++] = size;
        if (size <= 256) for (i = 0; i < size; ++i) out[count++] = palette[i];
      } else {
        REQUIRE(size <= 256);
        if (in[0] == 0) {
          PrepareMapToPalette(in + 1, size, sorted, map);
          for (i = 0; i < size; ++i) out[count++] = sorted[i];
          for (i = 0; i < size; ++i) out[count++] = map[i];
        } else {
          REQUIRE(in[0] == 1 || in[0] == 2);
          REQUIRE(PaletteSort(in[0] == 1 ? kSortedDefault : kMinimizeDelta,
                              NULL, in + 1, size, palette));
          for (i = 0; i < size; ++i) out[count++] = palette[i];
        }
      }
    } else if (IS(VP8LOptimizeSampling)) {
      int best_bits;
      REQUIRE(n >= 5 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383 && in[2] <= in[3] && in[3] <= 9);
      REQUIRE((uint64_t)VP8LSubSampleSize(in[0], in[2]) * VP8LSubSampleSize(in[1], in[2]) == n - 4);
      VP8LOptimizeSampling(in + 4, in[0], in[1], in[2], in[3], &best_bits);
      out[count++] = best_bits;
      for (i = 4; i < n; ++i) out[count++] = in[i];
    } else if (IS(VP8LClustering)) {
      REQUIRE(n >= 9 && n < 50000 && (n - 6) % 3 == 0 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383 && in[2] <= 100 && in[3] <= 1 && in[4] >= 1 && in[4] <= 9 && in[5] <= 10);
      REQUIRE((uint64_t)VP8LSubSampleSize(in[0], in[4]) * VP8LSubSampleSize(in[1], in[4]) <= 1200);
      count = WebpopClustering(in, n, out);
    } else if (IS(TokenStream)) {
      REQUIRE(n >= 7 && n < 50000 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383 && in[2] <= 9 && in[3] <= 10 && in[4] > 0 && in[4] <= 16 && in[5] <= 31 && in[6] < n - 6 && (n - 7 - in[6]) % 3 == 0);
      count = WebpopTokenStream(in, n, out);
    } else if (IS(ImageHeaders)) {
      REQUIRE(n == 5 && in[0] >= 1 && in[0] <= 16383 && in[1] >= 1 && in[1] <= 16383 && in[2] <= 1 && in[4] <= 31);
      count = WebpopImageHeaders(in, out);
    } else if (IS(EncodeImageNoHuffman)) {
      REQUIRE(n >= 7 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383 && (uint64_t)in[0] * in[1] == n - 6 && n < 20000 && in[2] <= 100 && in[3] <= 1 && in[5] <= 31);
      count = WebpopAuxiliary(in, out);
    } else if (IS(VP8LCostEstimates)) {
      REQUIRE(n > 0 && n <= 2328);
      count = WebpopCosts(in, n, out);
    } else if (IS(VP8LTrace)) {
      REQUIRE(n >= 7 && n < 200000 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[0] * in[1] == n - 6 && in[5] <= 10 && in[2] <= 100 && in[3] <= 1 && in[4] >= 1 && in[4] <= 3);
      count = WebpopTrace(in, n, out);
    } else if (IS(VP8LGetBackwardReferences)) {
      REQUIRE(n >= 8 && n < 200000 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[0] * in[1] == n - 7 && in[5] <= 10 && in[2] <= 100 && in[3] <= 1 && in[4] > 0 && in[4] <= 7 && in[6] <= 1);
      count = WebpopGetRefs(in, n, out);
    } else if (IS(VP8LMatches)) {
      REQUIRE(n >= 7 && n < 200000 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[0] * in[1] == n - 6);
      count = WebpopMatches(in, n, out);
    } else if (IS(VP8LBackwardRefs)) {
      REQUIRE(n >= 1 && (n - 1) % 3 == 0 && n < 10000 && in[0] < 10000);
      count = WebpopRefs(in, n, out);
    } else if (IS(PopulationCost)) {
      REQUIRE(n > 0 && n <= 2328);
      count = WebpopPopulationCost(in, n, out);
    } else if (IS(VP8LHistogram)) {
      REQUIRE(n >= 2 && (n - 2) % 3 == 0 && n < 10000 && in[0] <= 11 && in[1] <= 16383);
      count = WebpopHistogram(in, n, out);
    } else if (IS(VP8LHistogramSet)) {
      REQUIRE(n == 2 && in[0] > 0 && in[0] <= 16 && in[1] <= 11);
      count = WebpopHistogramSet(in, out);
    } else if (IS(VP8LDistanceToPlaneCode)) {
      REQUIRE(n == 2 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 0x7fffffff - 120);
      out[count++] = VP8LDistanceToPlaneCode((int)in[0], (int)in[1]);
    } else if (IS(VP8LBitWriter)) {
      VP8LBitWriter bw, saved;
      REQUIRE(n >= 1 && in[0] <= LIMIT && (n - 1) % 3 == 0);
      REQUIRE(VP8LBitWriterInit(&bw, in[0]));
      REQUIRE(VP8LBitWriterInit(&saved, 0));
      for (i = 1; i < n; i += 3) {
        const uint32_t op = in[i], value = in[i + 1], bits = in[i + 2];
        if (op == 0) {
          REQUIRE(bits <= 32 && (bits == 0 || bits == 32 || value < (1u << bits)));
          VP8LPutBits(&bw, value, (int)bits);
        } else if (op == 1) { REQUIRE(VP8LBitWriterClone(&bw, &saved)); }
        else if (op == 2) {
          REQUIRE(saved.cur - saved.buf <= bw.cur - bw.buf);
          VP8LBitWriterReset(&saved, &bw);
        } else if (op == 3) VP8LBitWriterSwap(&bw, &saved);
        else if (op == 4) VP8LBitWriterFinish(&bw);
        else if (op == 5) {
          REQUIRE(count + VP8LBitWriterNumBytes(&bw) + 2 <= LIMIT);
          count += InspectWriter(&bw, out + count);
        } else if (op == 6) {
          REQUIRE(value <= LIMIT);
          VP8LBitWriterWipeOut(&bw);
          REQUIRE(VP8LBitWriterInit(&bw, value));
        } else REQUIRE(0);
        REQUIRE(!bw.error && !saved.error);
      }
      REQUIRE(count + VP8LBitWriterNumBytes(&bw) + 2 <= LIMIT);
      count += InspectWriter(&bw, out + count);
      VP8LBitWriterWipeOut(&bw);
      VP8LBitWriterWipeOut(&saved);
    } else if (IS(VP8LCreateHuffmanTree) || IS(StoreHuffmanCode)) {
      const int store = IS(StoreHuffmanCode);
      uint32_t *histogram;
      uint8_t *lengths, *rle;
      uint16_t* codes;
      HuffmanTree* scratch;
      HuffmanTreeCode code;
      unsigned size, start, limit;
      REQUIRE(n >= (store ? 4u : 1u));
      start = store ? 4 : 1;
      size = store ? in[3] : n - 1;
      limit = store ? in[2] : in[0];
      REQUIRE(limit >= 1 && limit <= 15 && size <= 4096 && start + size <= n);
      REQUIRE(!store || (in[1] <= 31 && (in[1] == 0 || in[0] < (1u << in[1])) && (n - start - size) % 3 == 0));
      histogram = (uint32_t*)calloc(size + 1, sizeof(*histogram));
      lengths = (uint8_t*)calloc(size + 1, 1);
      rle = (uint8_t*)calloc(size + 1, 1);
      codes = (uint16_t*)calloc(size + 1, sizeof(*codes));
      scratch = (HuffmanTree*)calloc(3 * (size + 1), sizeof(*scratch));
      REQUIRE(histogram && lengths && rle && codes && scratch);
      memcpy(histogram, in + start, size * sizeof(*histogram));
      code.num_symbols = (int)size;
      code.code_lengths = lengths;
      code.codes = codes;
      VP8LCreateHuffmanTree(histogram, (int)limit, rle, scratch, &code);
      if (store) {
        VP8LBitWriter bw;
        REQUIRE(VP8LBitWriterInit(&bw, 0));
        VP8LPutBits(&bw, in[0], (int)in[1]);
        WebpopStoreHuffmanCode(&bw, &code);
        WebpopClearHuffmanTree(&code);
        for (i = start + size; i < n; i += 3) {
          REQUIRE(in[i] < size && in[i + 2] <= 16 && (in[i + 2] == 0 || in[i + 1] < (1u << in[i + 2])));
          WebpopWriteHuffmanCodeWithExtraBits(&bw, &code, (int)in[i], (int)in[i + 1], (int)in[i + 2]);
        }
        VP8LPutBits(&bw, 0x5ad, 11);  // Expose exact bit length before padding.
        count = InspectWriter(&bw, out);
        for (i = 0; i < size; ++i) out[count++] = lengths[i];
        for (i = 0; i < size; ++i) out[count++] = codes[i];
        VP8LBitWriterWipeOut(&bw);
      } else {
        for (i = 0; i < size; ++i) out[count++] = histogram[i];
        for (i = 0; i < size; ++i) out[count++] = lengths[i];
        for (i = 0; i < size; ++i) out[count++] = codes[i];
      }
      free(histogram); free(lengths); free(rle); free(codes); free(scratch);
    } else if (IS(VP8LCreateCompressedHuffmanTree)) {
      HuffmanTreeCode code;
      HuffmanTreeToken* tokens;
      uint8_t* lengths;
      int num_tokens;
      REQUIRE(n <= 4096);
      lengths = (uint8_t*)malloc(n + 1);
      tokens = (HuffmanTreeToken*)calloc(n + 1, sizeof(*tokens));
      REQUIRE(lengths && tokens);
      for (i = 0; i < n; ++i) { REQUIRE(in[i] <= 15); lengths[i] = (uint8_t)in[i]; }
      code.num_symbols = (int)n;
      code.code_lengths = lengths;
      code.codes = NULL;
      num_tokens = VP8LCreateCompressedHuffmanTree(&code, tokens, (int)n);
      for (i = 0; i < (unsigned)num_tokens; ++i) {
        out[count++] = tokens[i].code;
        out[count++] = tokens[i].extra_bits;
      }
      free(lengths); free(tokens);
    } else if (IS(PutLE32)) {
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
      REQUIRE(n == 1 && in[0] <= INT32_MAX);
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
    } else if (IS(VP8LFastLog2) || IS(VP8LFastSLog2) || IS(FastLog2Slow_C) || IS(FastSLog2Slow_C)) {
      REQUIRE(n == 1);
      if (IS(FastLog2Slow_C) || IS(FastSLog2Slow_C)) REQUIRE(in[0] >= 256);
      if (IS(VP8LFastLog2) || IS(FastLog2Slow_C)) {
        out[count++] = IS(VP8LFastLog2) ? VP8LFastLog2(in[0]) : VP8LFastLog2Slow(in[0]);
      } else {
        Write64(out, IS(VP8LFastSLog2) ? VP8LFastSLog2(in[0]) : VP8LFastSLog2Slow(in[0]));
        count = 2;
      }
    } else if (IS(RightShiftRound)) {
      REQUIRE(n == 3 && in[2] < 64);
      Write64(out, RightShiftRound(Read64(in), in[2]));
      count = 2;
    } else if (IS(DivRound)) {
      REQUIRE(n == 4 && Read64(in + 2) != 0);
      Write64(out, (uint64_t)DivRound((int64_t)Read64(in), (int64_t)Read64(in + 2)));
      count = 2;
    } else if (IS(VP8LPredictors)) {
      REQUIRE(n == 5 && in[0] < 14);
      out[count++] = VP8LPredictors[in[0]](in + 1, in + 3);
    } else if (IS(VP8LPredictorsSub)) {
      unsigned len;
      REQUIRE(n >= 4 && in[0] < 14 && (n - 4) % 2 == 0);
      len = (n - 4) / 2;
      VP8LPredictorsSub[in[0]](in + 2, in + len + 3, (int)len, out);
      count = len;
    } else if (IS(ShannonEntropy_C) || IS(CombinedShannonEntropy_C)) {
      if (IS(ShannonEntropy_C)) Write64(out, VP8LShannonEntropy(in, (int)n));
      else {
        REQUIRE(n == 512);
        Write64(out, VP8LCombinedShannonEntropy(in, in + 256));
      }
      count = 2;
    } else if (IS(VP8LBitEntropyInit) || IS(VP8LBitsEntropyUnrefined) ||
               IS(GetEntropyUnrefined_C) || IS(GetCombinedEntropyUnrefined_C)) {
      VP8LBitEntropy e;
      VP8LStreaks stats;
      if (IS(VP8LBitEntropyInit)) { REQUIRE(n == 0); VP8LBitEntropyInit(&e); }
      else if (IS(VP8LBitsEntropyUnrefined)) VP8LBitsEntropyUnrefined(in, (int)n, &e);
      else if (IS(GetEntropyUnrefined_C)) {
        REQUIRE(n > 0);
        VP8LGetEntropyUnrefined(in, (int)n, &e, &stats);
      } else {
        REQUIRE(n > 0 && n % 2 == 0);
        VP8LGetCombinedEntropyUnrefined(in, in + n/2, (int)n/2, &e, &stats);
      }
      count = WriteEntropy(out, &e);
      if (IS(GetEntropyUnrefined_C) || IS(GetCombinedEntropyUnrefined_C)) {
        for (i = 0; i < 2; ++i) out[count++] = (uint32_t)stats.counts[i];
        for (i = 0; i < 4; ++i) out[count++] = (uint32_t)stats.streaks[i/2][i%2];
      }
    } else if (IS(ExtraCost_C)) {
      REQUIRE(n >= 6 && n % 2 == 0);
      out[count++] = VP8LExtraCost(in, (int)n);
    } else if (IS(AddVector_C) || IS(AddVectorEq_C) || IS(VectorMismatch_C)) {
      REQUIRE(n % 2 == 0);
      if (IS(VectorMismatch_C)) out[count++] = VP8LVectorMismatch(in, in + n/2, (int)n/2);
      else {
        count = n/2;
        if (IS(AddVector_C)) VP8LAddVector(in, in + count, out, (int)count);
        else {
          memcpy(out, in + count, count * sizeof(*out));
          VP8LAddVectorEq(in, out, (int)count);
        }
      }
    } else if (IS(VP8LBundleColorMap_C)) {
      uint8_t* row;
      REQUIRE(n >= 1 && in[0] <= 3);
      row = (uint8_t*)malloc(n);
      REQUIRE(row != NULL);
      for (i = 1; i < n; ++i) { REQUIRE(in[i] <= 255); row[i-1] = (uint8_t)in[i]; }
      count = (n - 1 + (1u << in[0]) - 1) >> in[0];
      VP8LBundleColorMap_C(row, (int)n - 1, (int)in[0], out);
      free(row);
    } else if (IS(VP8LCollectColorRedTransforms_C) || IS(VP8LCollectColorBlueTransforms_C)) {
      uint64_t required;
      REQUIRE(n >= 261 && in[0] >= in[1] && in[3] <= 255 && in[4] <= 255);
      required = (in[2] == 0) ? 0 : (uint64_t)(in[2] - 1) * in[0] + in[1];
      REQUIRE(required <= n - 261);
      memcpy(out, in + 5, 256 * sizeof(*out));
      if (IS(VP8LCollectColorRedTransforms_C))
        VP8LCollectColorRedTransforms_C(in + 261, (int)in[0], (int)in[1], (int)in[2], (int)in[3], out);
      else VP8LCollectColorBlueTransforms_C(in + 261, (int)in[0], (int)in[1], (int)in[2], (int)in[3], (int)in[4], out);
      count = 256;
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
    } else if (WebpopNearLossless(name, in, n, out, &count)) {
      // Standalone near-lossless leaves and preprocessing passes.
    } else REQUIRE(0);
    printf("%u", count);
    for (i = 0; i < count; ++i) printf(" %08" PRIx32, out[i]);
    putchar('\n');
  }
  free(in);
  free(out);
  return ferror(stdin) || ferror(stdout) ? 1 : 0;
}
