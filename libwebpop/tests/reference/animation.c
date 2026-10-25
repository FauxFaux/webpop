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
#include "src/webp/demux.h"

// Test-only all-frame decoder. Canonicalize invisible RGB in the observations.
unsigned WebpopAnimationDecode(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPAnimDecoderOptions options;
  WebPAnimDecoder* decoder;
  WebPAnimInfo info;
  WebPData data;
  uint8_t* bytes;
  unsigned i, at = 0, count = 0;
  if (n < 1 || in[0] != n - 1) abort();
  bytes = (uint8_t*)malloc(in[0]);
  if (!bytes) abort();
  for (i = 0; i < in[0]; ++i) bytes[i] = in[i + 1];
  data.bytes = bytes; data.size = in[0];
  if (!WebPAnimDecoderOptionsInit(&options)) abort();
  options.color_mode = MODE_RGBA; options.use_threads = 0;
  decoder = WebPAnimDecoderNew(&data, &options);
  if (!decoder || !WebPAnimDecoderGetInfo(decoder, &info) ||
      info.canvas_width * (uint64_t)info.canvas_height * 4 * info.frame_count > (1u << 19)) abort();
  out[at++] = info.canvas_width; out[at++] = info.canvas_height;
  out[at++] = info.frame_count; out[at++] = info.loop_count;
  while (WebPAnimDecoderHasMoreFrames(decoder)) {
    uint8_t* rgba;
    int timestamp;
    if (!WebPAnimDecoderGetNext(decoder, &rgba, &timestamp)) abort();
    out[at++] = timestamp;
    for (i = 0; i < info.canvas_width * info.canvas_height * 4; i += 4) {
      out[at++] = rgba[i + 3] ? rgba[i] : 0;
      out[at++] = rgba[i + 3] ? rgba[i + 1] : 0;
      out[at++] = rgba[i + 3] ? rgba[i + 2] : 0;
      out[at++] = rgba[i + 3];
    }
    ++count;
  }
  if (count != info.frame_count) abort();
  WebPAnimDecoderDelete(decoder);
  free(bytes);
  return at;
}

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

// Serialize logical view contents only: no addresses or padding words.
static unsigned AnimationSubFrameOutput(const FrameRectangle* rect,
                                        const WebPPicture* view,
                                        uint32_t* out) {
  unsigned at = 0;
  int x, y;
  out[at++] = rect->x_offset; out[at++] = rect->y_offset;
  out[at++] = rect->width; out[at++] = rect->height;
  out[at++] = view->argb != NULL;
  if (view->argb != NULL) {
    out[at++] = view->width; out[at++] = view->height;
    out[at++] = view->argb_stride;
    for (y = 0; y < view->height; ++y) {
      for (x = 0; x < view->width; ++x) {
        out[at++] = view->argb[y * view->argb_stride + x];
      }
    }
  }
  return at;
}

unsigned WebpopAnimationSubframes(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPPicture src, dst, view;
  FrameRectangle rect;
  SubFrameParams params;
  unsigned src_size, dst_size, at = 0, op;
  float quality;
  int ok;
  if (n < 15 || !in[1] || !in[2] || in[1] > 64 || in[2] > 64 ||
      in[3] < in[1] || in[4] < in[1] || in[3] > 128 || in[4] > 128 ||
      in[5] > 1 || in[6] > 1 || in[7] > 1 || in[8] > 1 || in[10] > 1) abort();
  op = in[0];
  src_size = (in[2] - 1) * in[3] + in[1];
  dst_size = (in[2] - 1) * in[4] + in[1];
  if (n != 15 + src_size + dst_size || !WebPPictureInit(&src) ||
      !WebPPictureInit(&dst) || !WebPPictureInit(&view)) abort();
  src.use_argb = dst.use_argb = 1;
  src.width = dst.width = in[1]; src.height = dst.height = in[2];
  src.argb_stride = in[3]; dst.argb_stride = in[4];
  src.argb = (uint32_t*)(in + 15);
  dst.argb = (uint32_t*)(in + 15 + src_size);
  memcpy(&quality, in + 9, sizeof(quality));
  rect.x_offset = (int32_t)in[11]; rect.y_offset = (int32_t)in[12];
  rect.width = (int32_t)in[13]; rect.height = (int32_t)in[14];
  if (op == 0) {  // Raw ARGB view, including rejected rectangles.
    ok = WebPPictureView(&dst, rect.x_offset, rect.y_offset,
                         rect.width, rect.height, &view);
    out[at++] = ok;
    if (ok) at += AnimationSubFrameOutput(&rect, &view, out + at);
  } else {
    if (!(quality >= 0.f && quality <= 100.f)) abort();
    if (op == 1) {
      if (rect.x_offset < 0 || rect.y_offset < 0 || rect.width < 0 ||
          rect.height < 0 ||
          (int64_t)rect.x_offset + rect.width > src.width ||
          (int64_t)rect.y_offset + rect.height > src.height) abort();
      ok = GetSubRect(&src, &dst, in[5], in[6], in[7], in[8],
                      quality, &rect, &view);
      out[at++] = ok;
      if (ok) at += AnimationSubFrameOutput(&rect, &view, out + at);
    } else if (op == 2) {
      if (!SubFrameParamsInit(&params, in[10], in[7])) abort();
      ok = GetSubRects(&src, &dst, in[5], in[6], quality, &params);
      out[at++] = ok;
      if (ok) {
        out[at++] = params.should_try;
        out[at++] = params.empty_rect_allowed;
        at += AnimationSubFrameOutput(&params.rect_ll, &params.sub_frame_ll, out + at);
        at += AnimationSubFrameOutput(&params.rect_lossy, &params.sub_frame_lossy, out + at);
      }
      SubFrameParamsFree(&params);
    } else {
      abort();
    }
  }
  WebPPictureFree(&view);
  return at;
}

static unsigned AnimationCandidateOutput(const FrameRectangle* rect,
                                         const WebPMuxFrameInfo* info,
                                         int previous_dispose, int carries,
                                         const uint8_t* mask, uint32_t* out) {
  unsigned i, at = 0;
  const unsigned area = rect->width * rect->height;
  out[at++] = rect->x_offset; out[at++] = rect->y_offset;
  out[at++] = rect->width; out[at++] = rect->height;
  out[at++] = info->blend_method;
  out[at++] = previous_dispose;
  out[at++] = carries;
  out[at++] = carries ? area : 0;
  if (carries) for (i = 0; i < area; ++i) out[at++] = mask[i];
  if (info->bitstream.size > (1u << 19)) abort();
  out[at++] = info->bitstream.size;
  for (i = 0; i < info->bitstream.size; ++i) out[at++] = info->bitstream.bytes[i];
  if (info->duration != 0 || info->dispose_method != WEBP_MUX_DISPOSE_NONE) abort();
  return at;
}

unsigned WebpopAnimationLossless(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPPicture src, dst, view;
  FrameRectangle rect;
  WebPConfig config;
  unsigned src_size, dst_size, i, at = 0;
  uint32_t* pixels;
  uint8_t* mask;
  unsigned op, flags;
  if (n < 18) abort();
  op = in[0]; flags = in[9];
  if (!in[1] || !in[2] || in[1] > 64 || in[2] > 64 ||
      in[3] < in[1] || in[4] < in[1] || in[3] > 128 || in[4] > 128 ||
      flags > 63 || in[10] > 100 || in[11] > 6 || in[12] > 1) abort();
  src_size = (in[2] - 1) * in[3] + in[1];
  dst_size = (in[2] - 1) * in[4] + in[1];
  if (n != 18 + src_size + dst_size + in[17] ||
      !WebPPictureInit(&src) || !WebPPictureInit(&dst) || !WebPPictureInit(&view) ||
      !WebPConfigInit(&config)) abort();
  pixels = (uint32_t*)malloc(dst_size * sizeof(*pixels));
  mask = (uint8_t*)malloc(in[17] ? in[17] : 1);
  if (pixels == NULL || mask == NULL) abort();
  memcpy(pixels, in + 18 + src_size, dst_size * sizeof(*pixels));
  for (i = 0; i < in[17]; ++i) mask[i] = in[18 + src_size + dst_size + i];
  src.use_argb = dst.use_argb = 1;
  src.width = dst.width = in[1]; src.height = dst.height = in[2];
  src.argb_stride = in[3]; dst.argb_stride = in[4];
  src.argb = (uint32_t*)(in + 18); dst.argb = pixels;
  rect.x_offset = in[5]; rect.y_offset = in[6]; rect.width = in[7]; rect.height = in[8];
  if (rect.x_offset < 0 || rect.y_offset < 0 || rect.width < 0 || rect.height < 0 ||
      (int64_t)rect.x_offset + rect.width > src.width ||
      (int64_t)rect.y_offset + rect.height > src.height) abort();
  config.lossless = 1; config.quality = in[10]; config.method = in[11]; config.exact = in[12];
  if (op == 0) {
    if (in[17] < (unsigned)(rect.width * rect.height)) abort();
    out[at++] = IncreaseTransparency(&src, &rect, &dst, mask);
    for (i = 0; i < dst_size; ++i) out[at++] = pixels[i];
    for (i = 0; i < in[17]; ++i) out[at++] = mask[i];
  } else if (op == 1) {
    DisposeFrameRectangle((flags & 32) ? WEBP_MUX_DISPOSE_BACKGROUND : WEBP_MUX_DISPOSE_NONE,
                          &rect, &dst);
    for (i = 0; i < dst_size; ++i) out[at++] = pixels[i];
  } else if (op == 2) {
    Candidate candidate;
    if (!WebPPictureView(&dst, rect.x_offset, rect.y_offset, rect.width, rect.height, &view) ||
        EncodeCandidate(&view, &rect, &config, (flags & 16) != 0, &candidate) != VP8_ENC_OK) abort();
    candidate.info.bitstream.bytes = candidate.mem.mem;
    candidate.info.bitstream.size = candidate.mem.size;
    out[at++] = 1;
    at += AnimationCandidateOutput(&rect, &candidate.info, 0, 0, NULL, out + at);
    WebPMemoryWriterClear(&candidate.mem);
  } else if (op == 3) {
    WebPAnimEncoderOptions options;
    WebPAnimEncoder* enc;
    EncodedFrame* encoded;
    FrameRectangle best_rect;
    WebPMuxFrameInfo* info;
    int skipped = 0;
    const int key = flags & 1;
    if (!WebpopIncludedAnimOptionsInit(&options, WEBP_MUX_ABI_VERSION)) abort();
    options.minimize_size = (flags & 8) != 0;
    enc = WebpopIncludedAnimNew(src.width, src.height, &options, WEBP_MUX_ABI_VERSION);
    if (!enc || ((flags & 2) && !key)) abort();
    enc->count = 2;  // Provide the preceding frame whose disposal is selected.
    enc->curr_canvas = &dst;
    WebPCopyPixels(&dst, &enc->curr_canvas_copy);
    enc->curr_canvas_copy_modified = 0;
    WebPCopyPixels(&src, &enc->canvas_carryover);
    enc->prev_rect.x_offset = in[13]; enc->prev_rect.y_offset = in[14];
    enc->prev_rect.width = in[15]; enc->prev_rect.height = in[16];
    if (enc->prev_rect.x_offset < 0 || enc->prev_rect.y_offset < 0 || enc->prev_rect.width < 0 ||
        enc->prev_rect.height < 0 || (int64_t)in[13] + in[15] > src.width ||
        (int64_t)in[14] + in[16] > src.height) abort();
    enc->is_first_frame = (flags & 2) != 0;
    enc->prev_candidate_undecided = (flags & 4) != 0;
    memset(enc->candidate_carryover_mask, 0, src.width * src.height);
    memset(enc->best_candidate_carryover_mask, 0, src.width * src.height);
    encoded = GetFrame(enc, 1);
    if (SetFrame(enc, &config, key, &best_rect, encoded, &skipped) != VP8_ENC_OK) abort();
    out[at++] = !skipped;
    if (!skipped) {
      info = key ? &encoded->key_frame : &encoded->sub_frame;
      at += AnimationCandidateOutput(&best_rect, info,
              key ? 0 : 1 + GetPreviousDisposeMethod(enc),
              !key && enc->best_candidate_carries_over,
              enc->best_candidate_carryover_mask, out + at);
    }
    WebpopIncludedAnimDelete(enc);
  } else { abort(); }
  WebPPictureFree(&view);
  free(mask); free(pixels);
  return at;
}

// Output-only mux oracle: use exactly the supplied still payloads. Raw pixels
// are carried in the logical request solely for the Rust evaluator; C does not
// encode them, so this comparison isolates the mux from codec differences.
static void AnimationMuxData(const uint32_t* in, unsigned n, WebPData* result) {
  unsigned at = 5, i, j, frames;
  uint8_t* metadata[3] = {NULL, NULL, NULL};
  const char* const tags[3] = {"ICCP", "EXIF", "XMP "};
  WebPMuxAnimParams params;
  WebPMux* mux = WebPMuxNew();
  if (n < 6 || mux == NULL || in[4] > 7) abort();
  if (WebPMuxSetCanvasSize(mux, in[0], in[1]) != WEBP_MUX_OK) abort();
  params.bgcolor = in[2]; params.loop_count = in[3];
  if (WebPMuxSetAnimationParams(mux, &params) != WEBP_MUX_OK) abort();
  for (i = 0; i < 3; ++i) {
    if (in[4] & (1u << i)) {
      WebPData data;
      unsigned len;
      if (at >= n) abort();
      len = in[at++];
      if (len > n - at) abort();
      // Retain a non-null byte pointer for present, empty metadata too.
      metadata[i] = (uint8_t*)malloc(len ? len : 1);
      if (!metadata[i]) abort();
      for (j = 0; j < len; ++j) metadata[i][j] = in[at++];
      data.bytes = metadata[i]; data.size = len;
      if (WebPMuxSetChunk(mux, tags[i], &data, 0) != WEBP_MUX_OK) abort();
    }
  }
  if (at >= n) abort();
  frames = in[at++];
  for (i = 0; i < frames; ++i) {
    const uint32_t* f;
    uint8_t* bytes;
    WebPMuxFrameInfo info;
    unsigned len;
    if (n - at < 10) abort();
    f = in + at; at += 10;
    if (f[8] > n - at) abort();
    at += f[8];  // Skip raw RGBA.
    len = f[9];
    if (len > n - at || f[7] > 3) abort();
    bytes = (uint8_t*)malloc(len);
    if (!bytes) abort();
    for (j = 0; j < len; ++j) bytes[j] = in[at++];
    memset(&info, 0, sizeof(info));
    info.bitstream.bytes = bytes; info.bitstream.size = len;
    info.id = WEBP_CHUNK_ANMF;
    info.x_offset = f[4]; info.y_offset = f[5]; info.duration = f[6];
    info.blend_method = (f[7] & 2) ? WEBP_MUX_NO_BLEND : WEBP_MUX_BLEND;
    info.dispose_method = (f[7] & 1) ? WEBP_MUX_DISPOSE_BACKGROUND : WEBP_MUX_DISPOSE_NONE;
    if (WebPMuxPushFrame(mux, &info, 1) != WEBP_MUX_OK) abort();
    free(bytes);
  }
  if (at != n || WebPMuxAssemble(mux, result) != WEBP_MUX_OK) abort();
  WebPMuxDelete(mux);
  for (i = 0; i < 3; ++i) free(metadata[i]);
}

unsigned WebpopAnimationMux(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned i;
  WebPData result;
  WebPDataInit(&result);
  AnimationMuxData(in, n, &result);
  if (result.size > (1u << 20)) abort();
  for (i = 0; i < result.size; ++i) out[i] = result.bytes[i];
  i = (unsigned)result.size;
  WebPDataClear(&result);
  return i;
}

// Reference-only completion gate. This invokes the original decode-dependent
// OptimizeSingleFrame, not a Rust parity evaluator. Store before and after so
// portable tests demonstrate exactly which branch remains unsupported.
unsigned WebpopAnimationSingleFrame(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned i, at = 0;
  WebPAnimEncoderOptions options;
  WebPAnimEncoder* enc;
  WebPData result;
  if (n < 6 || !WebpopIncludedAnimOptionsInit(&options, WEBP_MUX_ABI_VERSION)) abort();
  enc = WebpopIncludedAnimNew(in[0], in[1], &options, WEBP_MUX_ABI_VERSION);
  if (!enc || !WebPConfigInit(&enc->last_config)) abort();
  enc->last_config.lossless = 1;
  enc->last_config.quality = 70.f;
  enc->last_config.method = 4;
  enc->out_frame_count = 1;
  AnimationMuxData(in, n, &result);
  if (result.size > (1u << 19) - 1) abort();
  out[at++] = result.size;
  for (i = 0; i < result.size; ++i) out[at++] = result.bytes[i];
  if (OptimizeSingleFrame(enc, &result) != WEBP_MUX_OK || result.size > (1u << 19) - 1) abort();
  out[at++] = result.size;
  for (i = 0; i < result.size; ++i) out[at++] = result.bytes[i];
  WebPDataClear(&result);
  WebpopIncludedAnimDelete(enc);
  return at;
}
