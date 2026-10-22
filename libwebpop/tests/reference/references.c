// Test-only access to static storage/locality helpers in pinned libwebp.
// Rename external definitions to keep public oracle encoding on normal libwebp.
#define VP8LDistanceToPlaneCode WebpopDistanceToPlaneCode
#define VP8LClearBackwardRefs WebpopClearBackwardRefs
#define VP8LBackwardRefsClear WebpopBackwardRefsClear
#define VP8LBackwardRefsInit WebpopBackwardRefsInit
#define VP8LRefsCursorInit WebpopRefsCursorInit
#define VP8LRefsCursorNextBlock WebpopRefsCursorNextBlock
#define VP8LBackwardRefsCursorAdd WebpopBackwardRefsCursorAdd
#define VP8LHashChainInit WebpopHashChainInit
#define VP8LHashChainClear WebpopHashChainClear
#define VP8LHashChainFill WebpopHashChainFill
#define VP8LGetBackwardReferences WebpopGetBackwardReferences
#include "src/enc/backward_references_enc.c"

// Request: block_size, then (operation, value, length) triples.
// 0 literal, 1 cache, 2 copy, 3 recycle, 4 clone to saved, 5 swap,
// 6 release, 7 locality(width=value), 8 inspect. Append implicit inspection.
// Inspection: count, followed by mode, length, and logical payload per token.
static unsigned InspectRefs(const VP8LBackwardRefs* refs, uint32_t* out) {
  VP8LRefsCursor c = VP8LRefsCursorInit(refs);
  unsigned count = 1;
  out[0] = 0;
  while (VP8LRefsCursorOk(&c)) {
    const PixOrCopy* p = c.cur_pos;
    out[count++] = PixOrCopyIsLiteral(p) ? 0 : PixOrCopyIsCacheIdx(p) ? 1 : 2;
    out[count++] = PixOrCopyLength(p);
    out[count++] = PixOrCopyIsLiteral(p)
        ? (PixOrCopyLiteral(p, 3) << 24) | (PixOrCopyLiteral(p, 2) << 16) |
          (PixOrCopyLiteral(p, 1) << 8) | PixOrCopyLiteral(p, 0)
        : PixOrCopyIsCacheIdx(p) ? PixOrCopyCacheIdx(p) : PixOrCopyDistance(p);
    ++out[0];
    VP8LRefsCursorNext(&c);
  }
  return count;
}

unsigned WebpopRefs(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned i, count = 0;
  VP8LBackwardRefs refs, saved;
  VP8LBackwardRefsInit(&refs, (int)in[0]);
  VP8LBackwardRefsInit(&saved, (int)in[0]);
  for (i = 1; i < n; i += 3) {
    if (count + n + 2 >= (1u << 20)) abort();
    if (in[i] == 1 && in[i + 1] >= (1u << MAX_COLOR_CACHE_BITS)) abort();
    if (in[i] == 2 && in[i + 2] > 65535) abort();
    if (in[i] == 7 && (in[i + 1] == 0 || in[i + 1] > 16383)) abort();
    switch (in[i]) {
      case 0: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateLiteral(in[i + 1])); break;
      case 1: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateCacheIdx((int)in[i + 1])); break;
      case 2: VP8LBackwardRefsCursorAdd(&refs, PixOrCopyCreateCopy(in[i + 1], (uint16_t)in[i + 2])); break;
      case 3: VP8LClearBackwardRefs(&refs); break;
      case 4: if (!BackwardRefsClone(&refs, &saved)) abort(); break;
      case 5: BackwardRefsSwap(&refs, &saved); break;
      case 6: VP8LBackwardRefsClear(&refs); break;
      case 7: BackwardReferences2DLocality((int)in[i + 1], &refs); break;
      case 8: count += InspectRefs(&refs, out + count); break;
      default: abort();
    }
    if (refs.error || saved.error) abort();
  }
  if (count + 2 * n + 2 >= (1u << 20)) abort();
  count += InspectRefs(&refs, out + count);
  count += InspectRefs(&saved, out + count);
  VP8LBackwardRefsClear(&refs);
  VP8LBackwardRefsClear(&saved);
  return count;
}

// width, height, quality, low_effort, strategy (0 hash only, 1 RLE,
// 2 standard, 3 box, 4 cache selection), cache_bits, pixels.
// Return packed chain (box chain for strategy 3), then logical tokens.
unsigned WebpopMatches(const uint32_t* in, unsigned n, uint32_t* out) {
  const int width = (int)in[0], height = (int)in[1], size = width * height;
  const int strategy = (int)in[4];
  int percent = 0, bits = (int)in[5];
  unsigned count = 0;
  VP8LHashChain chain = {0}, box = {0};
  VP8LBackwardRefs refs;
  if (width <= 0 || height <= 0 || width > 16383 || n != (unsigned)size + 6 ||
      bits < 0 || bits > 10 || in[2] > 100 || in[3] > 1 || strategy > 4) abort();
  if (!VP8LHashChainInit(&chain, size) ||
      !VP8LHashChainFill(&chain, (int)in[2], in + 6, width, height, (int)in[3],
                         NULL, 0, &percent)) abort();
  VP8LBackwardRefsInit(&refs, 256);
  if (strategy == 1) {
    if (!BackwardReferencesRle(width, height, in + 6, bits, &refs)) abort();
  } else if (strategy == 2 || strategy == 4) {
    if (!BackwardReferencesLz77(width, height, in + 6, strategy == 4 ? 0 : bits,
                                 &chain, &refs)) abort();
  } else if (strategy == 3) {
    if (!VP8LHashChainInit(&box, size) ||
        !BackwardReferencesLz77Box(width, height, in + 6, bits, &chain, &box, &refs)) abort();
  }
  for (int i = 0; i < size; ++i) out[count++] =
      strategy == 3 ? box.offset_length[i] : chain.offset_length[i];
  if (strategy == 4) {
    if (!CalculateBestCacheSize(in + 6, (int)in[2], &refs, &bits)) abort();
    out[count++] = (uint32_t)bits;
    if (bits && !BackwardRefsWithLocalCache(in + 6, bits, &refs)) abort();
  }
  count += InspectRefs(&refs, out + count);
  VP8LBackwardRefsClear(&refs);
  VP8LHashChainClear(&chain);
  VP8LHashChainClear(&box);
  return count;
}

#include "cost.c"

// width, height, quality, low_effort, lz77_types, cache_bits_max, do_no_cache, pixels
unsigned WebpopGetRefs(const uint32_t* in, unsigned n, uint32_t* out) {
  const int size = (int)n - 7;
  int percent = 0, bits = 0;
  unsigned count = 0;
  VP8LHashChain chain = {0};
  VP8LBackwardRefs refs[3];
  for (int i = 0; i < 3; ++i) VP8LBackwardRefsInit(&refs[i], 256);
  if (!VP8LHashChainInit(&chain, size) ||
      !VP8LHashChainFill(&chain, (int)in[2], in + 7, (int)in[0], (int)in[1], (int)in[3], NULL, 0, &percent) ||
      !VP8LGetBackwardReferences((int)in[0], (int)in[1], in + 7, (int)in[2], (int)in[3],
                                (int)in[4], (int)in[5], (int)in[6], &chain, refs, &bits, NULL, 0, &percent)) abort();
  out[count++] = (uint32_t)bits;
  count += InspectRefs(&refs[0], out + count);
  // Low effort only initializes the first result, even with do_no_cache set.
  if (in[6]) count += InspectRefs(&refs[1], out + count);
  for (int i = 0; i < 3; ++i) VP8LBackwardRefsClear(&refs[i]);
  VP8LHashChainClear(&chain);
  return count;
}
