// Test-only logical adapters to the normally linked pinned iterator and cost
// functions. Allocation/serialization only; no iterator algorithms are copied.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "src/dsp/dsp.h"
#include "src/enc/cost_enc.h"
#include "src/enc/vp8i_enc.h"

static unsigned Snapshot(const VP8EncIterator* it, uint32_t* out) {
  unsigned i, j, n = 0;
  out[n++] = it->x; out[n++] = it->y;
  out[n++] = (uint32_t)it->count_down; out[n++] = (uint32_t)it->count_down0;
  out[n++] = VP8IteratorIsDone(it);
  for (i = 0; i < 17; ++i) out[n++] = it->y_left[(int)i - 1];
  for (i = 0; i < 9; ++i) out[n++] = it->u_left[(int)i - 1];
  for (i = 0; i < 9; ++i) out[n++] = it->v_left[(int)i - 1];
  for (i = 0; i < 9; ++i) out[n++] = it->top_nz[i];
  for (i = 0; i < 9; ++i) out[n++] = it->left_nz[i];
  for (i = 0; i < 2; ++i) for (j = 0; j < 2; ++j) out[n++] = (uint32_t)(int32_t)it->left_derr[i][j];
  out[n++] = it->do_trellis;
  for (i = 0; i < 4; ++i) for (j = 0; j < 3; ++j) out[n++] = (uint32_t)it->bit_count[i][j];
  out[n++] = (uint32_t)it->luma_bits; out[n++] = (uint32_t)(it->luma_bits >> 32);
  out[n++] = (uint32_t)it->uv_bits; out[n++] = (uint32_t)(it->uv_bits >> 32);
  return n;
}
static unsigned Tops(const VP8EncIterator* it, uint32_t* out) {
  unsigned i;
  for (i = 0; i < 16; ++i) out[i] = it->y_top[i];
  for (i = 0; i < 16; ++i) out[16 + i] = it->uv_top[i];
  return 32;
}
static unsigned I4(const VP8EncIterator* it, uint32_t* out) {
  unsigned i;
  out[0] = it->i4; out[1] = it->i4_top - it->i4_boundary;
  for (i = 0; i < 37; ++i) out[2 + i] = it->i4_boundary[i];
  return 39;
}
unsigned WebpopIteratorScan(const uint32_t* in, uint32_t* out) {
  const unsigned w = in[0], h = in[1], source = in[2], diffusion = in[3];
  const unsigned start_row = in[4], count = in[5], trace_i4 = in[6];
  const unsigned uw = (w + 1) / 2, uh = (h + 1) / 2;
  const unsigned pixels = w * h + 2 * uw * uh;
  VP8Encoder enc = {0};
  VP8EncIterator it = {0};
  WebPPicture pic = {0};
  uint8_t* samples = (uint8_t*)malloc(pixels);
  uint32_t* nz;
  uint8_t* preds;
  uint8_t tmp[32 + WEBP_ALIGN_CST];
  uint8_t* scratch = (uint8_t*)WEBP_ALIGN(tmp);
  unsigned i, j, block, at = 7, n = 0, pred_size;
  if (samples == NULL) abort();
  for (i = 0; i < pixels; ++i) samples[i] = in[at++];
  pic.width = w; pic.height = h;
  pic.y_stride = w; pic.uv_stride = uw;
  pic.y = samples; pic.u = samples + w * h; pic.v = pic.u + uw * uh;
  enc.pic = &pic; enc.num_parts = 1;
  enc.mb_w = (w + 15) / 16; enc.mb_h = (h + 15) / 16;
  enc.preds_w = 4 * enc.mb_w + 1;
  pred_size = enc.preds_w * (4 * enc.mb_h + 1);
  // Guard the terminal SetRow pointer as well as the logical predictor map.
  preds = (uint8_t*)calloc(pred_size + enc.preds_w, 1);
  nz = (uint32_t*)calloc(enc.mb_w + 1, sizeof(*nz));
  enc.y_top = (uint8_t*)malloc(32 * enc.mb_w);
  enc.uv_top = enc.y_top + 16 * enc.mb_w;
  enc.mb_info = (VP8MBInfo*)calloc(enc.mb_w * enc.mb_h, sizeof(*enc.mb_info));
  if (!preds || !nz || !enc.y_top || !enc.mb_info) abort();
  enc.preds = preds + 1 + enc.preds_w; enc.nz = nz + 1;
  if (diffusion) {
    enc.top_derr = (DError*)malloc(enc.mb_w * sizeof(*enc.top_derr));
    if (!enc.top_derr) abort();
    memset(enc.top_derr, 0x5a, enc.mb_w * sizeof(*enc.top_derr));
  }
  for (i = 0; i < (unsigned)(enc.mb_w * enc.mb_h); ++i) enc.mb_info[i].alpha = 199;
  for (i = 0; i < 9; ++i) it.top_nz[i] = it.left_nz[i] = 3;
  for (i = 0; i < 4; ++i) for (j = 0; j < 3; ++j) it.bit_count[i][j] = 0xbeef;
  memset(it.left_derr, 7, sizeof(it.left_derr));
  it.do_trellis = 1; it.luma_bits = UINT64_C(0x12345678abcdef01); it.uv_bits = UINT64_C(0xfedcba9876543210);
  VP8IteratorInit(&enc, &it);
  memset(it.yuv_in, 0xa5, YUV_SIZE_ENC);
  n += Snapshot(&it, out + n);
  VP8IteratorSetRow(&it, start_row);
  VP8IteratorSetCountDown(&it, count);
  n += Snapshot(&it, out + n);
  for (block = 0; block < count; ++block) {
    const uint32_t* p = in + at;
    uint8_t modes[16];
    VP8IteratorImport(&it, source ? scratch : NULL);
    n += Snapshot(&it, out + n);
    for (i = 0; i < YUV_SIZE_ENC; ++i) out[n++] = it.yuv_in[i];
    n += Tops(&it, out + n);
    for (i = 0; i < YUV_SIZE_ENC; ++i) it.yuv_out[i] = p[i];
    if (trace_i4 && !source) {
      VP8IteratorStartI4(&it);
      n += I4(&it, out + n);
      for (i = 0; i < 16; ++i) {
        out[n++] = VP8IteratorRotateI4(&it, it.yuv_out);
        n += I4(&it, out + n);
      }
    }
    for (i = 0; i < 16; ++i) modes[i] = p[513 + i];
    if (p[512]) VP8SetIntra16Mode(&it, modes[0]); else VP8SetIntra4Mode(&it, modes);
    VP8SetIntraUVMode(&it, p[529]); VP8SetSkip(&it, p[530]); VP8SetSegment(&it, p[531]);
    for (i = 0; i < 9; ++i) { it.top_nz[i] = p[532 + i]; it.left_nz[i] = p[541 + i]; }
    if (diffusion) for (i = 0; i < 2; ++i) for (j = 0; j < 2; ++j) {
      it.left_derr[i][j] = (int8_t)(int32_t)p[550 + 2 * i + j];
      enc.top_derr[it.x][i][j] = (int8_t)(int32_t)p[554 + 2 * i + j];
    }
    VP8IteratorBytesToNz(&it);
    if (!source) VP8IteratorSaveBoundary(&it);
    n += Snapshot(&it, out + n);
    n += Tops(&it, out + n);
    out[n++] = it.nz[0];
    out[n++] = VP8IteratorNext(&it);
    n += Snapshot(&it, out + n);
    at += 558;
  }
  for (i = 0; i < pred_size; ++i) out[n++] = preds[i];
  for (i = 0; i < (unsigned)(enc.mb_w * enc.mb_h); ++i) {
    const VP8MBInfo* mb = enc.mb_info + i;
    out[n++] = mb->type; out[n++] = mb->uv_mode; out[n++] = mb->skip;
    out[n++] = mb->segment; out[n++] = mb->alpha;
  }
  for (i = 0; i <= (unsigned)enc.mb_w; ++i) out[n++] = nz[i];
  for (i = 0; i < 32u * enc.mb_w; ++i) out[n++] = enc.y_top[i];
  if (diffusion) for (i = 0; i < (unsigned)enc.mb_w; ++i)
    for (j = 0; j < 4; ++j) out[n++] = (uint32_t)(int32_t)((int8_t*)enc.top_derr[i])[j];
  free(enc.top_derr); free(enc.mb_info); free(enc.y_top); free(nz); free(preds); free(samples);
  return n;
}

unsigned WebpopIteratorNz(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0}; VP8EncIterator it = {0};
  uint32_t nz[2] = {in[2], in[1]};
  unsigned i, n = 0;
  enc.nz = nz + 1; it.enc = &enc; it.nz = nz + 1;
  for (i = 0; i < 9; ++i) { it.top_nz[i] = in[4 + i]; it.left_nz[i] = in[13 + i]; }
  it.left_nz[8] = in[3];
  if (!in[0]) VP8IteratorNzToBytes(&it);
  VP8IteratorBytesToNz(&it);
  for (i = 0; i < 9; ++i) out[n++] = it.top_nz[i];
  for (i = 0; i < 9; ++i) out[n++] = it.left_nz[i];
  out[n++] = nz[1]; out[n++] = nz[0];
  return n;
}

unsigned WebpopIteratorCosts(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0}; VP8EncIterator it = {0}; VP8ModeScore rd = {0};
  uint32_t nz[2] = {in[1], in[0]};
  int16_t levels[16];
  unsigned i, j, t, b, c, p, at = 22, n = 0;
  enc.nz = nz + 1; it.enc = &enc; it.nz = nz + 1;
  it.i4 = in[3];
  for (i = 0; i < 9; ++i) {it.top_nz[i] = in[4+i]; it.left_nz[i] = in[13+i];}
  it.left_nz[8] = in[2];
  for (t = 0; t < 4; ++t) for (b = 0; b < 8; ++b) for (c = 0; c < 3; ++c)
    for (p = 0; p < 11; ++p) enc.proba.coeffs[t][b][c][p] = in[at++];
  enc.proba.dirty = 1;
  VP8EncDspInit(); VP8EncDspCostInit(); VP8CalculateLevelCosts(&enc.proba);
  for (i = 0; i < 16; ++i) rd.y_dc_levels[i] = (int16_t)(int32_t)in[at++];
  for (i = 0; i < 16; ++i) for (j = 0; j < 16; ++j) rd.y_ac_levels[i][j] = (int16_t)(int32_t)in[at++];
  for (i = 0; i < 8; ++i) for (j = 0; j < 16; ++j) rd.uv_levels[i][j] = (int16_t)(int32_t)in[at++];
  for (i = 0; i < 16; ++i) levels[i] = (int16_t)(int32_t)in[at++];
  for (p = 0; p < 3; ++p) {
    out[n++] = p == 0 ? VP8GetCostLuma4(&it, levels) : p == 1 ? VP8GetCostLuma16(&it, &rd) : VP8GetCostUV(&it, &rd);
    for (i = 0; i < 9; ++i) out[n++] = it.top_nz[i];
    for (i = 0; i < 9; ++i) out[n++] = it.left_nz[i];
  }
  out[n++] = nz[0]; out[n++] = nz[1];
  return n;
}
