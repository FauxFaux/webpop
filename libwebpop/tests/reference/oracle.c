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
#include "src/enc/vp8i_enc.h"
#include "src/enc/cost_enc.h"
#include "src/utils/color_cache_utils.h"
#include "src/utils/bit_writer_utils.h"
#include "src/utils/huffman_encode_utils.h"
#include "src/utils/utils.h"
#include "src/utils/palette.h"
#include "src/webp/encode.h"

#if defined(WEBP_HAVE_SSE2) || defined(WEBP_HAVE_SSE41) || defined(WEBP_HAVE_AVX2) || defined(WEBP_HAVE_NEON) || defined(WEBP_USE_THREAD)
#error "The native oracle must use the generated scalar, unthreaded configuration"
#endif
#if defined(DISABLE_TOKEN_BUFFER)
#error "The native oracle must retain normal method-dependent token buffering"
#endif

#define LIMIT (1u << 20)
#define REQUIRE(x) do { if (!(x)) { fprintf(stderr, "invalid request: %s\n", name); return 1; } } while (0)
#define IS(x) (strcmp(name, #x) == 0)

void WebpopStoreHuffmanCode(VP8LBitWriter*, const HuffmanTreeCode*);
void WebpopClearHuffmanTree(HuffmanTreeCode*);
void WebpopWriteHuffmanCodeWithExtraBits(VP8LBitWriter*, const HuffmanTreeCode*, int, int, int);
unsigned WebpopTokenStream(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopImageHeaders(const uint32_t*, uint32_t*);
unsigned WebpopExpandMatrix(const uint32_t*, uint32_t*);
unsigned WebpopQuantSetup(const uint32_t*, uint32_t*);
unsigned WebpopAnalysisQuant(const uint32_t*, uint32_t*);
unsigned WebpopQuantizeSingle(const uint32_t*, uint32_t*);
unsigned WebpopTrellisScore(const uint32_t*, uint32_t*);
unsigned WebpopTrellisBlock(const uint32_t*, uint32_t*);
unsigned WebpopDCError(const uint32_t*, uint32_t*);
unsigned WebpopReconstruct(const uint32_t*, uint32_t*);
unsigned WebpopTrellisReconstruct(const uint32_t*, uint32_t*);
unsigned WebpopTrellisSearch(const uint32_t*, uint32_t*);
unsigned WebpopFastAnalyze(const uint32_t*, uint32_t*);
unsigned WebpopFastAnalysisFrame(const uint32_t*, uint32_t*);
unsigned WebpopDistortionSearch(const uint32_t*, uint32_t*);
unsigned WebpopFastSearchFrame(const uint32_t*, uint32_t*);
unsigned WebpopModeSearch(const uint32_t*, uint32_t*);
unsigned WebpopFlat(const uint32_t*, uint32_t*);
unsigned WebpopModeCost(const uint32_t*, uint32_t*);
unsigned WebpopScoreOps(const uint32_t*, uint32_t*);
unsigned WebpopAlphaOps(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAlphaMethodOps(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopLoopFrame(const uint32_t*, uint32_t*);
unsigned WebpopMethodLoopFrame(const uint32_t*, uint32_t*);
unsigned WebpopMethodLoopTrace(const uint32_t*, uint32_t*);
unsigned WebpopMethodTools(const uint32_t*, uint32_t*);
unsigned WebpopLossyMethodEncode(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopFrameSegments(const uint32_t*, uint32_t*);
unsigned WebpopAdjustFilter(const uint32_t*, uint32_t*);
unsigned WebpopModeSyntax(const uint32_t*, uint32_t*);
unsigned WebpopSyntax(const uint32_t*, uint32_t*);
unsigned WebpopFixedImage(const uint32_t*, uint32_t*);
unsigned WebpopSearchFrame(const uint32_t*, uint32_t*);
unsigned WebpopPaletteTransform(const uint32_t*, uint32_t*);
unsigned WebpopNearLosslessFeedback(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopPredictor(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopCrossColor(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopCrossColorTransform(const uint32_t*, uint32_t*);
int WebpopClampBits(const uint32_t*);
unsigned WebpopPredictorTransform(const uint32_t*, uint32_t*);
unsigned WebpopAuxiliary(const uint32_t*, uint32_t*);
unsigned WebpopMainImage(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnalysis(const char*, const uint32_t*, uint32_t*);
unsigned WebpopNearLosslessStream(const uint32_t*, uint32_t*);
unsigned WebpopEncodeStream(const uint32_t*, uint32_t*);
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
int WebpopNearLosslessPredictor(const char*, const uint32_t*, unsigned, uint32_t*, unsigned*);
size_t WebPEncodeRGBAWithNearLossless(const uint8_t*, int, int, int, int, uint8_t**);
unsigned WebpopLossyTransform(const uint32_t*, uint32_t*);
unsigned WebpopLossyTransform2(const uint32_t*, uint32_t*);
unsigned WebpopLossyInverseTransform(const uint32_t*, uint32_t*);
unsigned WebpopLossyWHT(const uint32_t*, int, uint32_t*);
unsigned WebpopLossyPredictions(const char*, const uint32_t*, uint32_t*);
unsigned WebpopLossyEncode(const uint32_t*, unsigned, int, uint32_t*);
unsigned WebpopAlphaWrapperErrors(uint32_t*);
unsigned WebpopVP8DspTables(uint32_t*);
unsigned WebpopVP8Quantize(const uint32_t*, uint32_t*);
unsigned WebpopVP8Metric(const uint32_t*, uint32_t*);
unsigned WebpopVP8Histogram(const uint32_t*, int, uint32_t*);
int WebpopBooleanWriter(const uint32_t*, unsigned, uint32_t*, unsigned*);
unsigned WebpopVP8CostTables(uint32_t*);
unsigned WebpopVP8LevelCosts(const uint32_t*, uint32_t*);
unsigned WebpopVP8Residual(const uint32_t*, uint32_t*);
unsigned WebpopVP8WriteProbas(const uint32_t*, uint32_t*);
unsigned WebpopVP8DefaultProbas(uint32_t*);
unsigned WebpopTokens(const uint32_t*, uint32_t*);
unsigned WebpopDirectCoeffs(const uint32_t*, uint32_t*);
unsigned WebpopDirectResiduals(const uint32_t*, uint32_t*);
unsigned WebpopSkipProba(const uint32_t*, uint32_t*);
unsigned WebpopTokenProba(const uint32_t*, uint32_t*);
unsigned WebpopProbaState(const uint32_t*, uint32_t*);
unsigned WebpopYuvTables(uint32_t*);
unsigned WebpopYuvGamma(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopYuvMatrix(const uint32_t*, int, uint32_t*);
unsigned WebpopYuvAccumulate(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopYuvaImport(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopIteratorScan(const uint32_t*, uint32_t*);
unsigned WebpopIteratorNz(const uint32_t*, uint32_t*);
unsigned WebpopIteratorCosts(const uint32_t*, uint32_t*);
unsigned WebpopSusceptibility(const uint32_t*, uint32_t*);
unsigned WebpopSegmentAlphas(const uint32_t*, uint32_t*);
unsigned WebpopAssignSegments(const uint32_t*, uint32_t*);
unsigned WebpopAnalysisProbe(const uint32_t*, uint32_t*);
unsigned WebpopAnalysisFrame(const uint32_t*, uint32_t*);
int WebpopSharpYuvLeaves(const char*, const uint32_t*, unsigned, uint32_t*, unsigned*);
int WebpopSharpYuvDsp(const uint32_t*, unsigned, uint32_t*, unsigned*);
int WebpopSharpYuvPublic(const char*, const uint32_t*, unsigned, uint32_t*, unsigned*);
unsigned WebpopSharpYuvEncoder(const uint32_t*, unsigned, int, uint32_t*);
unsigned WebpopAnimationLeaves(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationSubframes(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationLossless(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationLossyCandidate(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationLossy(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationLossyCarryoverSequence(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationNearLossless(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationCarryover(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationCarryoverSequence(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationCache(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationNearLosslessCache(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationDecode(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationMux(const uint32_t*, unsigned, uint32_t*);
unsigned WebpopAnimationSingleFrame(const uint32_t*, unsigned, uint32_t*);

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
    if (IS(AnimationMux)) {
      count = WebpopAnimationMux(in, n, out);
    } else if (IS(AnimationSingleFrame)) {
      count = WebpopAnimationSingleFrame(in, n, out);
    } else if (IS(AnimationLeaves)) {
      count = WebpopAnimationLeaves(in, n, out);
    } else if (IS(AnimationSubframes)) {
      count = WebpopAnimationSubframes(in, n, out);
    } else if (IS(AnimationLossless)) {
      count = WebpopAnimationLossless(in, n, out);
    } else if (IS(AnimationLossyCandidate)) {
      count = WebpopAnimationLossyCandidate(in, n, out);
    } else if (IS(AnimationLossy)) {
      count = WebpopAnimationLossy(in, n, out);
    } else if (IS(AnimationLossyCarryoverSequence)) {
      count = WebpopAnimationLossyCarryoverSequence(in, n, out);
    } else if (IS(AnimationNearLossless)) {
      count = WebpopAnimationNearLossless(in, n, out);
    } else if (IS(AnimationCarryover)) {
      count = WebpopAnimationCarryover(in, n, out);
    } else if (IS(AnimationCarryoverSequence)) {
      count = WebpopAnimationCarryoverSequence(in, n, out);
    } else if (IS(AnimationCache)) {
      count = WebpopAnimationCache(in, n, out);
    } else if (IS(AnimationNearLosslessCache)) {
      count = WebpopAnimationNearLosslessCache(in, n, out);
    } else if (IS(AnimationDecode)) {
      count = WebpopAnimationDecode(in, n, out);
    } else if (!strncmp(name, "SharpYuv", 8)) {
      if (IS(SharpYuvConvert) || IS(SharpYuvPublicInfo) || IS(SharpYuvTrace) || IS(SharpYuvImport) || IS(SharpYuvTimedConvert)) {
        REQUIRE(WebpopSharpYuvPublic(name, in, n, out, &count));
      } else if (IS(SharpYuvDsp)) {
        REQUIRE(WebpopSharpYuvDsp(in, n, out, &count));
      } else {
        REQUIRE(WebpopSharpYuvLeaves(name, in, n, out, &count));
      }
    } else if (IS(WebPEncodeLossyMethod)) {
      uint64_t input_size;
      REQUIRE(n>=7 && in[0]>=1 && in[0]<=16383 && in[1]>=1 && in[1]<=16383);
      REQUIRE(in[2]>=4*in[0] && in[2]<=4*in[0]+64 && in[6]<=1);
      input_size=(uint64_t)(in[1]-1)*in[2]+4*in[0];
      REQUIRE(n==7+input_size);
      for(i=7;i<n;++i) REQUIRE(in[i]<=255);
      count=WebpopLossyMethodEncode(in,n,out);
    } else if (IS(VP8MethodTools)) {
      float quality;
      REQUIRE(n==6 && in[0]>=1 && in[0]<=16383 && in[1]>=1 && in[1]<=16383);
      memcpy(&quality,in+2,sizeof(quality));REQUIRE(quality>=0.f && quality<=100.f);
      REQUIRE(in[3]<=6 && in[4]<=1 && in[5]<=3);
      count=WebpopMethodTools(in,out);
    } else if (IS(WebPEncodeLossyOptions) || IS(YuvaSharpImport)) {
      const unsigned offset=IS(YuvaSharpImport)?4:7;
      uint64_t input_size,output_size;
      REQUIRE(n>=offset && in[0]>=1 && in[0]<=16383 && in[1]>=1 && in[1]<=16383);
      REQUIRE(in[2]>=4*in[0] && in[2]<=4*in[0]+64);
      input_size=(uint64_t)(in[1]-1)*in[2]+4*in[0];
      output_size=2+2*(uint64_t)in[0]*in[1]+2*(uint64_t)((in[0]+1)/2)*((in[1]+1)/2);
      REQUIRE(n==offset+input_size && output_size<=LIMIT);
      for(i=offset;i<n;++i) REQUIRE(in[i]<=255);
      if(IS(YuvaSharpImport)) REQUIRE(in[3]<=1);
      else {
        float quality;memcpy(&quality,in+3,sizeof(quality));
        REQUIRE(quality>=0.f && quality<=100.f && in[4]<=100 && in[5]<=1 && (in[6]==0 || in[6]==4));
      }
      count=WebpopSharpYuvEncoder(in,n,IS(YuvaSharpImport),out);
    } else if (IS(VP8SearchFrame) || IS(VP8FastSearchFrame)) {
      uint32_t* request = in; unsigned size = n;
      if (IS(VP8FastSearchFrame)) {REQUIRE(n>=6 && in[0]<=2); ++in; --n;}
      uint64_t pixels, mbw, mbh, output_size; float quality;
      REQUIRE(n >= 5 && in[0]>=1 && in[0]<=16383 && in[1]>=1 && in[1]<=16383 && in[2]>=1 && in[2]<=4 && in[4]<=65536);
      memcpy(&quality,in+3,sizeof(quality));REQUIRE(quality>=0.f && quality<=100.f);
      pixels=(uint64_t)in[0]*in[1]+2*(uint64_t)((in[0]+1)/2)*((in[1]+1)/2);
      mbw=(in[0]+15)/16;mbh=(in[1]+15)/16;
      output_size=1067+1047*mbw*mbh+(4*mbw+1)*(4*mbh+1)+mbw+(quality<=98.f?4*mbw:0);
      REQUIRE(n==5+pixels && output_size<=LIMIT);
      for(i=5;i<n;++i) REQUIRE(in[i]<=255);
      count=IS(VP8FastSearchFrame) ? WebpopFastSearchFrame(request,out) : WebpopSearchFrame(in,out);
      in=request; n=size;
    } else if (IS(VP8ModeSyntax)) {
      REQUIRE(n == 7 && in[0] <= 3 && in[2] < 10 && in[3] < 10);
      REQUIRE(in[1] < (in[0] == 0 ? 10 : 4));
      for (i = 4; i < 7; ++i) REQUIRE(in[i] <= 255);
      count = WebpopModeSyntax(in, out);
    } else if (IS(VP8Syntax)) {
      unsigned mw, mh, ps, at;
      REQUIRE(n >= 31 && in[0] < 6 && in[1] > 0 && in[1] <= 129 && in[2] > 0 && in[2] <= 129);
      mw = (in[1]+15)/16; mh = (in[2]+15)/16; ps = (4*mw+1)*(4*mh+1);
      REQUIRE(n == 31 + ps + 4*mw*mh && in[3] <= 3 && in[4] >= 1 && in[4] <= 4);
      REQUIRE(in[5] <= 1 && in[6] <= 1 && in[7] <= 63 && in[8] <= 7 && in[10] <= 127);
      REQUIRE((int32_t)in[9] >= -63 && (int32_t)in[9] <= 63 && in[19] <= 1 && in[20] <= 255 && in[30] <= 4096);
      for (i = 11; i < 16; ++i) REQUIRE((int32_t)in[i] >= -15 && (int32_t)in[i] <= 15);
      for (i = 16; i < 19; ++i) REQUIRE(in[i] <= 255);
      for (i = 22; i < 30; i += 2) {
        REQUIRE((int32_t)in[i] >= -127 && (int32_t)in[i] <= 127);
        REQUIRE((int32_t)in[i+1] >= -63 && (int32_t)in[i+1] <= 63);
      }
      for (i = 31; i < 31+ps; ++i) REQUIRE(in[i] < 10);
      for (at = 31+ps; at < n; at += 4) {
        unsigned x = ((at-31-ps)/4)%mw, y = ((at-31-ps)/4)/mw;
        REQUIRE(in[at] <= 1 && in[at+1] < 4 && in[at+2] <= 1 && in[at+3] < 4);
        if (in[at]) REQUIRE(in[31+(4*y+1)*(4*mw+1)+4*x+1] < 4);
      }
      count = WebpopSyntax(in, out);
    } else if (IS(VP8FixedImage)) {
      REQUIRE(n == 3 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383 && in[2] <= 3);
      REQUIRE(((in[0]+15)/16)*((in[1]+15)/16) <= 4096);
      count = WebpopFixedImage(in, out);
    } else if (IS(VP8LoopFrame) || IS(VP8MethodLoopFrame) || IS(VP8MethodLoopTrace)) {
      const unsigned offset=IS(VP8LoopFrame)?0:1;
      const uint32_t* frame=in+offset;
      unsigned pixels;
      float quality;
      REQUIRE(n >= offset+9 && frame[0] > 0 && frame[0] <= 16383 && frame[1] > 0 && frame[1] <= 16383);
      REQUIRE(frame[0]*frame[1] <= 200000 && frame[3] >= 1 && frame[3] <= 4 && frame[4] <= 65536 && frame[5] <= ((524288u-2048u)<<11));
      memcpy(&quality,frame+2,4); REQUIRE(quality >= 0 && quality <= 100);
      REQUIRE(frame[6] <= 1 && frame[7] <= 1 && frame[8] >= 4*frame[0] && frame[8] <= 4*frame[0]+64);
      pixels=frame[6]?(frame[1]-1)*frame[8]+4*frame[0]:frame[0]*frame[1]+2*((frame[0]+1)/2)*((frame[1]+1)/2);
      REQUIRE(n == offset+9+pixels);
      for(i=offset+9;i<n;++i) REQUIRE(in[i] <= 255);
      if(frame[6]) { unsigned x,y; for(y=0;y<frame[1];++y) for(x=0;x<frame[0];++x) REQUIRE(frame[9+y*frame[8]+4*x+3] == 255); }
      if(IS(VP8MethodLoopTrace)) {
        REQUIRE(in[0]<=6 && frame[7]==1);
        count=WebpopMethodLoopTrace(in,out);
      } else if(offset) {
        REQUIRE(in[0]<=6 && frame[3]==4 && frame[4]==65536 && frame[5]==((524288u-2048u)<<11) && frame[7]==0);
        count=WebpopMethodLoopFrame(in,out);
      } else count=WebpopLoopFrame(in,out);
    } else if (IS(VP8FrameSegments)) {
      REQUIRE(n>=6 && in[0]>=1 && in[0]<=4 && in[1]>=1 && in[1]<=4096 && n==5+in[1]);
      for(i=2;i<5;++i) REQUIRE(in[i]<=255);
      for(i=5;i<n;++i) REQUIRE(in[i]<4);
      count=WebpopFrameSegments(in,out);
    } else if (IS(VP8AdjustFilter)) {
      REQUIRE(n==15 && in[0]<=100 && in[1]<=7 && in[2]<=63);
      for(i=3;i<15;i+=3) REQUIRE(in[i]<=2047 && in[i+1]<=65535 && in[i+2]<=63);
      count=WebpopAdjustFilter(in,out);
    } else if (IS(VP8ModeSearch) || IS(VP8DistortionSearch) || IS(VP8TrellisSearch)) {
      uint32_t* request = in; unsigned size = n;
      if (IS(VP8DistortionSearch)) {
        REQUIRE(n == 728 && in[0]<=2 && in[1]<=1 && in[2]<=3 && in[4]<=0x7fffff && in[5]<=1 && in[6]<=1 && in[7]<=1);
        in += 8; n -= 8;
      }
      if(IS(VP8TrellisSearch)) {
        REQUIRE(n==721 && (in[0]==5 || in[0]==6));in+=1;n-=1;
        REQUIRE(in[634]<=6 && in[0]<=1 && in[4]<4);
      }
      REQUIRE(n == 720 && in[1] <= 127 && in[2] <= 3 && in[3] <= 7 && (IS(VP8TrellisSearch) || in[634] <= 4) && in[635] <= 65536 && in[636] <= 4 && in[638] <= 0x7fffff);
      REQUIRE((int32_t)in[6] >= -15 && (int32_t)in[6] <= 15 && (int32_t)in[7] >= -15 && (int32_t)in[7] <= 15);
      for (i = 8; i < 16; ++i) REQUIRE((int32_t)in[i] >= -128 && (int32_t)in[i] <= 127);
      for (i = 16; i < 634; ++i) REQUIRE(in[i] <= 255);
      for (i = 639; i < n; ++i) REQUIRE(in[i] <= 9);
      count = IS(VP8TrellisSearch) ? WebpopTrellisSearch(request,out) :
          IS(VP8DistortionSearch) ? WebpopDistortionSearch(request, out) : WebpopModeSearch(in, out);
      in = request; n = size;
    } else if (IS(VP8Flat)) {
      REQUIRE(n >= 514 && in[0] <= 240 && in[1] <= 16 && n == 514+16*in[1]);
      for (i = 2; i < 514; ++i) REQUIRE(in[i] <= 255);
      for (; i < n; ++i) REQUIRE((int32_t)in[i] >= -32768 && (int32_t)in[i] <= 32767);
      count = WebpopFlat(in, out);
    } else if (IS(VP8ModeCost)) {
      REQUIRE(n == 99 && in[0] < 16 && in[1] < 4);
      for (i = 2; i < n; ++i) REQUIRE(in[i] < 10);
      count = WebpopModeCost(in, out);
    } else if (IS(VP8ScoreOps)) {
      REQUIRE(n == 24 && in[0] < 4 && in[1] <= 1000000);
      for (i = 2; i < 24; i += 11) {
        unsigned j;
        for (j = 0; j < 10; j += 2) {
          int64_t v = (int64_t)Read64(in+i+j);
          REQUIRE(v >= -((int64_t)1<<40) && v <= ((int64_t)1<<40));
        }
      }
      count = WebpopScoreOps(in, out);
    } else if (IS(VP8TrellisScore)) {
      int64_t rate, distortion;
      REQUIRE(n==5 && in[0]<=1000000);
      rate=(int64_t)Read64(in+1); distortion=(int64_t)Read64(in+3);
      REQUIRE(rate>=-((int64_t)1<<40) && rate<=((int64_t)1<<40));
      REQUIRE(distortion>=-((int64_t)1<<40) && distortion<=((int64_t)1<<40));
      count=WebpopTrellisScore(in,out);
    } else if (IS(VP8TrellisBlock)) {
      REQUIRE(n==380 && in[0]<4 && in[1]<3 && in[2]<=1000000 && in[3]<=1);
      for(i=0;i<16;++i) {
        const int32_t v=(int32_t)in[348+i];
        REQUIRE(in[4+i]>=4 && in[4+i]<=440 && in[20+i]==131072/in[4+i]);
        REQUIRE(in[68+i]<=512);
        REQUIRE(v>=-32768 && v<=32767);
        if(i!=0 || in[0]!=0) REQUIRE((v<0?-v:v)+(int32_t)in[68+i]<=8191);
        REQUIRE((int32_t)in[364+i]>=-32768 && (int32_t)in[364+i]<=32767);
      }
      for(i=84;i<348;++i) REQUIRE(in[i]<=255);
      count=WebpopTrellisBlock(in,out);
    } else if (IS(VP8QuantizeSingle)) {
      REQUIRE(n == 2 && in[0] >= 4 && in[0] <= 132 && (int32_t)in[1] >= -32768 && (int32_t)in[1] <= 32767);
      count = WebpopQuantizeSingle(in, out);
    } else if (IS(VP8DCError)) {
      REQUIRE(n == 137 && in[0] >= 4 && in[0] <= 132);
      for (i = 1; i < 9; ++i) REQUIRE((int32_t)in[i] >= -128 && (int32_t)in[i] <= 127);
      for (i = 9; i < n; ++i) REQUIRE((int32_t)in[i] >= -32768 && (int32_t)in[i] <= 32767);
      count = WebpopDCError(in, out);
    } else if (IS(VP8Reconstruct) || IS(VP8TrellisReconstruct)) {
      uint32_t* request=in;unsigned size=n;
      if(IS(VP8TrellisReconstruct)) { REQUIRE(n==635 && (in[0]==5 || in[0]==6));in+=1;n-=1; }
      REQUIRE(n == 634 && in[0] <= 3 && in[1] <= 127 && in[2] <= 3 && in[3] <= 7 && in[5] <= 15);
      REQUIRE(in[4] < ((in[0] == 0 || in[0] == 2) ? 4u : 10u));
      REQUIRE((int32_t)in[6] >= -15 && (int32_t)in[6] <= 15 && (int32_t)in[7] >= -15 && (int32_t)in[7] <= 15);
      for (i = 8; i < 16; ++i) REQUIRE((int32_t)in[i] >= -128 && (int32_t)in[i] <= 127);
      for (i = 16; i < n; ++i) REQUIRE(in[i] <= 255);
      count = IS(VP8TrellisReconstruct)?WebpopTrellisReconstruct(request,out):WebpopReconstruct(in,out);
      in=request;n=size;
    } else if (IS(VP8AnalysisQuant)) {
      uint64_t pixels, mbw, mbh, output_size;
      float quality;
      REQUIRE(n >= 4 && in[0] >= 1 && in[0] <= 16383 && in[1] >= 1 && in[1] <= 16383 && in[2] >= 1 && in[2] <= 4);
      memcpy(&quality, in+3, sizeof(quality)); REQUIRE(quality >= 0.f && quality <= 100.f);
      pixels = (uint64_t)in[0]*in[1] + 2*(uint64_t)((in[0]+1)/2)*((in[1]+1)/2);
      mbw = (in[0]+15)/16; mbh = (in[1]+15)/16;
      output_size = 1039 + 5*mbw*mbh + (4*mbw+1)*(4*mbh+1);
      REQUIRE(n == 4 + pixels && output_size <= LIMIT);
      for (i = 4; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopAnalysisQuant(in, out);
    } else if (IS(VP8FilterStrengthFromDelta)) {
      REQUIRE(n == 2 && in[0] <= 7 && in[1] <= 0x7fffffff);
      out[0] = VP8FilterStrengthFromDelta(in[0], in[1]); count = 1;
    } else if (IS(VP8ExpandMatrix)) {
      REQUIRE(n == 3 && in[0] < 3 && in[1] >= 4 && in[1] <= 440 && in[2] >= 4 && in[2] <= 440);
      count = WebpopExpandMatrix(in, out);
    } else if (IS(VP8QuantSetup)) {
      float quality;
      REQUIRE(n == 32 && in[0] <= 4 && in[1] >= 1 && in[1] <= 4 && in[2] <= 6);
      REQUIRE(in[3] <= 100 && in[4] <= 100 && in[5] <= 7 && in[6] <= 1 && in[7] <= 7 && in[8] <= 7905);
      memcpy(&quality, in + 9, sizeof(quality)); REQUIRE(quality >= 0.f && quality <= 100.f);
      memcpy(&quality, in + 31, sizeof(quality)); REQUIRE(quality >= 0.f && quality <= 100.f);
      for (i = 10; i < 15; ++i) REQUIRE((int32_t)in[i] >= -15 && (int32_t)in[i] <= 15);
      for (i = 15; i < 31; i += 4) {
        REQUIRE((int32_t)in[i] >= -127 && (int32_t)in[i] <= 127 && in[i+1] <= 255 && in[i+2] <= 127 && in[i+3] <= 63);
      }
      count = WebpopQuantSetup(in, out);
    } else if (IS(VP8Susceptibility)) {
      REQUIRE(n == 3 && in[0] <= 256 && in[1] <= 31 && (int32_t)in[2] >= -1048576 && (int32_t)in[2] <= 1048576);
      count = WebpopSusceptibility(in, out);
    } else if (IS(VP8SegmentAlphas)) {
      unsigned lo = 255, hi = 0;
      REQUIRE(n == 6 && in[0] >= 1 && in[0] <= 4);
      for (i = 2; i < n; ++i) REQUIRE(in[i] <= 255);
      for (i = 2; i < 2 + in[0]; ++i) {if (in[i] < lo) lo = in[i]; if (in[i] > hi) hi = in[i];}
      if (hi == lo) ++hi;
      REQUIRE(in[1] >= lo && in[1] <= hi);
      count = WebpopSegmentAlphas(in, out);
    } else if (IS(VP8AssignSegments)) {
      uint64_t total = 0;
      REQUIRE(n == 257 && in[0] >= 1 && in[0] <= 4);
      for (i = 1; i < n; ++i) total += in[i];
      REQUIRE(total > 0 && total <= 1048576);
      count = WebpopAssignSegments(in, out);
    } else if (IS(VP8AnalysisProbe)) {
      REQUIRE(n == 587 && in[0] <= 3);
      for (i = 1; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopAnalysisProbe(in, out);
    } else if (IS(VP8FastAnalyze)) {
      float quality;
      REQUIRE(n == 513); memcpy(&quality, in, sizeof(quality));
      REQUIRE(quality>=0.f && quality<=100.f);
      for(i=1;i<n;++i) REQUIRE(in[i]<=255);
      count=WebpopFastAnalyze(in,out);
    } else if (IS(VP8AnalysisFrame) || IS(VP8FastAnalysisFrame)) {
      uint32_t* request = in; unsigned size = n;
      if (IS(VP8FastAnalysisFrame)) {
        float quality; REQUIRE(n>=5 && in[0]<=2); memcpy(&quality,in+1,sizeof(quality));
        REQUIRE(quality>=0.f && quality<=100.f); in+=2; n-=2;
      }
      uint64_t pixels, mbw, mbh, output_size;
      REQUIRE(n >= 3 && in[0] >= 1 && in[0] <= 16383 && in[1] >= 1 && in[1] <= 16383 && in[2] >= 1 && in[2] <= 4);
      pixels = (uint64_t)in[0]*in[1] + 2*(uint64_t)((in[0]+1)/2)*((in[1]+1)/2);
      mbw = (in[0]+15)/16; mbh = (in[1]+15)/16;
      output_size = 273 + 10*mbw*mbh + (4*mbw+1)*(4*mbh+1);
      REQUIRE(n == 3 + pixels && output_size <= LIMIT);
      for (i = 3; i < n; ++i) REQUIRE(in[i] <= 255);
      count = IS(VP8FastAnalysisFrame) ? WebpopFastAnalysisFrame(request,out) : WebpopAnalysisFrame(in, out);
      in=request; n=size;
    } else if (IS(VP8IteratorNz)) {
      REQUIRE(n == 22 && in[0] <= 1 && in[3] <= 1);
      for (i = 4; i < n; ++i) REQUIRE(in[i] <= 1);
      count = WebpopIteratorNz(in, out);
    } else if (IS(VP8IteratorCosts)) {
      REQUIRE(n == 1494 && in[2] <= 1 && in[3] < 16);
      for (i = 4; i < 22; ++i) REQUIRE(in[i] <= 1);
      for (; i < 1078; ++i) REQUIRE(in[i] <= 255);
      for (; i < n; ++i) REQUIRE((int32_t)in[i] >= -2047 && (int32_t)in[i] <= 2047);
      for (i = 1094; i < 1350; i += 16) REQUIRE(in[i] == 0);
      count = WebpopIteratorCosts(in, out);
    } else if (IS(VP8IteratorScan)) {
      uint64_t pixels, mbw, mbh, pred_size, budget;
      REQUIRE(n >= 7 && in[0] >= 1 && in[0] <= 16383 && in[1] >= 1 && in[1] <= 16383);
      REQUIRE(in[2] <= 1 && in[3] <= 1 && in[6] <= 1);
      mbw = (in[0] + 15) / 16; mbh = (in[1] + 15) / 16;
      REQUIRE(in[4] <= mbh && in[5] <= mbw * (mbh - in[4]));
      pixels = (uint64_t)in[0] * in[1] + 2 * (uint64_t)((in[0] + 1) / 2) * ((in[1] + 1) / 2);
      pred_size = (4 * mbw + 1) * (4 * mbh + 1);
      budget = 158 + (815 + (in[6] && !in[2] ? 679 : 0)) * (uint64_t)in[5] + pred_size + 5 * mbw * mbh + mbw + 1 + 32 * mbw + (in[3] ? 4 * mbw : 0);
      REQUIRE(7 + pixels + 558 * (uint64_t)in[5] == n && budget <= LIMIT);
      for (i = 7; i < 7 + pixels; ++i) REQUIRE(in[i] <= 255);
      for (; i < n; i += 558) {
        unsigned j;
        for (j = 0; j < 512; ++j) REQUIRE(in[i + j] <= 255);
        REQUIRE(in[i + 512] <= 1 && in[i + 529] <= 3 && in[i + 530] <= 1 && in[i + 531] <= 3);
        for (j = 513; j < 529; ++j) REQUIRE(in[i + j] <= 9);
        if (in[i + 512]) REQUIRE(in[i + 513] <= 3);
        for (j = 532; j < 550; ++j) REQUIRE(in[i + j] <= 1);
        for (; j < 558; ++j) REQUIRE((int32_t)in[i + j] >= -128 && (int32_t)in[i + j] <= 127);
      }
      count = WebpopIteratorScan(in, out);
    } else if (IS(YuvTables)) {
      REQUIRE(n == 0);
      count = WebpopYuvTables(out);
    } else if (IS(YuvGamma)) {
      REQUIRE(n % 2 == 0);
      for (i = 0; i < n; i += 2) REQUIRE(in[i + 1] <= 2 && in[i] <= (16380u >> in[i + 1]));
      count = WebpopYuvGamma(in, n, out);
    } else if (IS(YuvLuma) || IS(YuvUV)) {
      unsigned max = IS(YuvLuma) ? 255 : 1020;
      REQUIRE(n == 4 && in[0] <= max && in[1] <= max && in[2] <= max && in[3] <= 131072);
      count = WebpopYuvMatrix(in, IS(YuvUV), out);
    } else if (IS(YuvAccumulate)) {
      REQUIRE(n >= 3 && in[0] >= 1 && in[0] <= 16383 && in[2] <= 1);
      REQUIRE(in[1] == 0 || (in[1] >= 4 * in[0] && in[1] <= 131072));
      REQUIRE(n == 3 + in[1] + 4 * in[0]);
      for (i = 3; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopYuvAccumulate(in, n, out);
    } else if (IS(YuvaImport)) {
      uint64_t size, output_size;
      REQUIRE(n >= 4 && in[0] >= 1 && in[0] <= 16383 && in[1] >= 1 && in[1] <= 16383 && in[3] <= 1);
      REQUIRE(in[2] >= 4 * in[0] && in[2] <= 131072);
      size = (uint64_t)(in[1] - 1) * in[2] + 4 * in[0];
      output_size = 2 + 2 * (uint64_t)in[0] * in[1] + 2 * (uint64_t)((in[0] + 1) / 2) * ((in[1] + 1) / 2);
      REQUIRE(n == 4 + size && output_size <= LIMIT);
      for (i = 4; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopYuvaImport(in, n, out);
    } else if (IS(VP8DirectCoeffs)) {
      REQUIRE(n>=1058 && in[0]<=31 && in[1]<=512 && n==1058+19*in[1]);
      for(i=2;i<1058;++i) REQUIRE(in[i]<=255);
      for(i=1058;i<n;i+=19) {
        unsigned k; REQUIRE(in[i]<=1 && in[i+1]<4 && in[i+2]<3);
        for(k=0;k<16;++k) REQUIRE((int32_t)in[i+3+k]>=-2047 && (int32_t)in[i+3+k]<=2047);
        if(in[i]) REQUIRE(in[i+3]==0);
      }
      count=WebpopDirectCoeffs(in,out);
    } else if (IS(VP8SkipProba)) {
      uint64_t nb,total; REQUIRE(n==5 && in[0]<=1);
      nb=Read64(in+1);total=Read64(in+3);REQUIRE(nb<=total && total<=UINT64_MAX/255);
      if(in[0]) REQUIRE(total<=1048576);
      count=WebpopSkipProba(in,out);
    } else if (IS(VP8DirectResiduals)) {
      unsigned blocks,at;
      REQUIRE(n>=4 && in[0]>=1 && in[0]<=16 && in[1]>=1 && in[1]<=16 && in[2]<=31);
      // At most 19 boolean events per coefficient, each at most 8 bits.
      // 128 MBs plus logical traces fit the fixed LIMIT-word response buffer.
      blocks=in[0]*in[1];at=1061+in[0];REQUIRE(blocks<=128 && n==at+403*blocks);
      REQUIRE((in[3]&65535)<=(in[3]>>16));
      for(i=4;i<1060;++i) REQUIRE(in[i]<=255);
      for(;at<n;at+=403) {
        unsigned k;REQUIRE(in[at]<=2 && in[at+1]<=1 && in[at+2]<4);
        for(k=3;k<403;++k) REQUIRE((int32_t)in[at+k]>=-2047 && (int32_t)in[at+k]<=2047);
        if(in[at+1]) for(k=0;k<16;++k) REQUIRE(in[at+19+16*k]==0);
      }
      count=WebpopDirectResiduals(in,out);
    } else if (IS(VP8TokenProba)) {
      REQUIRE(n == 3 && in[0] <= in[1] && in[1] <= 65535 && in[2] <= 255);
      count = WebpopTokenProba(in, out);
    } else if (IS(VP8ProbaState)) {
      REQUIRE(n == 2114 && in[0] <= 3 && in[1] <= 1);
      for (i = 2; i < 1058; ++i) REQUIRE(in[i] <= 255);
      for (; i < n; ++i) REQUIRE((in[i] & 65535) <= (in[i] >> 16));
      count = WebpopProbaState(in, out);
    } else if (IS(VP8Tokens)) {
      uint64_t budget = 0;
      REQUIRE(n >= 2117 && in[0] <= 32768 && in[1] <= 32768 && in[2] <= 1 && in[3] <= 31);
      REQUIRE(in[4] <= 32769 && n == 2117 + 19 * in[4]);
      for (i = 5; i < 1061; ++i) REQUIRE(in[i] <= 255);
      for (; i < 2117; ++i) REQUIRE((in[i] & 65535) <= (in[i] >> 16));
      for (; i < n; i += 19) {
        unsigned j;
        int nonzero = 0;
        REQUIRE(in[i] <= 1 && in[i + 1] < 4 && in[i + 2] < 3);
        for (j = 0; j < 16; ++j) {
          REQUIRE((int32_t)in[i + 3 + j] >= -2047 && (int32_t)in[i + 3 + j] <= 2047);
          nonzero |= in[i + 3 + j] != 0;
        }
        if (in[i]) REQUIRE(in[i + 3] == 0);
        budget += nonzero ? 400 : 1;
      }
      REQUIRE(budget <= 150000);
      count = WebpopTokens(in, out);
    } else if (IS(VP8CostTables) || IS(VP8DefaultProbas)) {
      REQUIRE(n == 0);
      count = IS(VP8CostTables) ? WebpopVP8CostTables(out) : WebpopVP8DefaultProbas(out);
    } else if (IS(VP8BitCost)) {
      REQUIRE(n == 2 && in[1] <= 255);
      out[count++] = VP8BitCost((int32_t)in[0], in[1]);
    } else if (IS(VP8RecordStats)) {
      proba_t stats;
      REQUIRE(n >= 1 && n <= 131073);
      stats = in[0];
      for (i = 1; i < n; ++i) {
        REQUIRE(in[i] <= 1);
        out[count++] = VP8RecordStats(in[i], &stats);
        out[count++] = stats;
      }
    } else if (IS(VP8LevelCosts)) {
      REQUIRE(n == 1057 && in[0] <= 1);
      for (i = 1; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopVP8LevelCosts(in, out);
    } else if (IS(VP8Residual)) {
      REQUIRE(n >= 2116 && in[0] <= 1 && in[1] < 4 && in[2] < 3);
      REQUIRE(in[3] <= 32768 && n == 2116 + 16 * in[3]);
      for (i = 4; i < 1060; ++i) REQUIRE(in[i] <= 255);
      for (i = 2116; i < n; ++i) {
        REQUIRE((int32_t)in[i] >= -2047 && (int32_t)in[i] <= 2047);
        if (in[0] && (i - 2116) % 16 == 0) REQUIRE(in[i] == 0);
      }
      count = WebpopVP8Residual(in, out);
    } else if (IS(VP8WriteProbas)) {
      REQUIRE(n == 1059 && in[0] <= 1 && in[1] <= 255 && in[2] <= 31);
      for (i = 3; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopVP8WriteProbas(in, out);
    } else if (IS(VP8BitWriter)) {
      REQUIRE(WebpopBooleanWriter(in, n, out, &count));
    } else if (IS(QUANTDIV)) {
      REQUIRE(n == 3);
      out[count++] = QUANTDIV(in[0], in[1], in[2]);
    } else if (IS(BIAS)) {
      REQUIRE(n == 1);
      out[count++] = BIAS(in[0]);
    } else if (IS(VP8DspTables)) {
      REQUIRE(n == 0);
      count = WebpopVP8DspTables(out);
    } else if (IS(VP8Quantize)) {
      REQUIRE(n >= 1 && in[0] <= 2 && n == (in[0] == 1 ? 113 : 97));
      for (i = 1; i < 33; ++i) REQUIRE(in[i] <= UINT16_MAX);
      for (i = 65; i < 81; ++i) REQUIRE(in[i] <= UINT16_MAX);
      for (i = 81; i < n; ++i) REQUIRE((int32_t)in[i] >= INT16_MIN && (int32_t)in[i] <= INT16_MAX);
      count = WebpopVP8Quantize(in, out);
    } else if (IS(VP8Metric)) {
      const unsigned widths[] = {4, 8, 16, 16, 4, 16, 16, 4, 16};
      const unsigned heights[] = {4, 8, 8, 16, 4, 16, 4, 4, 8};
      unsigned weights, pixels;
      REQUIRE(n >= 1 && in[0] <= 8);
      weights = in[0] == 4 || in[0] == 5 ? 16 : 0;
      pixels = widths[in[0]] * heights[in[0]];
      REQUIRE(n == 1 + weights + pixels * (in[0] <= 5 ? 2 : 1));
      for (i = 1; i <= weights; ++i) REQUIRE(in[i] <= UINT16_MAX);
      for (; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopVP8Metric(in, out);
    } else if (IS(VP8CollectHistogram)) {
      REQUIRE(n == 514 && in[0] <= in[1] && in[1] <= 24);
      for (i = 2; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopVP8Histogram(in, 1, out);
    } else if (IS(VP8SetHistogramData)) {
      REQUIRE(n == 32);
      count = WebpopVP8Histogram(in, 0, out);
    } else if (IS(VP8EncPredLuma4) || IS(VP8EncPredLuma16) || IS(VP8EncPredChroma8)) {
      REQUIRE(n == (IS(VP8EncPredLuma4) ? 14 : IS(VP8EncPredChroma8) ? 43 : 35));
      if (!IS(VP8EncPredLuma4)) REQUIRE(in[0] <= 3);
      for (i = 0; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopLossyPredictions(name, in, out);
    } else if (IS(VP8FTransform)) {
      REQUIRE(n == 32);
      for (i = 0; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopLossyTransform(in, out);
    } else if (IS(VP8FTransform2)) {
      REQUIRE(n == 64);
      for (i = 0; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopLossyTransform2(in, out);
    } else if (IS(VP8ITransform)) {
      unsigned pixels;
      REQUIRE(n > 0 && in[0] <= 1);
      pixels = in[0] ? 32 : 16;
      REQUIRE(n == 1 + 2 * pixels);
      for (i = 1; i <= pixels; ++i) REQUIRE(in[i] <= 255);
      for (; i < n; ++i) REQUIRE((int32_t)in[i] >= -4095 && (int32_t)in[i] <= 4095);
      count = WebpopLossyInverseTransform(in, out);
    } else if (IS(VP8FTransformWHT) || IS(VP8TransformWHT)) {
      const int inverse = IS(VP8TransformWHT);
      REQUIRE(n == (inverse ? 272 : 256));
      for (i = 0; i < n; ++i) REQUIRE((int32_t)in[i] >= INT16_MIN && (int32_t)in[i] <= INT16_MAX);
      if (inverse) {
        for (i = 0; i < 16; ++i) REQUIRE((int32_t)in[i] >= -16384 && (int32_t)in[i] <= 16383);
      } else {
        for (i = 0; i < 256; i += 16) REQUIRE((int32_t)in[i] >= -2048 && (int32_t)in[i] <= 2047);
      }
      count = WebpopLossyWHT(in, inverse, out);
    } else if (IS(AlphaMethodOps)) {
      uint64_t len;
      REQUIRE(n>=10 && in[0]<=6 && in[1]>=5 && in[1]<=10);
      REQUIRE(in[2]>0 && in[2]<=16383 && in[3]>0 && in[3]<=16383);
      REQUIRE(in[4]>=in[2] && in[4]<=in[2]+64);
      len=(uint64_t)(in[3]-1)*in[4]+in[2];
      REQUIRE(n==10+len && len<=65536 && 5*(len+1)+16<=LIMIT);
      if(in[1]!=9) REQUIRE(in[4]==in[2]);
      if(in[1]==5) REQUIRE((in[5]==0 || in[5]==5 || in[5]==6) && in[6]<=6);
      if(in[1]==6) REQUIRE(in[7]<=1);
      if(in[1]==7) REQUIRE(in[5]<=1 && in[6]<4 && in[7]<=1);
      if(in[1]==8 || in[1]==10) REQUIRE(in[5]<=1 && (in[6]==0 || in[6]==5 || in[6]==6) && in[7]<=1);
      if(in[1]==9) REQUIRE(in[6]<=6 && (in[7]==0 || in[7]==5 || in[7]==6));
      for(i=10;i<n;++i) REQUIRE(in[i]<=255);
      count=WebpopAlphaMethodOps(in,n,out);
    } else if (IS(AlphaOps)) {
      uint64_t len;
      REQUIRE(n>=9 && in[0]<=9 && in[1]<=16383 && in[2]<=16383);
      if(in[0]!=3) REQUIRE(in[1]>0 && in[2]>0);
      REQUIRE(in[3]>=in[1] && in[3]<=in[1]+64);
      len=in[2]?(uint64_t)(in[2]-1)*in[3]+in[1]:0;
      REQUIRE(n==9+len && len+6<=LIMIT);
      if(in[0]==0) REQUIRE(in[8]>=in[1] && in[8]<=in[1]+64 && (uint64_t)(in[2]-1)*in[8]+in[1]+5<=LIMIT);
      if(in[0]==1) REQUIRE(in[4]<4);
      if(in[0]==3) REQUIRE(in[4]<=257 && in[5]<=1 && in[6]<=1);
      if(in[0]==2 || in[0]==3 || (in[0]>=5 && in[0]<=8)) REQUIRE(in[3]==in[1]);
      if(in[0]==6) REQUIRE((uint64_t)in[1]*in[2]<=65536);
      if(in[0]==5) REQUIRE((in[4]==0 || in[4]==5 || in[4]==6) && in[5]<=6);
      if(in[0]==7) REQUIRE(in[4]<=1 && in[5]<4 && in[6]<=1);
      if(in[0]==8 || in[0]==9) REQUIRE(in[5]<=6 && (in[6]==0 || in[6]==1 || in[6]==5 || in[6]==6));
      if(in[0]==8) REQUIRE(in[4]<=1 && (in[5]==0 || in[5]==5 || in[5]==6) && in[6]<=1);
      for(i=9;i<n;++i) REQUIRE(in[i]<=255);
      count=WebpopAlphaOps(in,n,out);
    } else if (IS(WebPEncodeRGBA) || IS(WebPEncodeRGBAWithAlphaQuality) || IS(VP8AlphaFile)) {
      const unsigned offset = IS(WebPEncodeRGBA) ? 4 : 5;
      REQUIRE(n >= offset && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383);
      REQUIRE(in[2] >= in[0] * 4 && in[2] <= INT32_MAX);
      REQUIRE((uint64_t)(in[1] - 1) * in[2] + in[0] * 4 == n - offset);
      for (i = offset; i < n; ++i) REQUIRE(in[i] <= 255);
      count = WebpopLossyEncode(in, n, IS(WebPEncodeRGBAWithAlphaQuality) || IS(VP8AlphaFile), out);
    } else if (IS(AlphaQualityWrapperErrors)) {
      REQUIRE(n == 0);
      count = WebpopAlphaWrapperErrors(out);
    } else if (IS(VP8LEncodeStream)) {
      REQUIRE(n >= 8 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383 && in[2] >= in[0]);
      REQUIRE(in[3] <= 6 && in[4] <= 100 && in[5] <= 1 && in[6] <= 97);
      REQUIRE(n == 7 + (uint64_t)(in[1] - 1) * in[2] + in[0] && n < 20000);
      count = WebpopEncodeStream(in, out);
    } else if (IS(AnalysisSizing)) {
      REQUIRE(n == 4 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383);
      REQUIRE(in[2] <= 6 && in[3] <= 1);
      count = WebpopAnalysis(name, in, out);
    } else if (IS(AnalyzeEntropy) || IS(EncoderAnalyze)) {
      unsigned offset;
      REQUIRE(n >= 6 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383 && in[2] >= in[0]);
      if (IS(AnalyzeEntropy)) {
        REQUIRE(in[3] >= 2 && in[3] <= 9 && in[4] <= 256);
        offset = 5 + in[4];
      } else {
        REQUIRE(in[3] <= 6 && in[4] <= 100);
        offset = 5;
      }
      REQUIRE(n == offset + (uint64_t)(in[1] - 1) * in[2] + in[0]);
      count = WebpopAnalysis(name, in, out);
    } else if (IS(CrossColorTransform)) {
      REQUIRE(n >= 9 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383);
      REQUIRE(in[2] >= 2 && in[2] <= 9 && in[3] <= 100 && in[4] <= 1 && in[5] <= 1 && in[6] <= 3 && in[7] <= 97);
      REQUIRE((uint64_t)in[0] * in[1] == n - 8);
      count = WebpopCrossColorTransform(in, out);
    } else if (IS(CrossColor)) {
      REQUIRE(n > 0);
      count = WebpopCrossColor(in, n, out);
    } else if (IS(ClampBits)) {
      REQUIRE(n == 6 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383);
      REQUIRE(in[3] <= in[4] && in[4] <= 9);
      out[count++] = WebpopClampBits(in);
    } else if (IS(PredictorTransform)) {
      REQUIRE(n >= 9 && in[0] > 0 && in[1] > 0 && in[0] <= 16383 && in[1] <= 16383);
      REQUIRE(in[3] <= 6 && in[4] <= 100 && in[5] <= 1 && in[6] <= 1 && in[7] <= 97);
      REQUIRE((uint64_t)in[0] * in[1] == n - 8);
      count = WebpopPredictorTransform(in, out);
    } else if (IS(NearLosslessStream)) {
      count = WebpopNearLosslessStream(in, out);
    } else if (IS(NearLosslessFeedback)) {
      count = WebpopNearLosslessFeedback(in, n, out);
    } else if (IS(PredictorFoundations)) {
      REQUIRE(n > 0);
      count = WebpopPredictor(in, n, out);
    } else if (IS(EncodeImageInternal)) {
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
    } else if (IS(NearLosslessWrapperErrors)) {
      uint8_t rgba[4] = {1, 2, 3, 255}, *encoded;
      const int strengths[] = {-1, 101};
      REQUIRE(n == 0);
      out[count++] = WebPEncodeRGBAWithNearLossless(rgba, 1, 1, 4, 0, NULL) == 0;
      for (i = 0; i < sizeof(strengths) / sizeof(strengths[0]); ++i) {
        encoded = rgba;  // The wrapper must reset the caller's output slot.
        out[count++] = WebPEncodeRGBAWithNearLossless(rgba, 1, 1, 4, strengths[i], &encoded) == 0;
        out[count++] = encoded == NULL;
      }
    } else if (IS(WebPEncodeLosslessRGBA) || IS(WebPEncodeRGBAOptions) ||
               IS(WebPEncodeRGBAWithNearLossless) || IS(WebPEncodeNearLosslessOptions)) {
      uint8_t *allocation, *rgba, *encoded = NULL;
      size_t size;
      uint64_t required;
      const unsigned offset = IS(WebPEncodeLosslessRGBA) ? 3 :
          IS(WebPEncodeRGBAWithNearLossless) ? 4 : IS(WebPEncodeNearLosslessOptions) ? 7 : 6;
      REQUIRE(n >= 3 && in[0] > 0 && in[0] <= 16383 && in[1] > 0 && in[1] <= 16383);
      REQUIRE(in[2] >= in[0] * 4 && in[2] <= INT32_MAX);
      required = (uint64_t)(in[1] - 1) * in[2] + in[0] * 4;
      REQUIRE(n >= offset && required == n - offset);
      allocation = (uint8_t*)malloc(n - offset + 1);
      REQUIRE(allocation != NULL);
      rgba = allocation + 1;  // Exercise the importer's unaligned input path.
      for (i = offset; i < n; ++i) { REQUIRE(in[i] <= 255); rgba[i - offset] = (uint8_t)in[i]; }
      if (IS(WebPEncodeLosslessRGBA)) {
        size = WebPEncodeLosslessRGBA(rgba, (int)in[0], (int)in[1], (int)in[2], &encoded);
      } else if (IS(WebPEncodeRGBAWithNearLossless)) {
        REQUIRE(in[3] <= 100);
        size = WebPEncodeRGBAWithNearLossless(rgba, (int)in[0], (int)in[1], (int)in[2], (int)in[3], &encoded);
      } else {
        WebPConfig config;
        WebPPicture pic;
        WebPMemoryWriter writer;
        REQUIRE(in[3] <= 100 && in[4] <= 6 && in[5] <= 1);
        REQUIRE(WebPConfigInit(&config) && WebPPictureInit(&pic));
        config.lossless = 1; config.quality = in[3]; config.method = in[4]; config.exact = in[5];
        if (IS(WebPEncodeNearLosslessOptions)) {
          REQUIRE(in[6] <= 100);
          config.near_lossless = in[6];
        }
        pic.use_argb = 1; pic.width = in[0]; pic.height = in[1];
        WebPMemoryWriterInit(&writer);
        pic.writer = WebPMemoryWrite; pic.custom_ptr = &writer;
        REQUIRE(WebPPictureImportRGBA(&pic, rgba, in[2]) && WebPEncode(&config, &pic));
        WebPPictureFree(&pic);
        size = writer.size; encoded = writer.mem;
      }
      REQUIRE(size > 0 && size <= LIMIT && encoded != NULL);
      for (i = offset; i < n; ++i) REQUIRE(rgba[i - offset] == in[i]);
      for (i = 0; i < size; ++i) out[count++] = encoded[i];
      WebPFree(encoded);
      free(allocation);
    } else if (WebpopNearLossless(name, in, n, out, &count)) {
      // Standalone near-lossless leaves and preprocessing passes.
    } else if (WebpopNearLosslessPredictor(name, in, n, out, &count)) {
      // Predictor quantization leaves, with reconstructed pixels.
    } else REQUIRE(0);
    printf("%u", count);
    for (i = 0; i < count; ++i) printf(" %08" PRIx32, out[i]);
    putchar('\n');
  }
  free(in);
  free(out);
  return ferror(stdin) || ferror(stdout) ? 1 : 0;
}
