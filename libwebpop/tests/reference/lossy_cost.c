// Test-only adapters to the pinned cost/probability functions and tables.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "src/dsp/dsp.h"
#include "src/enc/cost_enc.h"
#include "src/enc/vp8i_enc.h"

unsigned WebpopVP8CostTables(uint32_t* out) {
  unsigned n = 0, i, t, b, c, p;
  for (i = 0; i < 256; ++i) out[n++] = VP8EntropyCost[i];
  for (i = 0; i < 2048; ++i) out[n++] = VP8LevelFixedCosts[i];
  for (i = 0; i < 17; ++i) out[n++] = VP8EncBands[i];
  for (i = 0; i < 4; ++i) out[n++] = VP8FixedCostsUV[i];
  for (i = 0; i < 4; ++i) out[n++] = VP8FixedCostsI16[i];
  for (i = 0; i < 10; ++i) for (b = 0; b < 10; ++b) for (c = 0; c < 10; ++c)
    out[n++] = VP8FixedCostsI4[i][b][c];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) out[n++] = VP8CoeffsProba0[t][b][c][p];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) out[n++] = VP8CoeffsUpdateProba[t][b][c][p];
  return n;
}

static void ReadProbas(const uint32_t* in, VP8EncProba* proba) {
  unsigned t, b, c, p, i = 0;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) proba->coeffs[t][b][c][p] = in[i++];
}

unsigned WebpopVP8LevelCosts(const uint32_t* in, uint32_t* out) {
  VP8EncProba proba = {0};
  unsigned t, b, c, v, n = 0;
  ReadProbas(in + 1, &proba);
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 68; ++v) proba.level_cost[t][b][c][v] = 1234;
  proba.dirty = in[0];
  VP8CalculateLevelCosts(&proba);
  out[n++] = proba.dirty;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 68; ++v) out[n++] = proba.level_cost[t][b][c][v];
  if (in[0]) {
    for (t = 0; t < 4; ++t) for (b = 0; b < 16; ++b) for (c = 0; c < 3; ++c) {
      for (v = 0; v < 8; ++v) if (proba.remapped_costs[t][b][c] == proba.level_cost[t][v][c]) break;
      if (v == 8) abort();
      out[n++] = v;
    }
  }
  return n;
}

unsigned WebpopVP8Residual(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0};
  VP8Residual res;
  int16_t coeffs[16];
  unsigned i, t, b, c, p, at = 4, n = 0;
  const unsigned first = in[0], type = in[1], ctx = in[2], count = in[3];
  ReadProbas(in + at, &enc.proba);
  at += 1056;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) enc.proba.stats[t][b][c][p] = in[at++];
  enc.proba.dirty = 1;
  VP8CalculateLevelCosts(&enc.proba);
  VP8EncDspCostInit();
  VP8InitResidual(first, type, &enc, &res);
  for (i = 0; i < count; ++i) {
    unsigned j;
    for (j = 0; j < 16; ++j) coeffs[j] = (int16_t)(int32_t)in[at++];
    VP8SetResidualCoeffs(coeffs, &res);
    out[n++] = (uint32_t)res.last;
    out[n++] = VP8GetResidualCost(ctx, &res);
    out[n++] = VP8RecordCoeffs(ctx, &res);
  }
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) out[n++] = enc.proba.stats[t][b][c][p];
  return n;
}

unsigned WebpopVP8WriteProbas(const uint32_t* in, uint32_t* out) {
  VP8EncProba proba = {0};
  VP8BitWriter bw;
  unsigned i, n = 0;
  ReadProbas(in + 3, &proba);
  proba.use_skip_proba = in[0];
  proba.skip_proba = in[1];
  if (!VP8BitWriterInit(&bw, 0)) abort();
  if (in[2]) VP8PutBits(&bw, 0x5ad, in[2]);
  VP8WriteProbas(&bw, &proba);
  out[n++] = (uint32_t)VP8BitWriterPos(&bw);
  VP8PutBits(&bw, 0x5ad, 11);
  VP8BitWriterFinish(&bw);
  if (bw.error) abort();
  out[n++] = (uint32_t)VP8BitWriterSize(&bw);
  for (i = 0; i < VP8BitWriterSize(&bw); ++i) out[n++] = bw.buf[i];
  VP8BitWriterWipeOut(&bw);
  return n;
}

unsigned WebpopVP8DefaultProbas(uint32_t* out) {
  VP8Encoder enc = {0};
  unsigned n = 0, t, b, c, p;
  enc.proba.skip_proba = 73;
  enc.proba.nb_skip = 12345;
  enc.proba.use_skip_proba = 1;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c) {
    for (p = 0; p < 11; ++p) enc.proba.stats[t][b][c][p] = 0xfffefffd;
    for (p = 0; p < 68; ++p) enc.proba.level_cost[t][b][c][p] = 1234;
  }
  VP8DefaultProbas(&enc);
  out[n++] = enc.proba.dirty;
  out[n++] = enc.proba.use_skip_proba;
  out[n++] = enc.proba.skip_proba;
  out[n++] = enc.proba.nb_skip;
  for (t = 0; t < 3; ++t) out[n++] = enc.proba.segments[t];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) out[n++] = enc.proba.coeffs[t][b][c][p];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) out[n++] = enc.proba.stats[t][b][c][p];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 68; ++p) out[n++] = enc.proba.level_cost[t][b][c][p];
  return n;
}
