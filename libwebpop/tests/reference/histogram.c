// Test-only access to static histogram helpers in the pinned source.
#define VP8LHistogramCreate WebpopHistogramCreate
#define VP8LHistogramInit WebpopHistogramInit
#define VP8LHistogramStoreRefs WebpopHistogramStoreRefs
#define VP8LFreeHistogram WebpopFreeHistogram
#define VP8LFreeHistogramSet WebpopFreeHistogramSet
#define VP8LAllocateHistogramSet WebpopAllocateHistogramSet
#define VP8LHistogramSetClear WebpopHistogramSetClear
#define VP8LAllocateHistogram WebpopAllocateHistogram
#define VP8LGetHistoImageSymbols WebpopGetHistoImageSymbols
#define VP8LBitsEntropy WebpopBitsEntropy
#define VP8LHistogramEstimateBits WebpopHistogramEstimateBits
#include "src/enc/histogram_enc.c"

extern int VP8LDistanceToPlaneCode(int, int);
extern void VP8LBackwardRefsCursorAdd(VP8LBackwardRefs*, PixOrCopy);

static unsigned Emit64(uint32_t* out, uint64_t v) {
  out[0] = (uint32_t)v;
  out[1] = (uint32_t)(v >> 32);
  return 2;
}

unsigned WebpopPopulationCost(const uint32_t* in, unsigned n, uint32_t* out) {
  uint16_t trivial;
  uint8_t used;
  unsigned count = Emit64(out, VP8LBitsEntropy(in, n));
  count += Emit64(out + count, PopulationCost(in, (int)n, &trivial, &used));
  out[count++] = trivial;
  out[count++] = used;
  return count;
}

static unsigned InspectHistogram(const VP8LHistogram* h, uint32_t* out) {
  unsigned i, count = 0;
  int length = VP8LHistogramNumCodes(h->palette_code_bits);
  out[count++] = h->palette_code_bits;
  for (i = 0; i < (unsigned)length; ++i) out[count++] = h->literal[i];
  for (i = 0; i < NUM_LITERAL_CODES; ++i) out[count++] = h->red[i];
  for (i = 0; i < NUM_LITERAL_CODES; ++i) out[count++] = h->blue[i];
  for (i = 0; i < NUM_LITERAL_CODES; ++i) out[count++] = h->alpha[i];
  for (i = 0; i < NUM_DISTANCE_CODES; ++i) out[count++] = h->distance[i];
  for (i = 0; i < 5; ++i) out[count++] = h->trivial_symbol[i];
  count += Emit64(out + count, h->bit_cost);
  for (i = 0; i < 5; ++i) count += Emit64(out + count, h->costs[i]);
  for (i = 0; i < 5; ++i) out[count++] = h->is_used[i];
  out[count++] = h->bin_id;
  count += Emit64(out + count, VP8LHistogramEstimateBits(h));
  return count;
}

// cache_bits, width (0 means unmodified distances), then op/value/length.
// 0 literal, 1 cache, 2 copy (buffered as refs), 3 store refs, 4 create,
// 5 init(value=reset arrays), 6 compute costs, 7 inspect,
// 8 set population bin(value=flattened index, length=count),
// 9 set bin_id(value), 10 copy from another histogram, 11 save histogram.
unsigned WebpopHistogram(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned i, count = 0;
  VP8LHistogram* h = VP8LAllocateHistogram((int)in[0]);
  VP8LHistogram* saved = VP8LAllocateHistogram((int)in[0]);
  VP8LBackwardRefs refs;
  if (!h || !saved) abort();
  VP8LHistogramInit(h, (int)in[0], 1);
  VP8LHistogramInit(saved, (int)in[0], 1);
  VP8LBackwardRefsInit(&refs, 256);
  for (i = 2; i < n; i += 3) {
    unsigned j;
    if (count + 3200 >= (1u << 20)) abort();
    if (in[i] == 1 && (in[0] == 0 || in[i + 1] >= (1u << in[0]) ||
                       in[i + 1] >= (1u << MAX_COLOR_CACHE_BITS))) abort();
    if (in[i] == 2 && (in[i + 1] == 0 || in[i + 1] > LZ77_WINDOW_SIZE ||
                       in[i + 2] == 0 || in[i + 2] > MAX_LENGTH)) abort();
    switch (in[i]) {
      case 0: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateLiteral(in[i + 1])); break;
      case 1: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateCacheIdx((int)in[i + 1])); break;
      case 2: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateCopy(in[i + 1], (uint16_t)in[i + 2])); break;
      case 3: VP8LHistogramStoreRefs(&refs, in[1] ? VP8LDistanceToPlaneCode : NULL, (int)in[1], h); break;
      case 4: VP8LHistogramCreate(h, &refs, in[i + 1] ? (int)in[0] : -1); break;
      case 5: VP8LHistogramInit(h, (int)in[0], (int)in[i + 1]); break;
      case 6: ComputeHistogramCost(h); break;
      case 7: count += InspectHistogram(h, out + count); break;
      case 8: {
        unsigned index = in[i + 1];
        for (j = 0; j < 5; ++j) {
          const uint32_t* pop;
          int length;
          GetPopulationInfo(h, (HistogramIndex)j, &pop, &length);
          if (index < (unsigned)length) { ((uint32_t*)pop)[index] = in[i + 2]; break; }
          index -= length;
        }
        if (j == 5) abort();
        break;
      }
      case 9: h->bin_id = (uint16_t)in[i + 1]; break;
      case 10: HistogramCopy(saved, h); break;
      case 11: HistogramCopy(h, saved); break;
      default: abort();
    }
  }
  if (count + 3200 >= (1u << 20)) abort();
  count += InspectHistogram(h, out + count);
  VP8LBackwardRefsClear(&refs);
  VP8LFreeHistogram(h);
  VP8LFreeHistogram(saved);
  return count;
}

// size, cache bits. Dirty every field; Clear restores all capacity slots.
unsigned WebpopHistogramSet(const uint32_t* in, uint32_t* out) {
  VP8LHistogramSet* set = VP8LAllocateHistogramSet((int)in[0], (int)in[1]);
  unsigned i, j, count = 0;
  if (set == NULL) abort();
  for (i = 0; i < in[0]; ++i) {
    VP8LHistogram* h = set->histograms[i];
    VP8LHistogramInit(h, (int)in[1], 1);
    for (j = 0; j < 5; ++j) {
      const uint32_t* pop;
      int length, k;
      GetPopulationInfo(h, (HistogramIndex)j, &pop, &length);
      for (k = 0; k < length; ++k) ((uint32_t*)pop)[k] = k + 1;
    }
    ComputeHistogramCost(h);
    h->bin_id = 77;
  }
  set->size = 1;
  VP8LHistogramSetClear(set);
  out[count++] = set->size;
  out[count++] = set->max_size;
  for (i = 0; i < in[0]; ++i) count += InspectHistogram(set->histograms[i], out + count);
  VP8LFreeHistogramSet(set);
  return count;
}
