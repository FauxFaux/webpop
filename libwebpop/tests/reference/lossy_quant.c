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
