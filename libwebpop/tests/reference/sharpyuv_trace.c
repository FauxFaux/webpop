// Include the original image algorithm, renaming exports and intercepting only
// DSP calls to observe its planes. No conversion/iteration formulas are copied.
#define SharpYuvGetVersion WebpopIncludedSharpYuvGetVersion
#define SharpYuvConvert WebpopIncludedSharpYuvConvert
#define SharpYuvConvertWithOptions WebpopIncludedSharpYuvConvertWithOptions
#define SharpYuvOptionsInitInternal WebpopIncludedSharpYuvOptionsInitInternal
#define SharpYuvInit WebpopIncludedSharpYuvInit
#define SharpYuvFilterRow WebpopTraceFilterRow
#define SharpYuvUpdateY WebpopTraceUpdateY
#define SharpYuvUpdateRGB WebpopTraceUpdateRGB
#include "sharpyuv/sharpyuv.c"
#undef SharpYuvFilterRow
#undef SharpYuvUpdateY
#undef SharpYuvUpdateRGB

extern void (*SharpYuvFilterRow)(const int16_t*, const int16_t*, int, const uint16_t*, uint16_t*, int);
extern uint64_t (*SharpYuvUpdateY)(const uint16_t*, const uint16_t*, uint16_t*, int, int);
extern void (*SharpYuvUpdateRGB)(const int16_t*, const int16_t*, int16_t*, int);

static uint32_t* trace_out;
static unsigned trace_pixels, trace_uv_size, trace_uv_h, trace_row, trace_iter, trace_count;
static uint64_t trace_diff;
static const uint16_t *trace_best_y, *trace_target_y;
static const int16_t *trace_best_uv, *trace_target_uv;

static void Snapshot(unsigned iteration) {
  unsigned i;
  trace_out[trace_count++] = iteration;
  trace_out[trace_count++] = trace_diff;
  trace_out[trace_count++] = trace_diff >> 32;
  for (i=0;i<trace_pixels;++i) trace_out[trace_count++]=trace_best_y[i];
  for (i=0;i<trace_pixels;++i) trace_out[trace_count++]=trace_target_y[i];
  for (i=0;i<trace_uv_size;++i) trace_out[trace_count++]=(int32_t)trace_best_uv[i];
  for (i=0;i<trace_uv_size;++i) trace_out[trace_count++]=(int32_t)trace_target_uv[i];
}

static void Filter(const int16_t* a, const int16_t* b, int len,
                   const uint16_t* best_y, uint16_t* out, int depth) {
  if(trace_out && trace_best_y==NULL) {
    // First interpolation call: best_y points one past the first W sample,
    // and A points at the first current UV row. The target planes follow
    // the best planes in the original allocation (see DoSharpArgbToYuv).
    trace_best_y=best_y-1; trace_target_y=trace_best_y+trace_pixels;
    trace_best_uv=a; trace_target_uv=a+trace_uv_size;
    Snapshot(UINT32_MAX);
  }
  SharpYuvFilterRow(a,b,len,best_y,out,depth);
}
static uint64_t UpdateY(const uint16_t* ref,const uint16_t* src,uint16_t* dst,int len,int depth) {
  uint64_t diff=SharpYuvUpdateY(ref,src,dst,len,depth);
  if(trace_out) trace_diff+=diff;
  return diff;
}
static void UpdateRGB(const int16_t* ref,const int16_t* src,int16_t* dst,int len) {
  SharpYuvUpdateRGB(ref,src,dst,len);
  if(trace_out && ++trace_row==trace_uv_h) {
    Snapshot(trace_iter++); trace_row=0; trace_diff=0;
  }
}

uint64_t (*WebpopTraceUpdateY)(const uint16_t*,const uint16_t*,uint16_t*,int,int)=UpdateY;
void (*WebpopTraceUpdateRGB)(const int16_t*,const int16_t*,int16_t*,int)=UpdateRGB;
void (*WebpopTraceFilterRow)(const int16_t*,const int16_t*,int,const uint16_t*,uint16_t*,int)=Filter;

int WebpopSharpYuvTracedConvert(const void* r,const void* g,const void* b,int step,int stride,int rgb_depth,
    void* y,int ys,void* u,int us,void* v,int vs,int yuv_depth,int width,int height,
    const SharpYuvOptions* options,uint32_t* out,unsigned* count) {
  int ok;
  trace_pixels=((width+1)&~1)*((height+1)&~1);
  trace_uv_h=(height+1)/2; trace_uv_size=((width+1)/2)*3*trace_uv_h;
  trace_row=trace_iter=trace_count=0; trace_diff=0; trace_best_y=NULL; trace_out=out;
  ok=WebpopIncludedSharpYuvConvertWithOptions(r,g,b,step,stride,rgb_depth,y,ys,u,us,v,vs,yuv_depth,width,height,options);
  *count=trace_count; trace_out=NULL;
  return ok;
}

unsigned WebpopSharpYuvInspectImport(const uint8_t* r,const uint8_t* g,const uint8_t* b,int step,int stride,
    int depth,int width,int height,int tf,uint32_t* out) {
  unsigned i,at=0,w=(width+1)&~1;
  const int internal_depth=depth+GetPrecisionShift(depth);
  uint16_t *rows=(uint16_t*)malloc(6*w*sizeof(*rows));
  uint16_t *gray=(uint16_t*)malloc(2*w*sizeof(*gray));
  int16_t *uv=(int16_t*)malloc(3*w/2*sizeof(*uv));
  if(!rows || !gray || !uv) abort();
  SharpYuvInitGammaTables(); SharpYuvInitDsp();
  ImportOneRow(r,g,b,step,depth,width,rows);
  ImportOneRow(r+(height>1?stride:0),g+(height>1?stride:0),b+(height>1?stride:0),step,depth,width,rows+3*w);
  for(i=0;i<6*w;++i) out[at++]=rows[i];
  StoreGray(rows,gray,w); StoreGray(rows+3*w,gray+w,w);
  for(i=0;i<2*w;++i) out[at++]=gray[i];
  UpdateW(rows,gray,w,internal_depth,tf); UpdateW(rows+3*w,gray+w,w,internal_depth,tf);
  for(i=0;i<2*w;++i) out[at++]=gray[i];
  UpdateChroma(rows,rows+3*w,uv,w/2,internal_depth,tf);
  for(i=0;i<3*w/2;++i) out[at++]=(int32_t)uv[i];
  free(rows);free(gray);free(uv); return at;
}
