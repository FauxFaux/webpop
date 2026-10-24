// Test-only encoder integration adapters. Conversion, cleanup and encoding
// call the normally linked pinned C library, without copied algorithms.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "src/webp/encode.h"

unsigned WebpopSharpYuvEncoder(const uint32_t* in, unsigned n, int import_only,
                               uint32_t* out) {
  const unsigned offset=import_only?4:7;
  unsigned i,x,y,at=0;
  WebPPicture pic;
  WebPConfig config;
  WebPMemoryWriter writer;
  uint8_t* storage=(uint8_t*)malloc(n-offset+1);
  uint8_t* rgba;
  float quality;
  if(!storage || !WebPPictureInit(&pic)) abort();
  rgba=storage+1;
  for(i=offset;i<n;++i) rgba[i-offset]=in[i];
  pic.width=in[0];pic.height=in[1];pic.use_argb=1;
  if(!WebPPictureImportRGBA(&pic,rgba,in[2])) abort();
  if(import_only) {
    if(!WebPPictureSharpARGBToYUVA(&pic)) abort();
    if(in[3]) WebPCleanupTransparentArea(&pic);
    out[at++]=WebPPictureHasTransparency(&pic);out[at++]=pic.a!=NULL;
    for(y=0;y<in[1];++y) for(x=0;x<in[0];++x) out[at++]=pic.y[y*pic.y_stride+x];
    for(y=0;y<(in[1]+1)/2;++y) for(x=0;x<(in[0]+1)/2;++x) out[at++]=pic.u[y*pic.uv_stride+x];
    for(y=0;y<(in[1]+1)/2;++y) for(x=0;x<(in[0]+1)/2;++x) out[at++]=pic.v[y*pic.uv_stride+x];
    if(pic.a) for(y=0;y<in[1];++y) for(x=0;x<in[0];++x) out[at++]=pic.a[y*pic.a_stride+x];
  } else {
    memcpy(&quality,in+3,sizeof(quality));
    if(!WebPConfigPreset(&config,WEBP_PRESET_DEFAULT,quality)) abort();
    config.alpha_quality=(int32_t)in[4];
    config.use_sharp_yuv=in[5];config.preprocessing=in[6];
    WebPMemoryWriterInit(&writer);pic.writer=WebPMemoryWrite;pic.custom_ptr=&writer;
    if(!WebPEncode(&config,&pic) || writer.size>(1u<<20)) abort();
    for(i=0;i<writer.size;++i) out[at++]=writer.mem[i];
    WebPMemoryWriterClear(&writer);
  }
  for(i=offset;i<n;++i) if(rgba[i-offset]!=in[i]) abort();
  WebPPictureFree(&pic);free(storage);return at;
}
