// Public calls use the normally built pinned library, not the included helpers.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "sharpyuv/sharpyuv.h"

int WebpopSharpYuvTracedConvert(const void*,const void*,const void*,int,int,int,
    void*,int,void*,int,void*,int,int,int,int,const SharpYuvOptions*,uint32_t*,unsigned*);
unsigned WebpopSharpYuvInspectImport(const uint8_t*,const uint8_t*,const uint8_t*,int,int,int,int,int,int,uint32_t*);

int WebpopSharpYuvPublic(const char* name, const uint32_t* in, unsigned n,
                        uint32_t* out, unsigned* count) {
  unsigned width, height, step, stride, rgb_depth, yuv_depth, tf, simple;
  unsigned i, k, x, y, len, uv_w, uv_h, bytes, at = 0, guard_ok = 1;
  SharpYuvConversionMatrix matrix;
  SharpYuvOptions options;
  uint8_t *rgb[3], *planes[3];
  unsigned strides[3], widths[3], heights[3], sizes[3];
  clock_t started;
  if (!strcmp(name, "SharpYuvPublicInfo")) {
    if (n != 1) return 0;
    memset(&matrix, 0, sizeof(matrix));
    out[at++] = SharpYuvGetVersion();
    out[at++] = SharpYuvOptionsInitInternal(&matrix, &options, in[0]);
    if (out[1]) out[at++] = options.transfer_type;
    *count = at; return 1;
  }
  if ((strcmp(name, "SharpYuvConvert") && strcmp(name, "SharpYuvTrace") && strcmp(name, "SharpYuvImport") && strcmp(name,"SharpYuvTimedConvert")) || n < 20) return 0;
  width=in[0]; height=in[1]; step=in[2]; stride=in[3];
  rgb_depth=in[4]; yuv_depth=in[5]; tf=in[6]; simple=in[7];
  if (!width || !height || width > 100000 || height > 100000 ||
      !step || step > 8 || stride < width*step || stride > 800064 ||
      (rgb_depth!=8 && rgb_depth!=10 && rgb_depth!=12 && rgb_depth!=16) ||
      (yuv_depth!=8 && yuv_depth!=10 && yuv_depth!=12) ||
      tf < 1 || tf == 2 || tf == 3 || tf > 18 || simple > 1) return 0;
  if (20 + 3*((uint64_t)(height-1)*stride+(width-1)*step+1) != n) return 0;
  len=(height-1)*stride+(width-1)*step+1;
  uv_w=(width+1)/2; uv_h=(height+1)/2;
  if (2+(uint64_t)width*height+2*(uint64_t)uv_w*uv_h > (1u<<20)) return 0;
  if (!strcmp(name,"SharpYuvTrace") && (simple || 1+5*(3+14*(uint64_t)uv_w*uv_h)+(uint64_t)width*height+2*(uint64_t)uv_w*uv_h > (1u<<20))) return 0;
  if (!strcmp(name,"SharpYuvImport") && 23*(uint64_t)uv_w > (1u<<20)) return 0;
  for (k=0;k<3;++k) {
    int* row=k==0?matrix.rgb_to_y:k==1?matrix.rgb_to_u:matrix.rgb_to_v;
    for(i=0;i<3;++i) {
      row[i]=(int32_t)in[8+4*k+i];
      if(row[i]<-131072 || row[i]>131072) return 0;
    }
    row[3]=(int32_t)in[11+4*k];
    if(row[3]<0 || row[3]>536870911) return 0;
  }
  bytes=rgb_depth>8?2:1;
  for(k=0;k<3;++k) {
    rgb[k]=(uint8_t*)malloc(bytes*(len+1));
    if(!rgb[k]) abort();
    for(i=0;i<len;++i) {
      if(bytes==1) rgb[k][i+1]=in[20+k*len+i];
      else ((uint16_t*)rgb[k])[i+1]=in[20+k*len+i];
    }
  }
  if (!strcmp(name,"SharpYuvImport")) {
    *count=WebpopSharpYuvInspectImport(rgb[0]+bytes,rgb[1]+bytes,rgb[2]+bytes,step*bytes,stride*bytes,rgb_depth,width,height,tf,out);
    for(k=0;k<3;++k) free(rgb[k]);
    return 1;
  }
  bytes=yuv_depth>8?2:1;
  widths[0]=width; widths[1]=widths[2]=uv_w;
  heights[0]=height; heights[1]=heights[2]=uv_h;
  for(k=0;k<3;++k) {
    strides[k]=(widths[k]+2)*bytes;
    sizes[k]=strides[k]*heights[k]+2*bytes;
    planes[k]=(uint8_t*)malloc(sizes[k]);
    if(!planes[k]) abort();
    memset(planes[k],0xa5,sizes[k]);
  }
  if(!SharpYuvOptionsInit(&matrix,&options)) abort();
  options.transfer_type=tf;
  started=clock();
  if (!strcmp(name,"SharpYuvTrace")) {
    unsigned trace_count=0;
    if(!WebpopSharpYuvTracedConvert(rgb[0]+(rgb_depth>8?2:1),rgb[1]+(rgb_depth>8?2:1),rgb[2]+(rgb_depth>8?2:1),step*(rgb_depth>8?2:1),stride*(rgb_depth>8?2:1),rgb_depth,
      planes[0]+bytes,strides[0],planes[1]+bytes,strides[1],planes[2]+bytes,strides[2],yuv_depth,width,height,&options,out+1,&trace_count)) abort();
    at=trace_count;
  } else if (!(simple ? SharpYuvConvert(rgb[0]+(rgb_depth>8?2:1),rgb[1]+(rgb_depth>8?2:1),rgb[2]+(rgb_depth>8?2:1),step*(rgb_depth>8?2:1),stride*(rgb_depth>8?2:1),rgb_depth,
      planes[0]+bytes,strides[0],planes[1]+bytes,strides[1],planes[2]+bytes,strides[2],yuv_depth,width,height,&matrix)
    : SharpYuvConvertWithOptions(rgb[0]+(rgb_depth>8?2:1),rgb[1]+(rgb_depth>8?2:1),rgb[2]+(rgb_depth>8?2:1),step*(rgb_depth>8?2:1),stride*(rgb_depth>8?2:1),rgb_depth,
      planes[0]+bytes,strides[0],planes[1]+bytes,strides[1],planes[2]+bytes,strides[2],yuv_depth,width,height,&options))) abort();
  if(at==0) out[0]=1;
  ++at;
  if(!strcmp(name,"SharpYuvTimedConvert")) out[at++]=(uint32_t)(1000000.0*(clock()-started)/CLOCKS_PER_SEC);
  for(k=0;k<3;++k) {
    for(i=0;i<bytes;++i) if(planes[k][i]!=0xa5 || planes[k][sizes[k]-1-i]!=0xa5) guard_ok=0;
    for(y=0;y<heights[k];++y) {
      const uint8_t* row=planes[k]+bytes+y*strides[k];
      for(x=0;x<widths[k];++x) out[at++]=bytes==1?row[x]:((const uint16_t*)row)[x];
      for(i=widths[k]*bytes;i<strides[k];++i) if(row[i]!=0xa5) guard_ok=0;
    }
    free(rgb[k]); free(planes[k]);
  }
  out[0]=guard_ok; *count=at; return 1;
}
