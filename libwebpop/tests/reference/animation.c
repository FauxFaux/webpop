// Access the original animation leaves. Rename all external definitions so the
// normally linked webpmux encoder remains available for future file comparisons.
#define WebPAnimEncoderOptionsInitInternal WebpopIncludedAnimOptionsInit
#define WebPAnimEncoderNewInternal WebpopIncludedAnimNew
#define WebPAnimEncoderDelete WebpopIncludedAnimDelete
#define WebPAnimEncoderRefineRect WebpopIncludedAnimRefineRect
#define WebPAnimEncoderAdd WebpopIncludedAnimAdd
#define WebPAnimEncoderAssemble WebpopIncludedAnimAssemble
#define WebPAnimEncoderGetError WebpopIncludedAnimGetError
#define WebPAnimEncoderSetChunk WebpopIncludedAnimSetChunk
#define WebPAnimEncoderGetChunk WebpopIncludedAnimGetChunk
#define WebPAnimEncoderDeleteChunk WebpopIncludedAnimDeleteChunk
#include "src/mux/anim_encode.c"

unsigned WebpopAnimationLeaves(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned op, i;
  float quality;
  if (n < 1) abort();
  op = in[0];
  if (op == 0) {  // quality -> max difference
    if (n != 2) abort();
    memcpy(&quality, in + 1, sizeof(quality));
    if (!(quality >= 0.f && quality <= 100.f)) abort();
    out[0] = QualityToMaxDiff(quality);
    return 1;
  } else if (op == 1) {  // similarity
    if (n != 4 || in[3] > 255) abort();
    out[0] = PixelsAreSimilar(in[1], in[2], in[3]);
    return 1;
  } else if (op == 2) {  // strided comparisons
    unsigned length, src_step, dst_step, src_size, dst_size;
    if (n < 5 || in[1] == 0 || in[1] > 1024 || in[2] > 64 ||
        in[3] > 64 || in[4] > 255) abort();
    length = in[1]; src_step = in[2]; dst_step = in[3];
    // Include one trailing step: the C loops advance even after the last pixel.
    src_size = length * src_step + 1;
    dst_size = length * dst_step + 1;
    if (n != 5 + src_size + dst_size) abort();
    out[0] = ComparePixelsLossless(in + 5, src_step,
                                 in + 5 + src_size, dst_step, length, in[4]);
    out[1] = ComparePixelsLossy(in + 5, src_step,
                              in + 5 + src_size, dst_step, length, in[4]);
    return 2;
  } else if (op == 3 || op == 4) {  // minimize/refine and blend eligibility
    WebPPicture src, dst;
    FrameRectangle rect;
    uint64_t src_size, dst_size;
    int ok = 1;
    if (n < 12 || !in[1] || !in[2] || in[1] > 16383 || in[2] > 16383 ||
        in[3] < in[1] || in[4] < in[1] || in[3] > 16383 ||
        in[4] > 16383 || in[9] > 1) abort();
    src_size = (uint64_t)(in[2] - 1) * in[3] + in[1];
    dst_size = (uint64_t)(in[2] - 1) * in[4] + in[1];
    if (n != 12 + src_size + dst_size || !WebPPictureInit(&src) ||
        !WebPPictureInit(&dst)) abort();
    src.use_argb = dst.use_argb = 1;
    src.width = dst.width = in[1]; src.height = dst.height = in[2];
    src.argb_stride = in[3]; dst.argb_stride = in[4];
    src.argb = (uint32_t*)(in + 12);
    dst.argb = (uint32_t*)(in + 12 + src_size);
    rect.x_offset = (int32_t)in[5]; rect.y_offset = (int32_t)in[6];
    rect.width = (int32_t)in[7]; rect.height = (int32_t)in[8];
    memcpy(&quality, in + 10, sizeof(quality));
    if (op == 4) {
      ok = WebPAnimEncoderRefineRect(&src, &dst, in[9], quality,
                                    &rect.x_offset, &rect.y_offset,
                                    &rect.width, &rect.height);
    } else {
      if (rect.x_offset < 0 || rect.y_offset < 0 || rect.width < 0 ||
          rect.height < 0 ||
          (int64_t)rect.x_offset + rect.width > src.width ||
          (int64_t)rect.y_offset + rect.height > src.height ||
          !(quality >= 0.f && quality <= 100.f) || in[11] > 1) abort();
      MinimizeChangeRectangle(&src, &dst, &rect, in[9], quality);
      if (in[11]) SnapToEvenOffsets(&rect);
    }
    out[0] = ok;
    out[1] = rect.x_offset; out[2] = rect.y_offset;
    out[3] = rect.width; out[4] = rect.height;
    if (!ok) return 5;
    out[5] = IsEmptyRect(&rect);
    out[6] = IsLosslessBlendingPossible(&src, &dst, &rect);
    out[7] = IsLossyBlendingPossible(&src, &dst, &rect, quality);
    return 8;
  } else if (op == 5) {  // raw even snapping / empty rectangle
    FrameRectangle rect;
    if (n != 5) abort();
    for (i = 1; i < 5; ++i) if (in[i] > INT_MAX - 1) abort();
    rect.x_offset = in[1]; rect.y_offset = in[2];
    rect.width = in[3]; rect.height = in[4];
    out[0] = IsEmptyRect(&rect);
    SnapToEvenOffsets(&rect);
    out[1] = rect.x_offset; out[2] = rect.y_offset;
    out[3] = rect.width; out[4] = rect.height;
    return 5;
  }
  abort();
}
