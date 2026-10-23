// Call original static analysis helpers without copying their algorithms.
// Rename the entry point so complete checks can still call the normal library.
#include "src/enc/cost_enc.h"
#define VP8EncAnalyze WebpopIncludedEncAnalyze
#include "src/enc/analysis_enc.c"
#undef VP8EncAnalyze
int VP8EncAnalyze(VP8Encoder*);

typedef struct {
  VP8Encoder enc;
  WebPConfig config;
  WebPPicture pic;
  uint8_t* preds;
  uint8_t* planes;
  uint32_t* nz;
  unsigned pred_size;
} AnalysisContext;

static void SeedSegments(VP8Encoder* enc) {
  int i;
  for (i = 0; i < NUM_MB_SEGMENTS; ++i) {
    enc->dqm[i].alpha = -60 + i;
    enc->dqm[i].beta = 70 + i;
  }
}
static void SeedMB(VP8MBInfo* mb) {
  mb->type = 0; mb->uv_mode = 3; mb->skip = 1; mb->segment = 3; mb->alpha = 199;
}
static void InitContext(AnalysisContext* ctx, unsigned w, unsigned h, unsigned nb) {
  VP8Encoder* enc = &ctx->enc;
  unsigned i, x, y, uw = (w + 1) / 2, uh = (h + 1) / 2;
  memset(ctx, 0, sizeof(*ctx));
  if (!WebPConfigInit(&ctx->config) || !WebPPictureInit(&ctx->pic)) abort();
  ctx->config.segments = nb;
  ctx->pic.width = w; ctx->pic.height = h;
  ctx->pic.y_stride = w; ctx->pic.uv_stride = uw;
  enc->pic = &ctx->pic; enc->config = &ctx->config;
  enc->method = 4; enc->num_parts = 1;
  enc->mb_w = (w + 15) / 16; enc->mb_h = (h + 15) / 16;
  enc->preds_w = 4 * enc->mb_w + 1;
  ctx->pred_size = enc->preds_w * (4 * enc->mb_h + 1);
  ctx->preds = (uint8_t*)calloc(ctx->pred_size + enc->preds_w, 1);
  ctx->planes = (uint8_t*)calloc(w * h + 2 * uw * uh, 1);
  ctx->nz = (uint32_t*)calloc(enc->mb_w + 1, sizeof(*ctx->nz));
  enc->mb_info = (VP8MBInfo*)calloc(enc->mb_w * enc->mb_h, sizeof(*enc->mb_info));
  enc->y_top = (uint8_t*)calloc(32 * enc->mb_w, 1);
  if (!ctx->preds || !ctx->planes || !ctx->nz || !enc->mb_info || !enc->y_top) abort();
  enc->preds = ctx->preds + 1 + enc->preds_w;
  enc->nz = ctx->nz + 1; enc->uv_top = enc->y_top + 16 * enc->mb_w;
  ctx->pic.y = ctx->planes; ctx->pic.u = ctx->planes + w * h; ctx->pic.v = ctx->pic.u + uw * uh;
  enc->segment_hdr.num_segments = nb; enc->segment_hdr.update_map = nb > 1; enc->segment_hdr.size = 37;
  enc->alpha = -13; enc->uv_alpha = 27;
  SeedSegments(enc);
  for (i = 0; i < (unsigned)(enc->mb_w * enc->mb_h); ++i) SeedMB(enc->mb_info + i);
  for (y = 0; y < 4u * enc->mb_h; ++y) for (x = 0; x < 4u * enc->mb_w; ++x)
    enc->preds[y * enc->preds_w + x] = 7;
  VP8EncDspInit();
}
static void ClearContext(AnalysisContext* ctx) {
  free(ctx->enc.mb_info); free(ctx->enc.y_top); free(ctx->nz); free(ctx->planes); free(ctx->preds);
}
static unsigned Segments(const VP8Encoder* enc, uint32_t* out) {
  unsigned i, n = 0;
  for (i = 0; i < NUM_MB_SEGMENTS; ++i) {
    out[n++] = (uint32_t)enc->dqm[i].alpha; out[n++] = (uint32_t)enc->dqm[i].beta;
  }
  return n;
}
static unsigned MBs(const VP8Encoder* enc, uint32_t* out) {
  unsigned i, n = 0;
  for (i = 0; i < (unsigned)(enc->mb_w * enc->mb_h); ++i) {
    const VP8MBInfo* mb = enc->mb_info + i;
    out[n++] = mb->type; out[n++] = mb->uv_mode; out[n++] = mb->skip;
    out[n++] = mb->segment; out[n++] = mb->alpha;
  }
  return n;
}
static unsigned Stats(const int alphas[256], int alpha, int uv_alpha, uint32_t* out) {
  unsigned i;
  for (i = 0; i < 256; ++i) out[i] = alphas[i];
  out[256] = alpha; out[257] = 0; out[258] = uv_alpha; out[259] = 0;
  return 260;
}
unsigned WebpopSusceptibility(const uint32_t* in, uint32_t* out) {
  VP8Histogram h;
  h.max_value = in[0]; h.last_non_zero = in[1];
  out[0] = GetAlpha(&h); out[1] = FinalAlphaValue((int32_t)in[2]);
  return 2;
}
unsigned WebpopSegmentAlphas(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0}; int centers[4]; unsigned i;
  enc.segment_hdr.num_segments = in[0];
  SeedSegments(&enc);
  for (i = 0; i < 4; ++i) centers[i] = in[2+i];
  SetSegmentAlphas(&enc, centers, in[1]);
  return Segments(&enc, out);
}
unsigned WebpopAssignSegments(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc = {0}; VP8MBInfo mb[256];
  int alphas[256]; unsigned i, n;
  enc.segment_hdr.num_segments = in[0];
  enc.mb_info = mb; enc.mb_h = 1;
  SeedSegments(&enc);
  for (i = 0; i < 256; ++i) {
    alphas[i] = in[1+i];
    if (alphas[i]) {SeedMB(mb + enc.mb_w); mb[enc.mb_w++].alpha = i;}
  }
  // Only default preprocessing is used. This private helper needs no picture.
  WebPConfig config;
  if (!WebPConfigInit(&config)) abort();
  enc.config = &config;
  AssignSegments(&enc, alphas);
  n = Segments(&enc, out);
  return n + MBs(&enc, out + n);
}
unsigned WebpopAnalysisProbe(const uint32_t* in, uint32_t* out) {
  AnalysisContext ctx;
  VP8EncIterator it = {0}; int alphas[256] = {0}, alpha = 0, uv_alpha = 0;
  unsigned i, at = 1, n = 0;
  InitContext(&ctx, 32, 32, 4);
  VP8IteratorInit(&ctx.enc, &it);
  VP8IteratorSetRow(&it, (in[0] >> 1) & 1);
  if (in[0] & 1) VP8IteratorNext(&it);
  for (i = 0; i < 17; ++i) it.y_left[(int)i - 1] = in[at++];
  for (i = 0; i < 25; ++i) it.u_left[(int)i - 1] = in[at++];
  for (i = 0; i < 16; ++i) it.y_top[i] = in[at++];
  for (i = 0; i < 16; ++i) it.uv_top[i] = in[at++];
  for (i = 0; i < YUV_SIZE_ENC; ++i) it.yuv_in[i] = in[at++];
  memset(it.yuv_p, 0xa5, PRED_SIZE_ENC);
  out[n++] = MBAnalyzeBestIntra16Mode(&it);
  out[n++] = MBAnalyzeBestUVMode(&it);
  MBAnalyze(&it, alphas, &alpha, &uv_alpha);
  n += Stats(alphas, alpha, uv_alpha, out + n);
  n += MBs(&ctx.enc, out + n);
  for (i = 0; i < PRED_SIZE_ENC; ++i) out[n++] = it.yuv_p[i];
  for (i = 0; i < ctx.pred_size; ++i) out[n++] = ctx.preds[i];
  ClearContext(&ctx);
  return n;
}
unsigned WebpopAnalysisFrame(const uint32_t* in, uint32_t* out) {
  AnalysisContext ctx; SegmentJob job;
  unsigned i, n = 0;
  const unsigned pixels = in[0]*in[1] + 2*((in[0]+1)/2)*((in[1]+1)/2);
  InitContext(&ctx, in[0], in[1], in[2]);
  for (i = 0; i < pixels; ++i) ctx.planes[i] = in[3+i];
  if (in[2] > 1) {
    InitSegmentJob(&ctx.enc, &job, 0, ctx.enc.mb_h);
    if (!DoSegmentsJob(&job, &job.it)) abort();
    n += Stats(job.alphas, job.alpha, job.uv_alpha, out + n);
    WebPGetWorkerInterface()->End(&job.worker);
  } else {
    int zero[256] = {0}; n += Stats(zero, 0, 0, out + n);
  }
  n += MBs(&ctx.enc, out + n);
  if (!VP8EncAnalyze(&ctx.enc)) abort();
  out[n++] = (uint32_t)ctx.enc.alpha; out[n++] = (uint32_t)ctx.enc.uv_alpha;
  out[n++] = ctx.enc.segment_hdr.num_segments;
  out[n++] = ctx.enc.segment_hdr.update_map; out[n++] = ctx.enc.segment_hdr.size;
  n += Segments(&ctx.enc, out + n);
  n += MBs(&ctx.enc, out + n);
  for (i = 0; i < ctx.pred_size; ++i) out[n++] = ctx.preds[i];
  ClearContext(&ctx);
  return n;
}

// Compose the normally linked analysis and setup entry points on image planes.
unsigned WebpopQuantState(const VP8Encoder*, uint32_t*);
unsigned WebpopAnalysisQuant(const uint32_t* in, uint32_t* out) {
  AnalysisContext ctx;
  unsigned i, n;
  float quality;
  const unsigned pixels = in[0]*in[1] + 2*((in[0]+1)/2)*((in[1]+1)/2);
  InitContext(&ctx, in[0], in[1], in[2]);
  ctx.enc.filter_hdr.simple = 1;
  memcpy(&quality, in+3, sizeof(quality));
  for (i = 0; i < pixels; ++i) ctx.planes[i] = in[4+i];
  if (!VP8EncAnalyze(&ctx.enc)) abort();
  VP8SetSegmentParams(&ctx.enc, quality);
  n = WebpopQuantState(&ctx.enc, out);
  out[n++] = ctx.enc.alpha; out[n++] = ctx.enc.uv_alpha;
  for (i = 0; i < ctx.pred_size; ++i) out[n++] = ctx.preds[i];
  ClearContext(&ctx);
  return n;
}

unsigned WebpopModeScore(const VP8ModeScore*, uint32_t*);
void WebpopRecordModeResiduals(VP8EncIterator*, const VP8ModeScore*);
unsigned WebpopSearchFrame(const uint32_t* in, uint32_t* out) {
  AnalysisContext ctx; VP8EncIterator it = {0}; unsigned i, ch, k, n = 0;
  float quality; const unsigned pixels = in[0]*in[1] + 2*((in[0]+1)/2)*((in[1]+1)/2);
  memcpy(&quality, in+3, sizeof(quality)); InitContext(&ctx,in[0],in[1],in[2]);
  ctx.enc.filter_hdr.simple = 1; ctx.enc.max_i4_header_bits = in[4];
  if (quality <= 98.f) {ctx.enc.top_derr = (DError*)calloc(ctx.enc.mb_w,sizeof(DError)); if (!ctx.enc.top_derr) abort();}
  for (i = 0; i < pixels; ++i) ctx.planes[i] = in[5+i];
  VP8EncDspCostInit(); VP8DefaultProbas(&ctx.enc);
  if (!VP8EncAnalyze(&ctx.enc)) abort(); VP8SetSegmentParams(&ctx.enc,quality); VP8CalculateLevelCosts(&ctx.enc.proba);
  // Analysis seeded arbitrary interior modes. The coding pass starts with DC
  // boundary modes; its interior is overwritten as each macroblock is selected.
  memset(ctx.preds,0,ctx.pred_size);
  VP8IteratorInit(&ctx.enc,&it); memset(it.yuv_p,0xa5,PRED_SIZE_ENC);
  memset(it.yuv_out,0x5a,YUV_SIZE_ENC); memset(it.yuv_out2,0xc3,YUV_SIZE_ENC);
  out[n++] = ctx.enc.segment_hdr.num_segments; out[n++] = ctx.enc.base_quant;
  for (i = 0; i < 4; ++i) out[n++] = ctx.enc.dqm[i].quant;
  do {
    VP8ModeScore rd = {0}, logical = {0};
    VP8IteratorImport(&it,NULL); out[n++] = VP8Decimate(&it,&rd,RD_OPT_BASIC);
    // The exported score serializer receives only defined fields.
    logical.D=rd.D; logical.SD=rd.SD; logical.R=rd.R; logical.H=rd.H; logical.score=rd.score; logical.nz=rd.nz;
    logical.mode_i16=rd.mode_i16; logical.mode_uv=rd.mode_uv;
    memcpy(logical.y_dc_levels,rd.y_dc_levels,sizeof(logical.y_dc_levels));
    memcpy(logical.y_ac_levels,rd.y_ac_levels,sizeof(logical.y_ac_levels));
    memcpy(logical.uv_levels,rd.uv_levels,sizeof(logical.uv_levels));
    if (it.mb->type==0) memcpy(logical.modes_i4,rd.modes_i4,sizeof(logical.modes_i4));
    if (ctx.enc.top_derr) memcpy(logical.derr,rd.derr,sizeof(logical.derr));
    n += WebpopModeScore(&logical,out+n);
    for (i=0;i<YUV_SIZE_ENC;++i) out[n++]=it.yuv_out[i];
    out[n++]=it.mb->type;out[n++]=it.mb->uv_mode;out[n++]=it.mb->skip;out[n++]=it.mb->segment;out[n++]=it.mb->alpha;
    WebpopRecordModeResiduals(&it,&rd);
    for (i=0;i<9;++i) out[n++]=it.top_nz[i]; for (i=0;i<9;++i) out[n++]=it.left_nz[i]; out[n++]=*it.nz;
    for (ch=0;ch<2;++ch) for(k=0;k<2;++k) out[n++]=(uint32_t)(int32_t)it.left_derr[ch][k];
    for (ch=0;ch<2;++ch) for(k=0;k<2;++k) out[n++]=ctx.enc.top_derr ? (uint32_t)(int32_t)ctx.enc.top_derr[it.x][ch][k]:0;
    VP8IteratorSaveBoundary(&it);
    for(i=0;i<17;++i) out[n++]=it.y_left[(int)i-1];
    for(i=0;i<9;++i) out[n++]=it.u_left[(int)i-1]; for(i=0;i<9;++i) out[n++]=it.v_left[(int)i-1];
    for(i=0;i<16;++i) out[n++]=it.y_top[i]; for(i=0;i<16;++i) out[n++]=it.uv_top[i];
  } while(VP8IteratorNext(&it));
  for(i=0;i<1056;++i) out[n++]=((proba_t*)ctx.enc.proba.stats)[i];
  for(i=0;i<ctx.pred_size;++i) out[n++]=ctx.preds[i];
  for(i=0;i<(unsigned)ctx.enc.mb_w+1;++i) out[n++]=ctx.nz[i];
  if(ctx.enc.top_derr) for(i=0;i<(unsigned)ctx.enc.mb_w;++i) for(ch=0;ch<2;++ch) for(k=0;k<2;++k) out[n++]=(uint32_t)(int32_t)ctx.enc.top_derr[i][ch][k];
  for(i=0;i<4;++i) out[n++]=ctx.enc.dqm[i].max_edge;
  free(ctx.enc.top_derr); ClearContext(&ctx); return n;
}
