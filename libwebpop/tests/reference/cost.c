// Test-only access to pinned traceback helpers; public encoder keeps libwebp's
// normal entry point. No production linkage to this wrapper.
#define VP8LBackwardReferencesTraceBackwards WebpopTraceBackwards
#include "src/enc/backward_references_cost_enc.c"

unsigned WebpopCosts(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned count = 0;
  if (n < 1) abort();
  uint32_t* costs = (uint32_t*)malloc(n * sizeof(*costs));
  if (costs == NULL) abort();
  ConvertPopulationCountTableToBitEstimates((int)n, in, costs);
  for (unsigned i = 0; i < n; ++i) out[count++] = costs[i];
  free(costs);
  return count;
}

// Takes the same input as VP8LMatches, strategy selects source generation:
// 1 RLE, 2 standard, 3 box. Outputs cost model, dist_array, packed chosen path,
// and refined tokens. This isolates the earliest differing intermediate.
unsigned WebpopTrace(const uint32_t* in, unsigned n, uint32_t* out) {
  int size = (int)(n - 6), percent = 0;
  unsigned count = 0;
  VP8LHashChain chain = {0}, box = {0};
  VP8LBackwardRefs refs, refined;
  CostModel m;
  uint16_t* dist = (uint16_t*)malloc(size * sizeof(*dist));
  uint16_t* path;
  int path_size;
  VP8LBackwardRefsInit(&refs, 256);
  VP8LBackwardRefsInit(&refined, 256);
  if (dist == NULL || !VP8LHashChainInit(&chain, size) ||
      !VP8LHashChainFill(&chain, (int)in[2], in + 6, (int)in[0], (int)in[1],
                       (int)in[3], NULL, 0, &percent)) abort();
  const VP8LHashChain* selected = &chain;
  if (in[4] == 1) {
    if (!BackwardReferencesRle((int)in[0], (int)in[1], in + 6, (int)in[5], &refs)) abort();
  } else if (in[4] == 3) {
    if (!VP8LHashChainInit(&box, size) ||
        !BackwardReferencesLz77Box((int)in[0], (int)in[1], in + 6, (int)in[5], &chain, &box, &refs)) abort();
    selected = &box;
  } else if (!BackwardReferencesLz77((int)in[0], (int)in[1], in + 6, (int)in[5], &chain, &refs)) abort();
  m.literal = (uint32_t*)malloc(VP8LHistogramNumCodes((int)in[5]) * sizeof(uint32_t));
  if (m.literal == NULL || !CostModelBuild(&m, (int)in[0], (int)in[5], &refs)) abort();
  for (int i = 0; i < VP8LHistogramNumCodes((int)in[5]); ++i) out[count++] = m.literal[i];
  for (int i = 0; i < 256; ++i) out[count++] = m.red[i];
  for (int i = 0; i < 256; ++i) out[count++] = m.blue[i];
  for (int i = 0; i < 256; ++i) out[count++] = m.alpha[i];
  for (int i = 0; i < NUM_DISTANCE_CODES; ++i) out[count++] = m.distance[i];
  if (!BackwardReferencesHashChainDistanceOnly((int)in[0], (int)in[1], in + 6, (int)in[5], selected, &refs, dist)) abort();
  for (int i = 0; i < size; ++i) out[count++] = dist[i];
  TraceBackwards(dist, size, &path, &path_size);
  out[count++] = (uint32_t)path_size;
  for (int i = 0; i < path_size; ++i) out[count++] = path[i];
  if (!BackwardReferencesHashChainFollowChosenPath(in + 6, (int)in[5], path, path_size, selected, &refined)) abort();
  count += InspectRefs(&refined, out + count);
  free(m.literal);
  free(dist);
  VP8LBackwardRefsClear(&refs);
  VP8LBackwardRefsClear(&refined);
  VP8LHashChainClear(&chain);
  VP8LHashChainClear(&box);
  return count;
}
