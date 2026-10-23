// Access pinned static helpers; normal VP8SetSegmentParams still comes from
// the linked library. No quantization algorithms are copied into this wrapper.
#define VP8SetSegmentParams WebpopIncludedSetSegmentParams
#define VP8MakeLuma16Preds WebpopQuantMakeLuma16Preds
#define VP8MakeChroma8Preds WebpopQuantMakeChroma8Preds
#define VP8Decimate WebpopQuantDecimate
#define VP8I16ModeOffsets WebpopQuantI16ModeOffsets
#define VP8UVModeOffsets WebpopQuantUVModeOffsets
#define VP8Scan WebpopQuantScan
#include "src/enc/quant_enc.c"
#undef VP8SetSegmentParams
void VP8SetSegmentParams(VP8Encoder*, float);

static unsigned Matrix(const VP8Matrix* m, uint32_t* out) {
  unsigned i, n = 0;
  for (i = 0; i < 16; ++i) out[n++] = m->q[i];
  for (i = 0; i < 16; ++i) out[n++] = m->iq[i];
  for (i = 0; i < 16; ++i) out[n++] = m->bias[i];
  for (i = 0; i < 16; ++i) out[n++] = m->zthresh[i];
  for (i = 0; i < 16; ++i) out[n++] = m->sharpen[i];
  return n;
}
static void SeedMatrix(VP8Matrix* m, unsigned seed) {
  unsigned i;
  for (i = 0; i < 16; ++i) {
    m->q[i] = 4 + seed + i; m->iq[i] = 500 + seed + i;
    m->bias[i] = 70000 + seed + i; m->zthresh[i] = 200 + seed + i;
    m->sharpen[i] = 40 + seed + i;
  }
}
unsigned WebpopExpandMatrix(const uint32_t* in, uint32_t* out) {
  VP8Matrix m;
  SeedMatrix(&m, 9); m.q[0] = in[1]; m.q[1] = in[2];
  out[0] = ExpandMatrix(&m, in[0]);
  return 1 + Matrix(&m, out + 1);
}
unsigned WebpopQuantState(const VP8Encoder*, uint32_t*);
unsigned WebpopQuantSetup(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc;
  WebPConfig config;
  VP8MBInfo mbs[8];
  unsigned i;
  float quality, second_quality;
  memset(&enc, 0, sizeof(enc));
  if (!WebPConfigInit(&config)) abort();
  enc.config = &config; enc.mb_w = 8; enc.mb_h = 1; enc.mb_info = mbs;
  enc.segment_hdr.num_segments = in[1];
  enc.segment_hdr.update_map = 1; enc.segment_hdr.size = 37;
  enc.method = config.method = in[2]; config.sns_strength = in[3];
  config.filter_strength = in[4]; config.filter_sharpness = in[5]; config.filter_type = in[6];
  enc.filter_hdr.simple = 1; enc.filter_hdr.level = 19;
  enc.filter_hdr.sharpness = in[7]; enc.filter_hdr.i4x4_lf_delta = -3;
  enc.uv_alpha = in[8]; enc.base_quant = 91;
  memcpy(&quality, in + 9, sizeof(quality));
  memcpy(&second_quality, in + 31, sizeof(second_quality));
  enc.dq_y1_dc = (int32_t)in[10]; enc.dq_y2_dc = (int32_t)in[11]; enc.dq_y2_ac = (int32_t)in[12];
  enc.dq_uv_dc = (int32_t)in[13]; enc.dq_uv_ac = (int32_t)in[14];
  for (i = 0; i < 4; ++i) {
    VP8SegmentInfo* s = enc.dqm + i;
    SeedMatrix(&s->y1, 30*i); SeedMatrix(&s->y2, 30*i+10); SeedMatrix(&s->uv, 30*i+20);
    s->alpha = (int32_t)in[15+4*i]; s->beta = in[16+4*i];
    s->quant = in[17+4*i]; s->fstrength = in[18+4*i];
    s->max_edge = 101+i; s->min_disto = 201+i;
    s->lambda_i16 = 301+i; s->lambda_i4 = 401+i; s->lambda_uv = 501+i;
    s->lambda_mode = 601+i; s->tlambda = 701+i;
    s->lambda_trellis_i16 = 801+i; s->lambda_trellis_i4 = 901+i; s->lambda_trellis_uv = 1001+i;
    s->i4_penalty = ((int64_t)1<<35) + i;
  }
  for (i = 0; i < 8; ++i) {
    mbs[i].type = i%2; mbs[i].uv_mode = i%4; mbs[i].skip = (i/2)%2;
    mbs[i].segment = i%in[1]; mbs[i].alpha = 200+i;
  }
  switch (in[0]) {
    case 0: SetupMatrices(&enc); break;
    case 1: SetupFilterStrength(&enc); break;
    case 2: SimplifySegments(&enc); break;
    case 3: VP8SetSegmentParams(&enc, quality); break;
    case 4: VP8SetSegmentParams(&enc, quality); VP8SetSegmentParams(&enc, second_quality); break;
    default: abort();
  }
  return WebpopQuantState(&enc, out);
}

unsigned WebpopQuantState(const VP8Encoder* enc, uint32_t* out) {
  unsigned i, n = 0;
  out[n++] = enc->segment_hdr.num_segments; out[n++] = enc->segment_hdr.update_map; out[n++] = enc->segment_hdr.size;
  out[n++] = enc->base_quant; out[n++] = enc->dq_y1_dc; out[n++] = enc->dq_y2_dc; out[n++] = enc->dq_y2_ac;
  out[n++] = enc->dq_uv_dc; out[n++] = enc->dq_uv_ac;
  out[n++] = enc->filter_hdr.simple; out[n++] = enc->filter_hdr.level; out[n++] = enc->filter_hdr.sharpness;
  out[n++] = enc->filter_hdr.i4x4_lf_delta;
  for (i = 0; i < 4; ++i) {
    const VP8SegmentInfo* s = enc->dqm + i;
    n += Matrix(&s->y1, out+n); n += Matrix(&s->y2, out+n); n += Matrix(&s->uv, out+n);
    out[n++] = s->alpha; out[n++] = s->beta; out[n++] = s->quant; out[n++] = s->fstrength;
    out[n++] = s->max_edge; out[n++] = s->min_disto;
    out[n++] = s->lambda_i16; out[n++] = s->lambda_i4; out[n++] = s->lambda_uv;
    out[n++] = s->lambda_mode; out[n++] = s->tlambda;
    out[n++] = s->lambda_trellis_i16; out[n++] = s->lambda_trellis_i4; out[n++] = s->lambda_trellis_uv;
    out[n++] = (uint32_t)s->i4_penalty; out[n++] = (uint32_t)(s->i4_penalty >> 32);
  }
  for (i = 0; i < (unsigned)(enc->mb_w * enc->mb_h); ++i) {
    out[n++] = enc->mb_info[i].type; out[n++] = enc->mb_info[i].uv_mode; out[n++] = enc->mb_info[i].skip;
    out[n++] = enc->mb_info[i].segment; out[n++] = enc->mb_info[i].alpha;
  }
  return n;
}

// Scalar reconstruction adapters: call original static helpers above.
static void SeedScore(VP8ModeScore* rd) {
  unsigned i, j;
  memset(rd, 0, sizeof(*rd));
  rd->D = -123; rd->SD = 456; rd->H = 789; rd->R = -987;
  rd->score = ((int64_t)1 << 40) + 3; rd->nz = 0x13579bdf;
  rd->mode_i16 = 3; rd->mode_uv = 2;
  for (i = 0; i < 16; ++i) {
    rd->y_dc_levels[i] = -1234; rd->modes_i4[i] = i % 10;
    for (j = 0; j < 16; ++j) rd->y_ac_levels[i][j] = 234;
  }
  for (i = 0; i < 8; ++i) for (j = 0; j < 16; ++j) rd->uv_levels[i][j] = -345;
  for (i = 0; i < 2; ++i) for (j = 0; j < 3; ++j) rd->derr[i][j] = -7 + 3*i+j;
}
unsigned WebpopModeScore(const VP8ModeScore* rd, uint32_t* out) {
  unsigned i, j, n = 0;
  const int64_t scores[] = {rd->D, rd->SD, rd->H, rd->R, rd->score};
  for (i = 0; i < 5; ++i) {out[n++] = (uint32_t)scores[i]; out[n++] = (uint32_t)(scores[i] >> 32);}
  for (i = 0; i < 16; ++i) out[n++] = (uint32_t)(int32_t)rd->y_dc_levels[i];
  for (i = 0; i < 16; ++i) for (j = 0; j < 16; ++j) out[n++] = (uint32_t)(int32_t)rd->y_ac_levels[i][j];
  for (i = 0; i < 8; ++i) for (j = 0; j < 16; ++j) out[n++] = (uint32_t)(int32_t)rd->uv_levels[i][j];
  out[n++] = rd->mode_i16;
  for (i = 0; i < 16; ++i) out[n++] = rd->modes_i4[i];
  out[n++] = rd->mode_uv; out[n++] = rd->nz;
  for (i = 0; i < 2; ++i) for (j = 0; j < 3; ++j) out[n++] = (uint32_t)(int32_t)rd->derr[i][j];
  return n;
}
static unsigned Errors(const VP8EncIterator* it, const DError top[2], uint32_t* out) {
  unsigned x, ch, k, n = 0;
  for (x = 0; x < 2; ++x) for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) out[n++] = (uint32_t)(int32_t)top[x][ch][k];
  for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) out[n++] = (uint32_t)(int32_t)it->left_derr[ch][k];
  return n;
}
unsigned WebpopQuantizeSingle(const uint32_t* in, uint32_t* out) {
  VP8Matrix m = {0}; int16_t v = (int16_t)in[1];
  m.q[0] = in[0]; m.q[1] = 4; ExpandMatrix(&m, 2);
  out[1] = (uint32_t)QuantizeSingle(&v, &m); out[0] = (uint32_t)(int32_t)v;
  return 2;
}
unsigned WebpopDCError(const uint32_t* in, uint32_t* out) {
  VP8EncIterator it = {0}; VP8ModeScore rd; VP8Matrix m = {0};
  DError top[2] = {{{11,12},{13,14}},{{0}}}; int16_t tmp[8][16];
  unsigned i, ch, k, at = 1, n = 0;
  it.x = 1; it.top_derr = top;
  for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) top[1][ch][k] = (int8_t)in[at++];
  for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) it.left_derr[ch][k] = (int8_t)in[at++];
  for (i = 0; i < 128; ++i) tmp[i/16][i%16] = (int16_t)in[at++];
  m.q[0] = in[0]; m.q[1] = 4; ExpandMatrix(&m, 2); SeedScore(&rd);
  CorrectDCValues(&it, &m, tmp, &rd);
  for (i = 0; i < 128; ++i) out[n++] = (uint32_t)(int32_t)tmp[i/16][i%16];
  n += WebpopModeScore(&rd, out+n);
  n += Errors(&it, top, out+n);
  StoreDiffusionErrors(&it, &rd);
  return n + Errors(&it, top, out+n);
}
unsigned WebpopReconstruct(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0}; WebPConfig config; VP8EncIterator it = {0}; VP8ModeScore rd;
  VP8MBInfo mbs[4] = {{0}}; uint8_t preds[99] = {0}, tops[64], dst[YUV_SIZE_ENC];
  uint32_t nz[3] = {0x123456,0xabcdef,0x765432}; DError top[2];
  unsigned i, ch, k, at = 16, n = 0; int mask = 0;
  if (!WebPConfigInit(&config)) abort();
  enc.config = &config; enc.method = 4; enc.num_parts = 1;
  enc.mb_w = enc.mb_h = 2; enc.preds_w = 9;
  enc.mb_info = mbs; enc.preds = preds+10; enc.nz = nz+1;
  enc.y_top = tops; enc.uv_top = tops+32;
  enc.segment_hdr.num_segments = 4;
  enc.dq_uv_dc = (int32_t)in[6]; enc.dq_uv_ac = (int32_t)in[7];
  enc.top_derr = (in[3]&4) ? top : NULL;
  VP8EncDspInit(); VP8IteratorInit(&enc, &it);
  VP8IteratorSetRow(&it, (in[3]>>1)&1);
  if (in[3]&1) VP8IteratorNext(&it);
  for (i = 0; i < 4; ++i) {mbs[i].segment = in[2]; mbs[i].alpha = 199; enc.dqm[i].quant = (in[1]+17*i)%128;}
  SetupMatrices(&enc);
  for (i = 0; i < 4; ++i) enc.dqm[i].max_edge = 1+i;
  for (i = 0; i < 17; ++i) it.y_left[(int)i-1] = in[at++];
  for (i = 0; i < 25; ++i) it.u_left[(int)i-1] = in[at++];
  for (i = 0; i < 64; ++i) tops[i] = in[at++];
  for (i = 0; i < YUV_SIZE_ENC; ++i) it.yuv_in[i] = in[at++];
  for (i = 0; i < 3; ++i) nz[i] = (uint32_t[]){0x123456,0xabcdef,0x765432}[i];
  for (i = 0; i < 9; ++i) {it.top_nz[i] = 2+i; it.left_nz[i] = 3+i;}
  for (i = 0; i < 2; ++i) for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) top[i][ch][k] = 11+4*i+2*ch+k;
  for (ch = 0; ch < 2; ++ch) for (k = 0; k < 2; ++k) {
    top[it.x][ch][k] = (int8_t)in[8+2*ch+k]; it.left_derr[ch][k] = (int8_t)in[12+2*ch+k];
  }
  memset(it.yuv_p, 0xa5, PRED_SIZE_ENC); memset(dst, 0xa5, sizeof(dst)); SeedScore(&rd);
  if (in[0] == 0) {
    VP8MakeLuma16Preds(&it); mask = ReconstructIntra16(&it, &rd, dst, in[4]);
    StoreMaxDelta(enc.dqm+in[2], rd.y_dc_levels);
  } else if (in[0] == 2) {
    VP8MakeChroma8Preds(&it); mask = ReconstructUV(&it, &rd, dst+U_OFF_ENC, in[4]);
  } else {
    VP8IteratorStartI4(&it);
    if (in[0] == 1) {
      for (i = 0; i < in[5]; ++i) VP8IteratorRotateI4(&it, dst);
      MakeIntra4Preds(&it);
      mask = ReconstructIntra4(&it, rd.y_ac_levels[it.i4], it.yuv_in+VP8Scan[it.i4], dst+VP8Scan[it.i4], in[4]);
    } else {
      for (i = 0; i < 16; ++i) {
        MakeIntra4Preds(&it);
        mask |= ReconstructIntra4(&it, rd.y_ac_levels[i], it.yuv_in+VP8Scan[i], dst+VP8Scan[i], (in[4]+i)%10) << i;
        VP8IteratorRotateI4(&it, dst);
      }
    }
  }
  out[n++] = mask; n += WebpopModeScore(&rd, out+n);
  for (i = 0; i < YUV_SIZE_ENC; ++i) out[n++] = dst[i];
  for (i = 0; i < PRED_SIZE_ENC; ++i) out[n++] = it.yuv_p[i];
  n += Errors(&it, top, out+n);
  if (in[0] == 2 && (in[3]&4)) StoreDiffusionErrors(&it, &rd);
  n += Errors(&it, top, out+n);
  for (i = 0; i < 9; ++i) out[n++] = it.top_nz[i];
  for (i = 0; i < 9; ++i) out[n++] = it.left_nz[i];
  out[n++] = it.i4; out[n++] = it.i4_top ? (uint32_t)(it.i4_top-it.i4_boundary) : 0;
  for (i = 0; i < 37; ++i) out[n++] = it.i4_boundary[i];
  for (i = 0; i < 4; ++i) {
    out[n++] = mbs[i].type; out[n++] = mbs[i].uv_mode; out[n++] = mbs[i].skip; out[n++] = mbs[i].segment; out[n++] = mbs[i].alpha;
  }
  for (i = 0; i < 81; ++i) out[n++] = preds[i];
  for (i = 0; i < 3; ++i) out[n++] = nz[i];
  for (i = 0; i < 4; ++i) out[n++] = enc.dqm[i].max_edge;
  return n;
}
