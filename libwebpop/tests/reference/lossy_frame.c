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

// Direct coding leaves, calling the original included static helpers above.
static unsigned DirectWriter(const VP8BitWriter* bw, unsigned result, uint32_t* out) {
  uint64_t pos = VP8BitWriterPos(bw); unsigned i; uint32_t hash = 2166136261u;
  for (i=0;i<VP8BitWriterSize(bw);++i) hash=(hash^VP8BitWriterBuf(bw)[i])*16777619u;
  out[0]=result; out[1]=bw->range; out[2]=bw->value; out[3]=bw->run; out[4]=bw->nb_bits;
  out[5]=(uint32_t)pos; out[6]=(uint32_t)(pos>>32); out[7]=VP8BitWriterSize(bw); out[8]=hash;
  return 9;
}
static unsigned DirectFinish(VP8BitWriter* bw, uint32_t* out) {
  unsigned i,n=0;
  VP8BitWriterFinish(bw); if (bw->error) abort();
  n+=DirectWriter(bw,0,out+n); out[n++]=VP8BitWriterSize(bw);
  for(i=0;i<VP8BitWriterSize(bw);++i) out[n++]=VP8BitWriterBuf(bw)[i];
  VP8BitWriterWipeOut(bw); return n;
}
unsigned WebpopDirectCoeffs(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc={0}; VP8BitWriter bw; VP8Residual res;
  unsigned i,n=0,at=2; int result;
  VP8EncDspInit(); VP8EncDspCostInit();
  for(i=0;i<1056;++i) ((uint8_t*)enc.proba.coeffs)[i]=in[at++];
  if(!VP8BitWriterInit(&bw,0)) abort();
  if(in[0]) VP8PutBits(&bw,0x5ad,in[0]);
  n+=DirectWriter(&bw,0,out+n);
  for(i=0;i<in[1];++i) {
    unsigned k; int16_t coeffs[16]; const uint32_t* b=in+at; at+=19;
    for(k=0;k<16;++k) coeffs[k]=(int16_t)b[3+k];
    VP8InitResidual(b[0],b[1],&enc,&res); VP8SetResidualCoeffs(coeffs,&res);
    result=PutCoeffs(&bw,b[2],&res);
    out[n++]=(uint32_t)res.last; n+=DirectWriter(&bw,result,out+n);
  }
  return n+DirectFinish(&bw,out+n);
}
unsigned WebpopSkipProba(const uint32_t* in, uint32_t* out) {
  uint64_t nb=(uint64_t)in[1]|((uint64_t)in[2]<<32);
  uint64_t total=(uint64_t)in[3]|((uint64_t)in[4]<<32);
  if(!in[0]) {out[0]=CalcSkipProba(nb,total);return 1;}
  VP8Encoder enc={0}; enc.mb_w=(int)total; enc.mb_h=1;
  enc.proba.nb_skip=(int)nb; enc.proba.skip_proba=17; enc.proba.use_skip_proba=1;
  out[0]=FinalizeSkipProba(&enc);out[1]=enc.proba.skip_proba;
  out[2]=enc.proba.use_skip_proba;out[3]=enc.proba.nb_skip;return 4;
}
unsigned WebpopDirectResiduals(const uint32_t* in, uint32_t* out) {
  VP8Encoder enc={0}; VP8EncIterator it={0}; VP8BitWriter bw;
  const unsigned w=in[0],h=in[1]; unsigned i,j,k,at=4,n=0;
  uint32_t* nz=(uint32_t*)calloc(w+1,sizeof(*nz));
  uint8_t* tops=(uint8_t*)calloc(32*w,1);
  uint8_t* preds=(uint8_t*)calloc((4*w+1)*(4*h+1)+4*w+2,1);
  VP8MBInfo* mbs=(VP8MBInfo*)calloc(w*h,sizeof(*mbs));
  if(!nz || !tops || !preds || !mbs) abort();
  enc.mb_w=w;enc.mb_h=h;enc.num_parts=1;enc.preds_w=4*w+1;
  enc.nz=nz+1;enc.y_top=tops;enc.uv_top=tops+16*w;enc.preds=preds+enc.preds_w+1;enc.mb_info=mbs;
  VP8EncDspInit();VP8EncDspCostInit();
  for(i=0;i<1056;++i) {((uint8_t*)enc.proba.coeffs)[i]=in[at++];((proba_t*)enc.proba.stats)[i]=in[3];}
  VP8IteratorInit(&enc,&it);
  for(i=0;i<w+1;++i) nz[i]=in[at++];
  for(i=0;i<9;++i) {it.top_nz[i]=i%2;it.left_nz[i]=(i+1)%2;}
  it.luma_bits=(1ULL<<34)+17;it.uv_bits=(1ULL<<35)+19;
  for(i=0;i<4;++i) for(j=0;j<3;++j) it.bit_count[i][j]=(1ULL<<40)+3*i+j;
  if(!VP8BitWriterInit(&bw,0)) abort(); if(in[2]) VP8PutBits(&bw,0x5ad,in[2]);
  n+=DirectWriter(&bw,0,out+n);
  do {
    VP8ModeScore rd={0}; const unsigned op=in[at++];
    it.mb->type=in[at++];it.mb->segment=in[at++];
    for(i=0;i<16;++i) rd.y_dc_levels[i]=(int16_t)in[at++];
    for(i=0;i<16;++i) for(j=0;j<16;++j) rd.y_ac_levels[i][j]=(int16_t)in[at++];
    for(i=0;i<8;++i) for(j=0;j<16;++j) rd.uv_levels[i][j]=(int16_t)in[at++];
    if(op==0) RecordResiduals(&it,&rd);
    else if(op==1) CodeResiduals(&bw,&it,&rd);
    else ResetAfterSkip(&it);
    n+=DirectWriter(&bw,0,out+n);
    out[n++]=(uint32_t)it.luma_bits;out[n++]=(uint32_t)(it.luma_bits>>32);
    out[n++]=(uint32_t)it.uv_bits;out[n++]=(uint32_t)(it.uv_bits>>32);
    for(i=0;i<4;++i) for(j=0;j<3;++j) {uint64_t v=it.bit_count[i][j];out[n++]=(uint32_t)v;out[n++]=(uint32_t)(v>>32);}
    for(i=0;i<9;++i) out[n++]=it.top_nz[i];for(i=0;i<9;++i) out[n++]=it.left_nz[i];
    for(i=0;i<w+1;++i) out[n++]=nz[i];
    k=VP8IteratorNext(&it);out[n++]=it.x;out[n++]=it.y;out[n++]=it.left_nz[8];
  } while(k);
  for(i=0;i<1056;++i) out[n++]=((proba_t*)enc.proba.stats)[i];
  n+=DirectFinish(&bw,out+n);
  free(mbs);free(preds);free(tops);free(nz);return n;
}
