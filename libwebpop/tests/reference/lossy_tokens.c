// Access the pinned page layout for logical inspection; no coding algorithms
// are copied. Rename exports so the normal library remains the encoder oracle.
#define VP8TBufferInit WebpopIncludedTBufferInit
#define VP8TBufferClear WebpopIncludedTBufferClear
#define VP8EmitTokens WebpopIncludedEmitTokens
#define VP8RecordCoeffTokens WebpopIncludedRecordCoeffTokens
#define VP8EstimateTokenSize WebpopIncludedEstimateTokenSize
#include "src/enc/token_enc.c"

int WebpopFitTokenProbas(VP8EncProba*);

static uint32_t CoeffHash(const VP8EncProba* p) {
  uint32_t hash = 2166136261u;
  const uint8_t* a = (const uint8_t*)p->coeffs;
  unsigned i;
  for (i = 0; i < 1056; ++i) hash = (hash ^ a[i]) * 16777619u;
  return hash;
}
static unsigned Inspect(const VP8TBuffer* b, uint32_t* out) {
  unsigned pages = 0, count = 0, n = 4;
  const VP8Tokens* p = b->pages;
  while (p != NULL) {
    int i = b->page_size;
    const int last = p->next == NULL ? b->left : 0;
    while (i-- > last) { out[n++] = TOKEN_DATA(p)[i]; ++count; }
    ++pages;
    p = p->next;
  }
  out[0] = b->page_size; out[1] = pages; out[2] = b->left; out[3] = count;
  return n;
}
static unsigned Emit(VP8TBuffer* b, const uint8_t* probas, int final_pass,
                     unsigned prefix_bits, uint32_t* out) {
  VP8BitWriter bw;
  uint64_t size = VP8EstimateTokenSize(b, probas), pos;
  unsigned i, n = 0;
  if (!VP8BitWriterInit(&bw, 0)) abort();
  if (prefix_bits) VP8PutBits(&bw, 0x5ad, prefix_bits);
  if (!VP8EmitTokens(b, &bw, probas, final_pass)) abort();
  pos = VP8BitWriterPos(&bw);
  VP8PutBits(&bw, 0x5ad, 11);
  VP8BitWriterFinish(&bw);
  if (bw.error) abort();
  out[n++] = (uint32_t)size; out[n++] = (uint32_t)(size >> 32);
  out[n++] = (uint32_t)pos; out[n++] = (uint32_t)(pos >> 32);
  out[n++] = (uint32_t)VP8BitWriterSize(&bw);
  for (i = 0; i < VP8BitWriterSize(&bw); ++i) out[n++] = bw.buf[i];
  VP8BitWriterWipeOut(&bw);
  return n;
}
unsigned WebpopTokens(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0};
  VP8Residual res;
  VP8TBuffer tokens;
  int16_t coeffs[16] = {0};
  uint8_t initial[1056];
  unsigned i, t, b, c, v, at = 5, n, fits = 0;
  const unsigned count = in[4], period = in[1];
  for (i = 0; i < 1056; ++i) initial[i] = ((uint8_t*)enc.proba.coeffs)[i] = in[at++];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) enc.proba.stats[t][b][c][v] = in[at++];
  VP8EncDspCostInit();
  VP8TBufferInit(&tokens, in[0]);
  out[0] = count;
  n = count + 2;
  for (i = 0; i < count; ++i) {
    const unsigned first = in[at++], type = in[at++], ctx = in[at++];
    if (period && i && i % period == 0) {
      out[n++] = i; out[n++] = WebpopFitTokenProbas(&enc.proba);
      out[n++] = enc.proba.dirty; out[n++] = CoeffHash(&enc.proba);
      VP8CalculateLevelCosts(&enc.proba);
      ++fits;
    }
    for (v = 0; v < 16; ++v) coeffs[v] = (int16_t)(int32_t)in[at++];
    VP8InitResidual(first, type, &enc, &res);
    VP8SetResidualCoeffs(coeffs, &res);
    out[1 + i] = VP8RecordCoeffTokens(ctx, &res, &tokens);
    if (tokens.error) abort();
  }
  if (in[2]) {
    out[n++] = count; out[n++] = WebpopFitTokenProbas(&enc.proba);
    out[n++] = enc.proba.dirty; out[n++] = CoeffHash(&enc.proba); ++fits;
  }
  out[1 + count] = fits;
  n += Inspect(&tokens, out + n);
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) out[n++] = enc.proba.stats[t][b][c][v];
  for (i = 0; i < 1056; ++i) out[n++] = ((uint8_t*)enc.proba.coeffs)[i];
  out[n++] = enc.proba.dirty;
  n += Emit(&tokens, initial, 0, in[3], out + n);
  n += Emit(&tokens, (const uint8_t*)enc.proba.coeffs, 0, in[3], out + n);
  n += Emit(&tokens, (const uint8_t*)enc.proba.coeffs, 1, in[3], out + n);
  out[n++] = tokens.pages == NULL;
  out[n++] = (uint32_t)VP8EstimateTokenSize(&tokens, initial);
  VP8TBufferClear(&tokens); VP8TBufferClear(&tokens);
  memset(coeffs, 0, sizeof(coeffs));
  VP8InitResidual(0, 3, &enc, &res); VP8SetResidualCoeffs(coeffs, &res);
  if (VP8RecordCoeffTokens(2, &res, &tokens) != 0 || tokens.error) abort();
  n += Inspect(&tokens, out + n);
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (v = 0; v < 11; ++v) out[n++] = enc.proba.stats[t][b][c][v];
  n += Emit(&tokens, initial, 1, 0, out + n);
  VP8TBufferClear(&tokens);
  if (n > (1u << 20)) abort();
  return n;
}

// Compact frame-loop diagnostics; optional complete tokens for small subgraphs.
unsigned WebpopLoopTokenWords(const VP8TBuffer* b, uint32_t* out, int full, unsigned capacity) {
  const VP8Tokens* p = b->pages;
  unsigned count = 0;
  uint32_t hash = 2166136261u;
  if (capacity < 2) abort();
  while (p != NULL) {
    int i = b->page_size;
    const int last = p->next == NULL ? b->left : 0;
    while (i-- > last) {
      const uint32_t v = TOKEN_DATA(p)[i];
      hash = (hash ^ v) * 16777619u;
      if (full) {
        if (2 + count >= capacity) abort();
        out[2 + count] = v;
      }
      ++count;
    }
    p = p->next;
  }
  out[0] = count; out[1] = hash;
  return 2 + (full ? count : 0);
}
