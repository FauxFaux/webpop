// Static alpha leaves are the original pinned implementations. External alpha
// lifecycle symbols are renamed so public references still use the linked C.
#define VP8EncInitAlpha WebpopIncludedInitAlpha
#define VP8EncStartAlpha WebpopIncludedStartAlpha
#define VP8EncFinishAlpha WebpopIncludedFinishAlpha
#define VP8EncDeleteAlpha WebpopIncludedDeleteAlpha
#include "src/enc/alpha_enc.c"

static unsigned AlphaOps(const uint32_t* in, unsigned count, int effort,
                         int configurable, uint32_t* out) {
  const unsigned op=in[0],w=in[1],h=in[2],stride=in[3];
  const unsigned len=count-9;
  uint8_t* allocation=(uint8_t*)malloc(len+1);
  uint8_t* data;
  unsigned i,n=0;
  if(!allocation) abort();data=allocation+1;
  for(i=0;i<len;++i) data[i]=in[9+i];
  WebPInitAlphaProcessing();VP8FiltersInit();
  if(op==0) {
    unsigned size=(h-1)*in[8]+w+5;
    uint32_t* dst=(uint32_t*)malloc(size*sizeof(*dst));if(!dst) abort();
    for(i=0;i<size;++i) dst[i]=0x5aa5a55a;
    WebPDispatchAlphaToGreen(data,stride,w,h,dst,in[8]);
    for(i=0;i<size;++i) out[n++]=dst[i];free(dst);
  } else if(op==1) {
    uint8_t* filtered=(uint8_t*)malloc(len+5);if(!filtered) abort();memset(filtered,0xa5,len+5);
    if(in[4]) WebPFilters[in[4]](data,w,h,stride,filtered);
    else WebPCopyPlane(data,stride,filtered,stride,w,h);
    for(i=0;i<len+5;++i) out[n++]=filtered[i];free(filtered);
  } else if(op==2) {out[n++]=WebPEstimateBestFilter(data,w,h);
  } else if(op==3) {
    uint64_t sse=0x12345678;
    int ok=QuantizeLevels(in[6]?NULL:data,w,h,in[4],in[5]?NULL:&sse);
    out[n++]=ok;out[n++]=(uint32_t)sse;out[n++]=(uint32_t)(sse>>32);
    for(i=0;i<len;++i) out[n++]=data[i];
  } else if(op==4) {out[n++]=GetNumColors(data,w,h,stride);
  } else if(op==5) {out[n++]=GetFilterMap(data,w,h,in[4],in[5]);
  } else if(op==6) {
    VP8LBitWriter bw;
    if(!VP8LBitWriterInit(&bw,0)||!EncodeLossless(data,w,h,effort,configurable ? !in[6] : 1,&bw,NULL)) abort();
    VP8LBitWriterFinish(&bw);if(bw.error) abort();
    for(i=0;i<VP8LBitWriterNumBytes(&bw);++i) out[n++]=bw.buf[i];VP8LBitWriterWipeOut(&bw);
  } else if(op==7) {
    FilterTrial result;uint8_t* tmp=(uint8_t*)malloc(w*h);if(!tmp) abort();
    if(!EncodeAlphaInternal(data,w,h,in[4],in[5],in[6],effort,tmp,&result)) abort();
    for(i=0;i<VP8BitWriterSize(&result.bw);++i) out[n++]=result.bw.buf[i];
    VP8BitWriterWipeOut(&result.bw);free(tmp);
  } else if(op==8) {
    uint8_t* bytes=NULL;size_t size=0;
    if(!ApplyFiltersAndEncode(data,w,h,w*h,in[4],in[5],in[6],effort,&bytes,&size,NULL)) abort();
    for(i=0;i<size;++i) out[n++]=bytes[i];WebPSafeFree(bytes);
  } else if(op==9) {
    VP8Encoder enc={0};WebPPicture pic;WebPAuxStats stats={0};uint8_t* bytes=NULL;size_t size=0;int ok;
    if(!WebPPictureInit(&pic)) abort();pic.width=w;pic.height=h;pic.a=data;pic.a_stride=stride;pic.stats=&stats;enc.pic=&pic;
    ok=EncodeAlpha(&enc,(int32_t)in[4],in[5],in[6],effort,&bytes,&size);
    out[n++]=ok;out[n++]=(uint32_t)enc.sse[3];out[n++]=(uint32_t)(enc.sse[3]>>32);
    for(i=0;i<size;++i) out[n++]=bytes[i];WebPSafeFree(bytes);
  } else if(op==10) {
    uint32_t map=GetFilterMap(data,w,h,in[5],effort);
    uint8_t* tmp=(uint8_t*)malloc(w*h);if(!tmp) abort();
    uint8_t* bytes=NULL;size_t size=0;unsigned filter;
    out[n++]=map;
    for(filter=0;filter<WEBP_FILTER_LAST;++filter) if(map&(1u<<filter)) {
      FilterTrial trial;
      if(!EncodeAlphaInternal(data,w,h,in[4],filter,in[6],effort,tmp,&trial)) abort();
      out[n++]=filter;out[n++]=VP8BitWriterSize(&trial.bw);
      for(i=0;i<VP8BitWriterSize(&trial.bw);++i) out[n++]=trial.bw.buf[i];
      VP8BitWriterWipeOut(&trial.bw);
    }
    if(!ApplyFiltersAndEncode(data,w,h,w*h,in[4],in[5],in[6],effort,&bytes,&size,NULL)) abort();
    out[n++]=size;for(i=0;i<size;++i) out[n++]=bytes[i];
    WebPSafeFree(bytes);free(tmp);
  } else {abort();}
  free(allocation);
  return n;
}

unsigned WebpopAlphaOps(const uint32_t* in, unsigned count, uint32_t* out) {
  return AlphaOps(in,count,4,0,out);
}
unsigned WebpopAlphaMethodOps(const uint32_t* in, unsigned count, uint32_t* out) {
  return AlphaOps(in+1,count-1,in[0],1,out);
}
