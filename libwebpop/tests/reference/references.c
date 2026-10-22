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
