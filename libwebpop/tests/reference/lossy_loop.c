// Observe original pinned frame coding; no frame-loop algorithms are copied.
#include <stdlib.h>
#include <string.h>
#include "src/enc/vp8i_enc.h"
#include "src/enc/cost_enc.h"
int WebpopLoopSetError(const WebPPicture*, WebPEncodingError);
int WebpopLoopProgress(const WebPPicture*, int, int*);

#define WebPGetEncoderVersion WebpopLoopEncoderVersion
#define WebPEncode WebpopLoopEncode
#define WebPEncodingSetError WebpopLoopSetError
#define WebPReportProgress WebpopLoopProgress
#define GetPSNR WebpopLoopGetPSNR
#include "src/enc/webp_enc.c"
#undef GetPSNR
#undef WebPReportProgress
#undef WebPEncodingSetError
#undef WebPEncode

unsigned WebpopLoopTokenWords(const VP8TBuffer*, uint32_t*, int, unsigned);
static uint32_t* trace_out;
static unsigned trace_n, loop_pass, trace_enabled;
static uint64_t header_sum, distortion_sum;
static uint64_t header_limit;
static int imported, initialized;
static VP8ModeScore last_score;
static void ObserveInit(VP8Encoder*, VP8EncIterator*);
static void ObserveImport(VP8EncIterator*, uint8_t*);
static void ObserveCosts(VP8EncProba*);
static int ObserveDecimate(VP8EncIterator*, VP8ModeScore*, VP8RDLevel);
static void ObserveSave(VP8EncIterator*);
static int ObserveEmit(VP8TBuffer*, VP8BitWriter*, const uint8_t*, int);
#define VP8Cat3 WebpopLoopCat3
#define VP8Cat4 WebpopLoopCat4
#define VP8Cat5 WebpopLoopCat5
#define VP8Cat6 WebpopLoopCat6
#define VP8EncLoop WebpopObservedEncLoop
#define VP8EncTokenLoop WebpopObservedTokenLoop
#define VP8IteratorInit ObserveInit
#define VP8IteratorImport ObserveImport
#define VP8CalculateLevelCosts ObserveCosts
#define VP8Decimate ObserveDecimate
#define VP8IteratorSaveBoundary ObserveSave
#define VP8EmitTokens ObserveEmit
#include "src/enc/frame_enc.c"
#undef VP8IteratorInit
#undef VP8IteratorImport
#undef VP8CalculateLevelCosts
#undef VP8Decimate
#undef VP8IteratorSaveBoundary
#undef VP8EmitTokens

static void Word(uint32_t v) {
  if (trace_n >= (1u << 20)) abort();
  trace_out[trace_n++] = v;
}
static void Wide(uint64_t v) { Word((uint32_t)v); Word((uint32_t)(v>>32)); }
static void State(const VP8Encoder* enc) {
  unsigned t,b,c,p,i; uint32_t hash = 2166136261u;
  Word(enc->segment_hdr.num_segments); Word(enc->segment_hdr.update_map); Word(enc->segment_hdr.size);
  Word(enc->max_i4_header_bits); Word(enc->base_quant);
  Word(enc->dq_y1_dc); Word(enc->dq_y2_dc); Word(enc->dq_y2_ac); Word(enc->dq_uv_dc); Word(enc->dq_uv_ac);
  Word(enc->filter_hdr.simple); Word(enc->filter_hdr.level); Word(enc->filter_hdr.sharpness); Word(enc->filter_hdr.i4x4_lf_delta);
  Word(enc->proba.dirty); Word(enc->proba.use_skip_proba); Word(enc->proba.skip_proba); Word(enc->proba.nb_skip);
  for (i=0;i<3;++i) Word(enc->proba.segments[i]);
  for (i=0;i<4;++i) { Word(enc->dqm[i].quant); Word(enc->dqm[i].fstrength); Word(enc->dqm[i].max_edge); Word(enc->dqm[i].y2.q[1]); }
  for (t=0;t<4;++t) for(b=0;b<8;++b) for(c=0;c<3;++c) for(p=0;p<11;++p) Word(enc->proba.coeffs[t][b][c][p]);
  for (t=0;t<4;++t) for(b=0;b<8;++b) for(c=0;c<3;++c) for(p=0;p<11;++p) Word(enc->proba.stats[t][b][c][p]);
  for (t=0;t<4;++t) for(b=0;b<8;++b) for(c=0;c<3;++c) for(p=0;p<68;++p) hash=(hash^enc->proba.level_cost[t][b][c][p])*16777619u;
  Word(hash);
}
static void ObserveInit(VP8Encoder* enc, VP8EncIterator* it) {
  VP8IteratorInit(enc,it);
  if(initialized) ++loop_pass;
  initialized=1; imported=0; header_sum=distortion_sum=0;
}
static void ObserveImport(VP8EncIterator* it, uint8_t* tmp) {
  VP8IteratorImport(it,tmp); imported=1;
  if(trace_enabled && it->x==0 && it->y==0) { Word(0); Word(loop_pass); State(it->enc); }
}
static VP8Encoder* observed_enc;
static VP8EncIterator* observed_it;
static void ObserveCosts(VP8EncProba* p) {
  VP8CalculateLevelCosts(p);
  if(trace_enabled && imported) {
    Word(1); Word(loop_pass); Word(observed_it->x+observed_it->y*observed_enc->mb_w); State(observed_enc);
  }
}
static int ObserveDecimate(VP8EncIterator* it, VP8ModeScore* rd, VP8RDLevel level) {
  int result=VP8Decimate(it,rd,level);
  imported=0; observed_it=it;
  last_score=*rd;
  header_sum+=rd->H; distortion_sum+=rd->D;
  // A synthetic first-MB header-cost bias drives the original retry branch.
  // Lowering the limit by the same offset in Rust preserves every real mode/score,
  // token, reset and branch timing unchanged. Never used by public references.
  if(it->x==0 && it->y==0) rd->H += PARTITION0_SIZE_LIMIT-header_limit;
  return result;
}
static void ObserveSave(VP8EncIterator* it) {
  unsigned i,y,x; uint32_t h=2166136261u;
  VP8Encoder* enc=it->enc;
  if(trace_enabled) {
    Word(2); Word(loop_pass); Word(it->x+it->y*enc->mb_w);
    Wide(last_score.D); Wide(last_score.SD); Wide(last_score.H); Wide(last_score.R); Wide(last_score.score); Word(last_score.nz);
    for(i=0;i<16;++i) h=(h^(uint32_t)(int32_t)last_score.y_dc_levels[i])*16777619u;
    for(y=0;y<16;++y) for(x=0;x<16;++x) h=(h^(uint32_t)(int32_t)last_score.y_ac_levels[y][x])*16777619u;
    for(y=0;y<8;++y) for(x=0;x<16;++x) h=(h^(uint32_t)(int32_t)last_score.uv_levels[y][x])*16777619u;
    Word(h); h=2166136261u;
    for(y=0;y<16;++y) for(x=0;x<16;++x) h=(h^it->yuv_out[y*BPS+x])*16777619u;
    for(y=0;y<8;++y) for(x=16;x<32;++x) h=(h^it->yuv_out[y*BPS+x])*16777619u;
    Word(h);
    Word(it->mb->type); Word(it->mb->uv_mode); Word(it->mb->skip); Word(it->mb->segment); Word(it->mb->alpha);
    for(i=0;i<9;++i) Word(it->top_nz[i]); for(i=0;i<9;++i) Word(it->left_nz[i]); Word(*it->nz);
    for(y=0;y<2;++y) for(x=0;x<2;++x) Word(enc->top_derr?(uint32_t)(int32_t)it->left_derr[y][x]:0);
    for(y=0;y<2;++y) for(x=0;x<2;++x) Word(enc->top_derr?(uint32_t)(int32_t)enc->top_derr[it->x][y][x]:0);
  }
  VP8IteratorSaveBoundary(it);
  if(trace_enabled && it->x==enc->mb_w-1 && it->y==enc->mb_h-1) {
    uint64_t size=header_sum+enc->segment_hdr.size;
    Word(3); Word(loop_pass); Wide(size); Wide(distortion_sum); Word(enc->max_i4_header_bits>0 && size>header_limit);
  }
}
static int ObserveEmit(VP8TBuffer* tokens, VP8BitWriter* bw, const uint8_t* probas, int final_pass) {
  if(trace_enabled) {
    Word(4); State(observed_enc);
    trace_n+=WebpopLoopTokenWords(tokens,trace_out+trace_n,observed_enc->mb_w*observed_enc->mb_h<=64,(1u<<20)-trace_n);
  }
  return VP8EmitTokens(tokens,bw,probas,final_pass);
}

unsigned WebpopLoopFrame(const uint32_t* in,uint32_t* out) {
  WebPConfig config; WebPPicture pic; WebPMemoryWriter writer; VP8Encoder* enc;
  unsigned x,y,i,n,w=in[0],h=in[1],at=9;
  float quality; memcpy(&quality,in+2,4);
  if(!WebPConfigInit(&config)||!WebPPictureInit(&pic)) abort();
  config.quality=quality; config.segments=in[3];
  pic.width=w; pic.height=h; pic.writer=WebPMemoryWrite;
  WebPMemoryWriterInit(&writer); pic.custom_ptr=&writer;
  if(in[6]) {
    unsigned len=(h-1)*in[8]+4*w;
    uint8_t* rgba=(uint8_t*)malloc(len+1);
    if(!rgba) abort(); for(i=0;i<len;++i) rgba[1+i]=in[at++];
    if(!WebPPictureImportRGBA(&pic,rgba+1,in[8])) abort(); free(rgba);
    WebPCleanupTransparentArea(&pic);
  } else {
    if(!WebPPictureAlloc(&pic)) abort();
    for(y=0;y<h;++y) for(x=0;x<w;++x) pic.y[y*pic.y_stride+x]=in[at++];
    for(y=0;y<(h+1)/2;++y) for(x=0;x<(w+1)/2;++x) pic.u[y*pic.uv_stride+x]=in[at++];
    for(y=0;y<(h+1)/2;++y) for(x=0;x<(w+1)/2;++x) pic.v[y*pic.uv_stride+x]=in[at++];
  }
  enc=InitVP8Encoder(&config,&pic); if(!enc) abort();
  enc->max_i4_header_bits=in[4];
  if(!VP8EncAnalyze(enc)) abort();
  trace_out=out; trace_n=loop_pass=0; initialized=imported=0; observed_enc=enc;
  trace_enabled=in[7]; header_limit=in[5];
  if(!WebpopObservedTokenLoop(enc)) abort();
  if(trace_enabled) {
    Word(5); State(enc);
    n=enc->preds_w*(4*enc->mb_h+1); Word(n);
    for(i=0;i<n;++i) Word(enc->preds[(int)i-1-enc->preds_w]);
    for(i=0;i<(unsigned)(enc->mb_w*enc->mb_h);++i) {
      const VP8MBInfo* mb=enc->mb_info+i;
      Word(mb->type);Word(mb->uv_mode);Word(mb->skip);Word(mb->segment);Word(mb->alpha);
    }
    Word(VP8BitWriterSize(&enc->parts[0]));
    for(i=0;i<VP8BitWriterSize(&enc->parts[0]);++i) Word(enc->parts[0].buf[i]);
  }
  if(!VP8EncWrite(enc)) abort();
  if(trace_enabled) Word(writer.size);
  for(i=0;i<writer.size;++i) Word(writer.mem[i]);
  n=trace_n;
  if(!DeleteVP8Encoder(enc)) abort();
  WebPPictureFree(&pic);WebPMemoryWriterClear(&writer);
  return n;
}

unsigned WebpopFrameSegments(const uint32_t* in,uint32_t* out) {
  VP8Encoder enc={0}; WebPPicture pic={0}; unsigned i,n=0;
  enc.pic=&pic;
  enc.mb_w=in[1];enc.mb_h=1;enc.segment_hdr.num_segments=in[0];
  enc.mb_info=(VP8MBInfo*)calloc(enc.mb_w,sizeof(*enc.mb_info));if(!enc.mb_info) abort();
  for(i=0;i<3;++i) enc.proba.segments[i]=in[2+i];
  for(i=0;i<(unsigned)enc.mb_w;++i) enc.mb_info[i].segment=in[5+i];
  SetSegmentProbas(&enc);
  out[n++]=enc.segment_hdr.update_map;out[n++]=enc.segment_hdr.size;
  for(i=0;i<3;++i) out[n++]=enc.proba.segments[i];
  for(i=0;i<(unsigned)enc.mb_w;++i) out[n++]=enc.mb_info[i].segment;
  free(enc.mb_info);return n;
}
unsigned WebpopAdjustFilter(const uint32_t* in,uint32_t* out) {
  VP8Encoder enc={0};VP8EncIterator it={0};WebPConfig cfg;unsigned i,n=0;
  WebPConfigInit(&cfg);cfg.filter_strength=in[0];enc.config=&cfg;enc.filter_hdr.sharpness=in[1];enc.filter_hdr.level=in[2];
  for(i=0;i<4;++i) { enc.dqm[i].max_edge=in[3+3*i];enc.dqm[i].y2.q[1]=in[4+3*i];enc.dqm[i].fstrength=in[5+3*i]; }
  it.enc=&enc;VP8AdjustFilterStrength(&it);out[n++]=enc.filter_hdr.level;
  for(i=0;i<4;++i) out[n++]=enc.dqm[i].fstrength;
  return n;
}
