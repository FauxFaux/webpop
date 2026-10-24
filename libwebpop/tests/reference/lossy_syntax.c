// Test-only access to pinned syntax/mode helpers. Public encoder entry points
// remain those of the normally linked library. No coding algorithms are copied.
#define VP8EncWrite WebpopIncludedEncWrite
#define VP8EncFreeBitWriters WebpopIncludedFreeBitWriters
#include "src/enc/syntax_enc.c"
#undef VP8EncWrite
#define VP8CoeffsProba0 WebpopSyntaxCoeffsProba0
#define VP8CoeffsUpdateProba WebpopSyntaxCoeffsUpdateProba
#define VP8DefaultProbas WebpopSyntaxDefaultProbas
#define VP8CodeIntraModes WebpopSyntaxCodeIntraModes
#define VP8WriteProbas WebpopSyntaxWriteProbas
#include "src/enc/tree_enc.c"
#include <stdlib.h>
#include <string.h>
#include "src/enc/cost_enc.h"

static unsigned Finish(VP8BitWriter* bw, uint32_t* out) {
  unsigned n = 0, i;
  const uint64_t pos = VP8BitWriterPos(bw);
  out[n++] = (uint32_t)pos; out[n++] = (uint32_t)(pos >> 32);
  VP8PutBits(bw, 0x5ad, 11);
  VP8BitWriterFinish(bw);
  if (bw->error) abort();
  for (i = 0; i < VP8BitWriterSize(bw); ++i) out[n++] = bw->buf[i];
  VP8BitWriterWipeOut(bw);
  return n;
}
unsigned WebpopModeSyntax(const uint32_t* in, uint32_t* out) {
  VP8BitWriter bw;
  uint8_t p[3] = {in[4], in[5], in[6]};
  if (!VP8BitWriterInit(&bw, 0)) abort();
  switch (in[0]) {
    case 0: PutI4Mode(&bw, in[1], kBModesProba[in[2]][in[3]]); break;
    case 1: PutI16Mode(&bw, in[1]); break;
    case 2: PutUVMode(&bw, in[1]); break;
    case 3: PutSegment(&bw, in[1], p); break;
    default: abort();
  }
  return Finish(&bw, out);
}

static void Init(VP8Encoder* enc, WebPPicture* pic, unsigned w, unsigned h) {
  unsigned pred_size;
  uint8_t* preds;
  uint32_t* nz;
  memset(enc, 0, sizeof(*enc));
  if (!WebPPictureInit(pic)) abort();
  pic->width = w; pic->height = h;
  pic->writer = WebPMemoryWrite;
  enc->pic = pic; enc->num_parts = 1;
  enc->mb_w = (w + 15) / 16; enc->mb_h = (h + 15) / 16;
  enc->preds_w = 4 * enc->mb_w + 1;
  pred_size = enc->preds_w * (4 * enc->mb_h + 2);
  preds = (uint8_t*)calloc(pred_size, 1);
  nz = (uint32_t*)calloc(enc->mb_w + 1, sizeof(uint32_t));
  enc->y_top = (uint8_t*)calloc(32 * enc->mb_w, 1);
  enc->mb_info = (VP8MBInfo*)calloc(enc->mb_w * enc->mb_h, sizeof(VP8MBInfo));
  if (!preds || !nz || !enc->y_top || !enc->mb_info) abort();
  enc->preds = preds + 1 + enc->preds_w; enc->nz = nz + 1;
  enc->uv_top = enc->y_top + 16 * enc->mb_w;
  VP8EncDspInit(); VP8EncDspCostInit();
  VP8DefaultProbas(enc);
}
static void Clear(VP8Encoder* enc) {
  free(enc->preds - 1 - enc->preds_w); free(enc->nz - 1);
  free(enc->y_top); free(enc->mb_info);
  WebpopIncludedFreeBitWriters(enc);
}
static unsigned File(VP8Encoder* enc, uint32_t* out) {
  WebPMemoryWriter writer;
  unsigned i, n;
  WebPMemoryWriterInit(&writer); enc->pic->custom_ptr = &writer;
  if (!WebpopIncludedEncWrite(enc)) abort();
  n = writer.size;
  for (i = 0; i < n; ++i) out[i] = writer.mem[i];
  WebPMemoryWriterClear(&writer);
  return n;
}
unsigned WebpopSyntax(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc;
  WebPPicture pic;
  unsigned i, s, t, b, c, p, at = 31, n = 0;
  const unsigned op = in[0];
  Init(&enc, &pic, in[1], in[2]);
  enc.profile = in[3];
  enc.segment_hdr.num_segments = in[4]; enc.segment_hdr.update_map = in[5];
  enc.filter_hdr.simple = in[6]; enc.filter_hdr.level = in[7];
  enc.filter_hdr.sharpness = in[8]; enc.filter_hdr.i4x4_lf_delta = (int32_t)in[9];
  enc.base_quant = in[10]; enc.dq_y1_dc = (int32_t)in[11]; enc.dq_y2_dc = (int32_t)in[12];
  enc.dq_y2_ac = (int32_t)in[13]; enc.dq_uv_dc = (int32_t)in[14]; enc.dq_uv_ac = (int32_t)in[15];
  for (i = 0; i < 3; ++i) enc.proba.segments[i] = in[16+i];
  enc.proba.use_skip_proba = in[19]; enc.proba.skip_proba = in[20];
  s = 0;
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c) for (p = 0; p < 11; ++p, ++s) {
    if (in[21]) enc.proba.coeffs[t][b][c][p] = in[21] == 1 ? 0 : in[21] == 2 ? 255 : (s * 37 + in[21]) & 255;
  }
  for (i = 0; i < 4; ++i) { enc.dqm[i].quant = (int32_t)in[22+2*i]; enc.dqm[i].fstrength = (int32_t)in[23+2*i]; }
  for (i = 0; i < (unsigned)enc.preds_w * (4*enc.mb_h+1); ++i) enc.preds[(int)i - 1 - enc.preds_w] = in[at++];
  for (i = 0; i < (unsigned)enc.mb_w * enc.mb_h; ++i) {
    enc.mb_info[i].type = in[at++]; enc.mb_info[i].uv_mode = in[at++];
    enc.mb_info[i].skip = in[at++]; enc.mb_info[i].segment = in[at++];
  }
  if (op < 4) {
    if (!VP8BitWriterInit(&enc.bw, 0)) abort();
    if (op == 0) PutSegmentHeader(&enc.bw, &enc);
    else if (op == 1) PutFilterHeader(&enc.bw, &enc.filter_hdr);
    else if (op == 2) PutQuant(&enc.bw, &enc);
    else VP8CodeIntraModes(&enc);
    n = Finish(&enc.bw, out);
  } else if (op == 4) {
    if (!GeneratePartition0(&enc)) abort();
    n = VP8BitWriterSize(&enc.bw);
    for (i = 0; i < n; ++i) out[i] = enc.bw.buf[i];
  } else {
    uint8_t* bytes = (uint8_t*)malloc(in[30] + 1);
    if (!bytes) abort();
    for (i = 0; i < in[30]; ++i) bytes[i] = (i * 73 + 19) & 255;
    if (!VP8BitWriterInit(&enc.parts[0], 0) || !VP8BitWriterAppend(&enc.parts[0], bytes, in[30])) abort();
    free(bytes);
    n = File(&enc, out);
  }
  Clear(&enc);
  return n;
}
unsigned WebpopFixedImage(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc;
  WebPPicture pic;
  VP8TBuffer tokens;
  VP8Residual res;
  int16_t zeros[16] = {0};
  unsigned i, j, n;
  Init(&enc, &pic, in[0], in[1]);
  enc.profile = in[2]; enc.segment_hdr.num_segments = 1;
  enc.segment_hdr.update_map = 0; enc.filter_hdr.simple = 1;
  VP8TBufferInit(&tokens, 0);
  for (i = 0; i < (unsigned)enc.mb_w * enc.mb_h; ++i) {
    enc.mb_info[i].type = 1; enc.mb_info[i].skip = 1;
    VP8InitResidual(0, 1, &enc, &res); VP8SetResidualCoeffs(zeros, &res);
    VP8RecordCoeffTokens(0, &res, &tokens);
    VP8InitResidual(1, 0, &enc, &res); VP8SetResidualCoeffs(zeros, &res);
    for (j = 0; j < 16; ++j) VP8RecordCoeffTokens(0, &res, &tokens);
    VP8InitResidual(0, 2, &enc, &res); VP8SetResidualCoeffs(zeros, &res);
    for (j = 0; j < 8; ++j) VP8RecordCoeffTokens(0, &res, &tokens);
  }
  if (!VP8BitWriterInit(&enc.parts[0], 0)) abort();
  VP8EmitTokens(&tokens, &enc.parts[0], (const uint8_t*)enc.proba.coeffs, 1);
  VP8BitWriterFinish(&enc.parts[0]);
  n = File(&enc, out);
  VP8TBufferClear(&tokens); Clear(&enc);
  return n;
}
