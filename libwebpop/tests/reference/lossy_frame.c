// Expose pinned static fitting and reset helpers without copying algorithms.
#define VP8Cat3 WebpopIncludedCat3
#define VP8Cat4 WebpopIncludedCat4
#define VP8Cat5 WebpopIncludedCat5
#define VP8Cat6 WebpopIncludedCat6
#define VP8EncLoop WebpopIncludedEncLoop
#define VP8EncTokenLoop WebpopIncludedEncTokenLoop
#include "src/enc/frame_enc.c"

int WebpopFitTokenProbas(VP8EncProba* p) { return FinalizeTokenProbas(p); }
unsigned WebpopTokenProba(const uint32_t* in, uint32_t* out) {
  out[0] = CalcTokenProba(in[0], in[1]);
  out[1] = BranchCost(in[0], in[1], in[2]);
  return 2;
}
unsigned WebpopProbaState(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0};
  VP8EncProba* p = &enc.proba;
  unsigned t, b, c, v, at = 2, n = 0;
  uint32_t hash = 2166136261u;
  int size = 0;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) p->coeffs[t][b][c][v] = in[at++];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) p->stats[t][b][c][v] = in[at++];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 68; ++v) p->level_cost[t][b][c][v] = 1234;
  p->dirty = in[1]; p->nb_skip = 12345; p->skip_proba = 17;
  p->use_skip_proba = 1; p->segments[0] = 3; p->segments[1] = 5; p->segments[2] = 7;
  if (in[0] <= 1) { size = FinalizeTokenProbas(p); if (in[0]) VP8CalculateLevelCosts(p); }
  else if (in[0] == 2) ResetTokenStats(&enc);
  else ResetStats(&enc);
  out[n++] = size; out[n++] = p->dirty; out[n++] = p->nb_skip;
  out[n++] = p->skip_proba; out[n++] = p->use_skip_proba;
  for (v = 0; v < 3; ++v) out[n++] = p->segments[v];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) out[n++] = p->coeffs[t][b][c][v];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) out[n++] = p->stats[t][b][c][v];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 68; ++v) hash = (hash ^ p->level_cost[t][b][c][v]) * 16777619u;
  out[n++] = hash;
  return n;
}

// Test-only fixed-probability raster composition uses the original context
// recorder; probability refresh and the production frame loop remain separate.
void WebpopRecordModeResiduals(VP8EncIterator* it, const VP8ModeScore* rd) {
  RecordResiduals(it, rd);
}
