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
// Compact method observations deliberately exclude uninitialized candidate
// fields. Statistics/final scans may use different RD levels and visit counts.
static int method_observing;
static unsigned method_pass_count, method_refresh_count, method_emit_count;
static struct {
  unsigned level, visited, intra16, intra4, zero, trellis;
} method_passes[32];
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
  if(method_observing) {
    if(method_pass_count >= 32) abort();
    memset(&method_passes[method_pass_count++],0,sizeof(method_passes[0]));
  }
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
  if(method_observing && imported) ++method_refresh_count;
  if(trace_enabled && imported) {
    Word(1); Word(loop_pass); Word(observed_it->x+observed_it->y*observed_enc->mb_w); State(observed_enc);
  }
}
static int ObserveDecimate(VP8EncIterator* it, VP8ModeScore* rd, VP8RDLevel level) {
  int result=VP8Decimate(it,rd,level);
  if(method_observing) {
    const unsigned pass=method_pass_count-1;
    method_passes[pass].level=level;
    ++method_passes[pass].visited;
    method_passes[pass].intra16+=(it->mb->type==1);
    method_passes[pass].intra4+=(it->mb->type==0);
    method_passes[pass].zero+=result!=0;
    method_passes[pass].trellis+=it->do_trellis!=0;
  }
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
  if(method_observing) ++method_emit_count;
  if(trace_enabled) {
    Word(4); State(observed_enc);
    trace_n+=WebpopLoopTokenWords(tokens,trace_out+trace_n,observed_enc->mb_w*observed_enc->mb_h<=64,(1u<<20)-trace_n);
  }
  return VP8EmitTokens(tokens,bw,probas,final_pass);
}

static unsigned LoopFrame(const uint32_t* in,uint32_t* out,
                          unsigned method,int compact) {
  WebPConfig config; WebPPicture pic; WebPMemoryWriter writer; VP8Encoder* enc;
  unsigned x,y,i,n,w=in[0],h=in[1],at=9;
  float quality; memcpy(&quality,in+2,4);
  if(!WebPConfigInit(&config)||!WebPPictureInit(&pic)) abort();
  config.quality=quality; config.segments=in[3];config.method=method;
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
  trace_enabled=compact?0:in[7]; header_limit=in[5];
  method_observing=compact;
  method_pass_count=method_refresh_count=method_emit_count=0;
  if(!(enc->use_tokens?WebpopObservedTokenLoop(enc):WebpopObservedEncLoop(enc))) abort();
  method_observing=0;
  if(compact) {
    uint32_t hash=2166136261u;unsigned p,t,b,c;
    Word(enc->method);Word(enc->rd_opt_level);Word(enc->use_tokens);Word(enc->num_parts);
    Word(method_pass_count);
    for(p=0;p<method_pass_count;++p) {
      Word(method_passes[p].level);Word(method_passes[p].visited);
      Word(method_passes[p].intra16);Word(method_passes[p].intra4);
      Word(method_passes[p].zero);Word(method_passes[p].trellis);
    }
    Word(method_refresh_count);Word(method_emit_count);
    Word(enc->proba.use_skip_proba);Word(enc->proba.skip_proba);Word(enc->proba.nb_skip);
    for(t=0;t<4;++t) for(b=0;b<8;++b) for(c=0;c<3;++c) for(p=0;p<11;++p)
      hash=(hash^enc->proba.coeffs[t][b][c][p])*16777619u;
    Word(hash);Word(enc->filter_hdr.level);
    Word(VP8BitWriterSize(&enc->parts[0]));
  }
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
  if(trace_enabled || compact) Word(writer.size);
  for(i=0;i<writer.size;++i) Word(writer.mem[i]);
  n=trace_n;
  if(!DeleteVP8Encoder(enc)) abort();
  WebPPictureFree(&pic);WebPMemoryWriterClear(&writer);
  return n;
}

unsigned WebpopLoopFrame(const uint32_t* in,uint32_t* out) {
  return LoopFrame(in,out,4,0);
}

unsigned WebpopMethodLoopFrame(const uint32_t* in,uint32_t* out) {
  return LoopFrame(in+1,out,in[0],1);
}

unsigned WebpopMethodTools(const uint32_t* in,uint32_t* out) {
  WebPConfig config;WebPPicture pic;VP8Encoder* enc;float quality;unsigned n=0;
  memcpy(&quality,in+2,sizeof(quality));
  if(!WebPConfigPreset(&config,WEBP_PRESET_DEFAULT,quality) || !WebPPictureInit(&pic)) abort();
  config.method=in[3];config.low_memory=in[4];config.partitions=in[5];
  if(!WebPValidateConfig(&config)) abort();
  pic.width=in[0];pic.height=in[1];
  enc=InitVP8Encoder(&config,&pic);if(!enc) abort();
  out[n++]=enc->method;out[n++]=enc->rd_opt_level;
  out[n++]=enc->use_tokens;out[n++]=enc->num_parts;out[n++]=enc->do_search;
  out[n++]=enc->max_i4_header_bits;
  out[n++]=(uint32_t)enc->mb_header_limit;out[n++]=(uint64_t)enc->mb_header_limit>>32;
  out[n++]=enc->top_derr!=NULL;out[n++]=enc->profile;
  if(!DeleteVP8Encoder(enc)) abort();
  WebPPictureFree(&pic);return n;
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
