// Inspect original static gamma helpers without replacing the normally linked
// importer. No conversion or cleanup formulas are copied into this adapter.
#define WebPSamplerProcessPlane WebpopIncludedSamplerProcessPlane
#define WebPSamplers WebpopIncludedSamplers
#define WebPInitSamplers WebpopIncludedInitSamplers
#define WebPConvertARGBToUV_C WebpopIncludedConvertARGBToUV_C
#define WebPConvertRGBA32ToUV_C WebpopIncludedConvertRGBA32ToUV_C
#define WebPInitGammaTables WebpopIncludedInitGammaTables
#define WebPAccumulateRGBA WebpopIncludedAccumulateRGBA
#define WebPAccumulateRGB WebpopIncludedAccumulateRGB
#define WebPConvertRGBToY WebpopIncludedConvertRGBToY
#define WebPConvertBGRToY WebpopIncludedConvertBGRToY
#define WebPConvertRGBA32ToUV WebpopIncludedConvertRGBA32ToUV
#define WebPImportYUVAFromRGBA WebpopIncludedImportYUVAFromRGBA
#define WebPImportYUVAFromRGBALastLine WebpopIncludedImportYUVAFromRGBALastLine
#define WebPConvertARGBToY WebpopIncludedConvertARGBToY
#define WebPConvertARGBToUV WebpopIncludedConvertARGBToUV
#define WebPInitConvertARGBToYUV WebpopIncludedInitConvertARGBToYUV
#include "src/dsp/yuv.c"
#include "src/webp/encode.h"

unsigned WebpopYuvTables(uint32_t* out) {
  unsigned i, n = 0;
  WebPInitGammaTables();
  for (i = 0; i < 256; ++i) out[n++] = kGammaToLinearTab[i];
  for (i = 0; i < 33; ++i) out[n++] = kLinearToGammaTab[i];
  for (i = 0; i < 1021; ++i) out[n++] = kInvAlpha[i];
  return n;
}
unsigned WebpopYuvGamma(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned i, at = 0;
  WebPInitGammaTables();
  for (i = 0; i < n; i += 2) {
    out[at++] = Interpolate(in[i] << in[i + 1]);
    out[at++] = LinearToGamma(in[i], in[i + 1]);
  }
  return at;
}
unsigned WebpopYuvMatrix(const uint32_t* in, int uv, uint32_t* out) {
  if (!uv) {
    out[0] = VP8RGBToY(in[0], in[1], in[2], in[3]);
    return 1;
  }
  out[0] = VP8RGBToU(in[0], in[1], in[2], in[3]);
  out[1] = VP8RGBToV(in[0], in[1], in[2], in[3]);
  return 2;
}
unsigned WebpopYuvAccumulate(const uint32_t* in, unsigned n, uint32_t* out) {
  const unsigned width = in[0], stride = in[1], uv_width = (width + 1) / 2;
  unsigned i, at = 0;
  uint8_t* rgba = (uint8_t*)malloc(n - 3 + 1);
  uint16_t* tmp = (uint16_t*)malloc(4 * uv_width * sizeof(*tmp));
  uint8_t* u = (uint8_t*)malloc(uv_width);
  uint8_t* v = (uint8_t*)malloc(uv_width);
  if (rgba == NULL || tmp == NULL || u == NULL || v == NULL) abort();
  for (i = 3; i < n; ++i) rgba[i - 3] = in[i];
  for (i = 0; i < 4 * uv_width; ++i) tmp[i] = 0xa55a;
  WebPInitGammaTables();
  if (in[2]) WebPAccumulateRGBA(rgba, rgba + 1, rgba + 2, rgba + 3, stride, tmp, width);
  else WebPAccumulateRGB(rgba, rgba + 1, rgba + 2, 4, stride, tmp, width);
  WebPConvertRGBA32ToUV_C(tmp, u, v, uv_width);
  for (i = 0; i < 4 * uv_width; ++i) out[at++] = tmp[i];
  for (i = 0; i < uv_width; ++i) out[at++] = u[i];
  for (i = 0; i < uv_width; ++i) out[at++] = v[i];
  free(rgba); free(tmp); free(u); free(v);
  return at;
}
unsigned WebpopYuvaImport(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPPicture pic;
  unsigned i, x, y, at = 0;
  uint8_t* storage = (uint8_t*)malloc(n - 4 + 1);
  uint8_t* rgba;
  if (storage == NULL || !WebPPictureInit(&pic)) abort();
  // Exercise unaligned input and the checked minimum final-row length.
  rgba = storage + 1;
  for (i = 4; i < n; ++i) rgba[i - 4] = in[i];
  pic.width = in[0]; pic.height = in[1];
  if (!WebPPictureImportRGBA(&pic, rgba, in[2])) abort();
  if (in[3]) WebPCleanupTransparentArea(&pic);
  out[at++] = WebPPictureHasTransparency(&pic);
  out[at++] = pic.a != NULL;
  for (y = 0; y < in[1]; ++y) for (x = 0; x < in[0]; ++x)
    out[at++] = pic.y[y * pic.y_stride + x];
  for (y = 0; y < (in[1] + 1) / 2; ++y) for (x = 0; x < (in[0] + 1) / 2; ++x)
    out[at++] = pic.u[y * pic.uv_stride + x];
  for (y = 0; y < (in[1] + 1) / 2; ++y) for (x = 0; x < (in[0] + 1) / 2; ++x)
    out[at++] = pic.v[y * pic.uv_stride + x];
  if (pic.a != NULL) for (y = 0; y < in[1]; ++y) for (x = 0; x < in[0]; ++x)
    out[at++] = pic.a[y * pic.a_stride + x];
  WebPPictureFree(&pic); free(storage);
  return at;
}
