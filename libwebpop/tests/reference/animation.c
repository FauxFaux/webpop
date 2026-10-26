// Access the original animation leaves. Rename all external definitions so the
// normally linked webpmux encoder remains available for future file comparisons.
#include "src/webp/encode.h"
static int AnimationObservedEncode(const WebPConfig*, WebPPicture*);
#define WebPEncode AnimationObservedEncode
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
#undef WebPEncode
#include "src/webp/demux.h"

// Observe actual codec invocations from the original GenerateCandidates. No
// selection/eligibility algorithm is duplicated in this test instrumentation.
static int animation_observe;
static unsigned animation_trial_count;
static uint32_t animation_trials[8][12];
static WebPData animation_trial_data[8];

static void AnimationObserveBegin(int enabled) {
  unsigned i;
  for (i = 0; i < 8; ++i) WebPDataClear(&animation_trial_data[i]);
  animation_trial_count = 0; animation_observe = enabled;
}

static unsigned AnimationConfigOutput(const WebPConfig* config, uint32_t* out) {
  out[0] = config->lossless;
  memcpy(out + 1, &config->quality, sizeof(config->quality));
  out[2] = config->method; out[3] = config->near_lossless;
  out[4] = config->exact; out[5] = config->alpha_quality;
  out[6] = config->use_sharp_yuv;
  return 7;
}

static int AnimationObservedEncode(const WebPConfig* config, WebPPicture* pic) {
  const int ok = WebPEncode(config, pic);
  if (animation_observe) {
    uint32_t* out;
    const WebPMemoryWriter* writer = (const WebPMemoryWriter*)pic->custom_ptr;
    if (!ok || animation_trial_count >= 8 || writer->size > (1u << 19)) abort();
    {
      const WebPData data = {writer->mem, writer->size};
      if (!WebPDataCopy(&data, &animation_trial_data[animation_trial_count])) abort();
    }
    out = animation_trials[animation_trial_count++];
    out[0] = config->lossless; out[1] = pic->width; out[2] = pic->height;
    memcpy(out + 3, &config->quality, sizeof(config->quality));
    out[4] = config->method; out[5] = config->near_lossless;
    out[6] = config->exact; out[7] = config->alpha_quality;
    out[8] = config->use_sharp_yuv; out[9] = config->filter_strength;
    out[10] = config->autofilter; out[11] = writer->size;
  }
  return ok;
}

static unsigned AnimationMixedState(const WebPAnimEncoder* enc, uint32_t* out,
                                    unsigned available) {
  unsigned i, j, at = 0;
  uint64_t needed = 15;
  for (i = 0; i < animation_trial_count; ++i) needed += 12 + animation_trial_data[i].size;
  if (needed > available) abort();
  at += AnimationConfigOutput(&enc->last_config, out + at);
  at += AnimationConfigOutput(&enc->last_config_reversed, out + at);
  out[at++] = animation_trial_count;
  for (i = 0; i < animation_trial_count; ++i) {
    for (j = 0; j < 12; ++j) out[at++] = animation_trials[i][j];
    for (j = 0; j < animation_trial_data[i].size; ++j) out[at++] = animation_trial_data[i].bytes[j];
    WebPDataClear(&animation_trial_data[i]);
  }
  return at;
}

// Original EncodeCandidate: exercise its filter override without duplicating it.
// Input: width, height, ARGB stride, x, y, quality bits, method,
// alpha quality, SharpYUV, use blending, then strided ARGB (no final padding).
unsigned WebpopAnimationLossyCandidate(const uint32_t* in, unsigned n,
                                       uint32_t* out) {
  WebPConfig config;
  WebPPicture pic;
  Candidate candidate;
  FrameRectangle rect;
  uint32_t* pixels;
  uint64_t size;
  float quality;
  unsigned i, at = 0;
  if (n < 10 || !in[0] || !in[1] || in[0] > 16383 || in[1] > 16383 ||
      in[2] < in[0] || in[2] > in[0] + 64 || in[3] >= (1u << 24) ||
      in[4] >= (1u << 24) || (in[3] & 1) || (in[4] & 1) ||
      in[8] > 1 || in[9] > 1) abort();
  size = (uint64_t)(in[1] - 1) * in[2] + in[0];
  if (size > (1u << 20) || n != 10 + size) abort();
  memcpy(&quality, in + 5, sizeof(quality));
  if (!WebPConfigPreset(&config, WEBP_PRESET_DEFAULT, quality)) return 0;
  config.method = (int32_t)in[6];
  config.alpha_quality = (int32_t)in[7];
  config.use_sharp_yuv = in[8];
  if (!WebPValidateConfig(&config)) return 0;
  if (!WebPPictureInit(&pic)) abort();
  pixels = (uint32_t*)malloc(size * sizeof(*pixels));
  if (!pixels) abort();
  memcpy(pixels, in + 10, size * sizeof(*pixels));
  pic.use_argb = 1; pic.width = in[0]; pic.height = in[1];
  pic.argb_stride = in[2]; pic.argb = pixels;
  rect.x_offset = in[3]; rect.y_offset = in[4];
  rect.width = in[0]; rect.height = in[1];
  if (EncodeCandidate(&pic, &rect, &config, in[9], &candidate) != VP8_ENC_OK ||
      candidate.mem.size > (1u << 20) - 8) abort();
  out[at++] = candidate.rect.x_offset; out[at++] = candidate.rect.y_offset;
  out[at++] = candidate.rect.width; out[at++] = candidate.rect.height;
  out[at++] = candidate.info.blend_method;
  out[at++] = candidate.info.duration; out[at++] = candidate.info.dispose_method;
  out[at++] = candidate.mem.size;
  for (i = 0; i < candidate.mem.size; ++i) out[at++] = candidate.mem.mem[i];
  WebPMemoryWriterClear(&candidate.mem);
  WebPPictureFree(&pic);
  free(pixels);
  return at;
}

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

static unsigned AnimationCandidates(const uint32_t* in, unsigned n, uint32_t* out,
                                    unsigned near_lossless, int lossy,
                                    unsigned alpha_quality, unsigned sharp, int mixed) {
  WebPPicture src, dst, view;
  FrameRectangle rect;
  WebPConfig config;
  unsigned src_size, dst_size, i, at = 0;
  uint32_t* pixels;
  uint8_t* mask;
  unsigned op, flags;
  if (n < 18) abort();
  op = in[0]; flags = in[9];
  if (!in[1] || !in[2] || in[1] > 96 || in[2] > 64 ||
      in[3] < in[1] || in[4] < in[1] || in[3] > 128 || in[4] > 128 ||
      flags > 63 || (!lossy && !mixed && in[10] > 100) || in[11] > 6 || in[12] > 1) abort();
  src_size = (in[2] - 1) * in[3] + in[1];
  dst_size = (in[2] - 1) * in[4] + in[1];
  if (n != 18 + src_size + dst_size + in[17] ||
      !WebPPictureInit(&src) || !WebPPictureInit(&dst) || !WebPPictureInit(&view) ||
      !WebPConfigInit(&config)) abort();
  pixels = (uint32_t*)malloc(dst_size * sizeof(*pixels));
  // FlattenSimilarBlocks forms grid-aligned row pointers even for empty/skinny
  // rectangles. Guard storage keeps those pointers inside this allocation;
  // observations still contain exactly the caller's original mask extent.
  mask = (uint8_t*)malloc((size_t)in[17] + 8 * in[1] + 16);
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
  config.lossless = !lossy; config.method = in[11]; config.exact = in[12];
  if (lossy || mixed) memcpy(&config.quality, in + 10, sizeof(config.quality));
  else config.quality = in[10];
  config.alpha_quality = alpha_quality; config.use_sharp_yuv = sharp;
  config.near_lossless = near_lossless;
  if (!WebPValidateConfig(&config)) abort();
  if (op == 0) {
    if (in[17] < (unsigned)(rect.width * rect.height)) abort();
    out[at++] = lossy ? FlattenSimilarBlocks(&src, &rect, &dst, config.quality, mask)
                       : IncreaseTransparency(&src, &rect, &dst, mask);
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
    options.allow_mixed = mixed;
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
    AnimationObserveBegin(mixed);
    if (SetFrame(enc, &config, key, &best_rect, encoded, &skipped) != VP8_ENC_OK) abort();
    animation_observe = 0;
    if (mixed) {
      unsigned future = 1;
      if (!skipped) {
        info = key ? &encoded->key_frame : &encoded->sub_frame;
        if (info->bitstream.size > (1u << 19)) abort();
        future += 9 + info->bitstream.size;
        if (!key && enc->best_candidate_carries_over) future += best_rect.width * best_rect.height;
      }
      if (future > (1u << 20) - at) abort();
      at += AnimationMixedState(enc, out + at, (1u << 20) - at - future);
    }
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

unsigned WebpopAnimationLossless(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCandidates(in, n, out, 100, 0, 100, 0, 0);
}

unsigned WebpopAnimationNearLossless(const uint32_t* in, unsigned n, uint32_t* out) {
  if (!n || in[0] > 100) abort();
  return AnimationCandidates(in + 1, n - 1, out, in[0], 0, 100, 0, 0);
}

// Lossy requests prefix the existing candidate layout with alpha quality and
// SharpYUV; color quality occupies the existing quality word as f32 bits.
unsigned WebpopAnimationLossy(const uint32_t* in, unsigned n, uint32_t* out) {
  if (n < 2 || in[0] > 100 || in[1] > 1) abort();
  return AnimationCandidates(in + 2, n - 2, out, 100, 1, in[0], in[1], 0);
}

// Prefix: primary lossless flag, near-lossless strength, alpha quality, SharpYUV.
unsigned WebpopAnimationMixed(const uint32_t* in, unsigned n, uint32_t* out) {
  if (n < 4 || in[0] > 1 || in[1] > 100 || in[2] > 100 || in[3] > 1) abort();
  return AnimationCandidates(in + 4, n - 4, out, in[1], !in[0], in[2], in[3], 1);
}

// Three separately strided canvases, including destination padding, and a
// packed byte mask. Call the original helpers; do not replicate their loops.
unsigned WebpopAnimationCarryover(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPPicture a, b, dst;
  unsigned sizes[3], i, at = 7;
  uint32_t* pixels;
  uint8_t* mask;
  if (n < 7 || in[0] > 1 || !in[1] || !in[2] || in[1] > 64 || in[2] > 64 ||
      in[3] < in[1] || in[4] < in[1] || in[5] < in[1] ||
      in[3] > 128 || in[4] > 128 || in[5] > 128 || in[6] < in[1] * in[2]) abort();
  for (i = 0; i < 3; ++i) sizes[i] = (in[2] - 1) * in[3 + i] + in[1];
  if (n != at + sizes[0] + sizes[1] + sizes[2] + in[6] ||
      !WebPPictureInit(&a) || !WebPPictureInit(&b) || !WebPPictureInit(&dst)) abort();
  a.use_argb = b.use_argb = dst.use_argb = 1;
  a.width = b.width = dst.width = in[1]; a.height = b.height = dst.height = in[2];
  a.argb_stride = in[3]; b.argb_stride = in[4]; dst.argb_stride = in[5];
  a.argb = (uint32_t*)(in + at); at += sizes[0];
  b.argb = (uint32_t*)(in + at); at += sizes[1];
  pixels = (uint32_t*)malloc(sizes[2] * sizeof(*pixels));
  mask = (uint8_t*)malloc(in[6]);
  if (!pixels || !mask) abort();
  memcpy(pixels, in + at, sizes[2] * sizeof(*pixels)); at += sizes[2];
  for (i = 0; i < in[6]; ++i) {
    if (in[at + i] > 255) abort();
    mask[i] = in[at + i];
  }
  dst.argb = pixels;
  if (in[0] == 0) CopyIdenticalPixels(&a, &b, &dst);
  else CopyMaskedPixels(&a, mask, &dst);
  memcpy(out, pixels, sizes[2] * sizeof(*out));
  free(mask); free(pixels);
  return sizes[2];
}

static unsigned AnimationCanvasOutput(const WebPPicture* pic, uint32_t* out) {
  int x, y;
  unsigned at = 0;
  for (y = 0; y < pic->height; ++y) {
    for (x = 0; x < pic->width; ++x) out[at++] = pic->argb[y * pic->argb_stride + x];
  }
  return at;
}

// Observe original CacheFrame canvas updates and selected candidates, then
// FlushFrames to retain its normal one-frame lookbehind. Durations remain zero:
// timing/merging and final assembly parity are outside this request's contract.
static unsigned AnimationCarryoverSequence(const uint32_t* in, unsigned n,
                                             uint32_t* out, int mode) {
  const int mixed = mode == 2;
  const int lossy = mode == 1;
  WebPAnimEncoderOptions options;
  WebPAnimEncoder* enc;
  unsigned frame, at = 4, written = 0;
  if (n < 4 || !in[0] || !in[1] || in[0] > 64 || in[1] > 64 ||
      in[2] > 1 || !in[3] || in[3] > 16 ||
      !WebpopIncludedAnimOptionsInit(&options, WEBP_MUX_ABI_VERSION)) abort();
  options.minimize_size = in[2]; options.allow_mixed = mixed;
  enc = WebpopIncludedAnimNew(in[0], in[1], &options, WEBP_MUX_ABI_VERSION);
  if (!enc) abort();
  for (frame = 0; frame < in[3]; ++frame) {
    WebPPicture current;
    WebPConfig config;
    const unsigned header = mixed ? 8 : lossy ? 5 : 4;
    unsigned size, count = enc->count;
    if (n - at < header || in[at] < in[0] || in[at] > 128 ||
        (!lossy && !mixed && in[at + 1] > 100) || in[at + 2] > 6 ||
        (!lossy && !mixed && in[at + 3] > 1) ||
        (lossy && (in[at + 3] > 100 || in[at + 4] > 1)) ||
        (mixed && (in[at + 3] > 1 || in[at + 4] > 100 || in[at + 5] > 1 || in[at + 6] > 100 || in[at + 7] > 1)) || !WebPPictureInit(&current) ||
        !WebPConfigInit(&config)) abort();
    current.use_argb = 1; current.width = in[0]; current.height = in[1];
    current.argb_stride = in[at];
    config.lossless = !lossy; config.method = in[at + 2];
    if (mixed) {
      memcpy(&config.quality, in + at + 1, sizeof(config.quality));
      config.lossless = in[at + 3]; config.near_lossless = in[at + 4];
      config.exact = in[at + 5]; config.alpha_quality = in[at + 6]; config.use_sharp_yuv = in[at + 7];
    } else if (lossy) {
      memcpy(&config.quality, in + at + 1, sizeof(config.quality));
      config.alpha_quality = in[at + 3]; config.use_sharp_yuv = in[at + 4];
    } else {
      config.quality = in[at + 1]; config.exact = in[at + 3];
    }
    if (!WebPValidateConfig(&config)) abort();
    at += header; size = (current.height - 1) * current.argb_stride + current.width;
    if (size > n - at) abort();
    current.argb = (uint32_t*)(in + at); at += size;
    enc->curr_canvas = &current; enc->curr_canvas_copy_modified = 1;
    CopyCurrentCanvas(enc);
    AnimationObserveBegin(mixed);
    if (!CacheFrame(enc, &config)) abort();
    animation_observe = 0;
    if (mixed) {
      unsigned future = 5 + 2 * in[0] * in[1];
      if (enc->count > count) {
        const EncodedFrame* encoded = GetFrame(enc, enc->count - 1);
        const WebPMuxFrameInfo* info = encoded->is_key_frame ? &encoded->key_frame : &encoded->sub_frame;
        if (info->bitstream.size > (1u << 19)) abort();
        future += 9 + info->bitstream.size;
        if (!encoded->is_key_frame && enc->best_candidate_carries_over) future += enc->prev_rect.width * enc->prev_rect.height;
      }
      if (future > (1u << 20) - written) abort();
      written += AnimationMixedState(enc, out + written, (1u << 20) - written - future);
    }
    out[written++] = enc->count > count;
    if (enc->count > count) {
      EncodedFrame* encoded = GetFrame(enc, enc->count - 1);
      const WebPMuxFrameInfo* info = encoded->is_key_frame ? &encoded->key_frame : &encoded->sub_frame;
      written += AnimationCandidateOutput(&enc->prev_rect, info,
          encoded->is_key_frame ? 0 : 1 + GetPreviousDisposeMethod(enc),
          !encoded->is_key_frame && enc->best_candidate_carries_over,
          enc->best_candidate_carryover_mask, out + written);
    }
    out[written++] = enc->prev_rect.x_offset; out[written++] = enc->prev_rect.y_offset;
    out[written++] = enc->prev_rect.width; out[written++] = enc->prev_rect.height;
    written += AnimationCanvasOutput(&enc->prev_canvas, out + written);
    written += AnimationCanvasOutput(&enc->canvas_carryover, out + written);
    if (!FlushFrames(enc)) abort();
  }
  if (at != n) abort();
  WebpopIncludedAnimDelete(enc);
  return written;
}

unsigned WebpopAnimationCarryoverSequence(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCarryoverSequence(in, n, out, 0);
}

unsigned WebpopAnimationLossyCarryoverSequence(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCarryoverSequence(in, n, out, 1);
}

unsigned WebpopAnimationMixedCarryoverSequence(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCarryoverSequence(in, n, out, 2);
}

static unsigned AnimationCacheStatus(const WebPAnimEncoder* enc, uint32_t* out) {
  out[0] = enc->in_frame_count; out[1] = enc->out_frame_count;
  out[2] = enc->count; out[3] = enc->count_since_key_frame;
  out[4] = enc->prev_rect.x_offset; out[5] = enc->prev_rect.y_offset;
  out[6] = enc->prev_rect.width; out[7] = enc->prev_rect.height;
  out[8] = enc->count != 0;
  out[9] = enc->count ? GetFrame(enc, enc->count - 1)->sub_frame.duration : 0;
  return 10;
}

// Extended v1 observations keep the original ten counter words as a prefix.
// After final assembly Rust releases canvases; omit them on both sides when
// no pending frame remains. Saved configs remain observable after finish.
static unsigned AnimationCacheExtended(const WebPAnimEncoder* enc, uint32_t* out) {
  unsigned at = 0, y, x;
  const WebPMuxFrameInfo* info = NULL;
  at += AnimationConfigOutput(&enc->last_config, out + at);
  at += AnimationConfigOutput(&enc->last_config_reversed, out + at);
  if (enc->count) {
    const EncodedFrame* f = GetFrame(enc, enc->count - 1);
    info = f->is_key_frame ? &f->key_frame : &f->sub_frame;
  }
  out[at++] = info ? info->x_offset : 0;
  out[at++] = info ? info->y_offset : 0;
  out[at++] = info ? info->dispose_method : 0;
  out[at++] = info ? info->blend_method : 0;
  out[at++] = enc->count ? enc->canvas_width * enc->canvas_height : 0;
  if (enc->count) {
    const WebPPicture* pictures[2] = {&enc->prev_canvas, &enc->canvas_carryover};
    unsigned p;
    for (p = 0; p < 2; ++p) for (y = 0; y < (unsigned)enc->canvas_height; ++y)
      for (x = 0; x < (unsigned)enc->canvas_width; ++x)
        out[at++] = pictures[p]->argb[y * pictures[p]->argb_stride + x];
  }
  return at;
}

// Effective policy and initial storage from the original constructor. Signed
// inputs are copied bit-for-bit, including INT_MIN/INT_MAX; verbose stays off.
unsigned WebpopAnimationKeyframeOptions(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPAnimEncoderOptions options;
  WebPAnimEncoder* enc;
  uint64_t penalty;
  if (n != 3 || in[2] > 1 || !WebpopIncludedAnimOptionsInit(&options, WEBP_MUX_ABI_VERSION)) abort();
  memcpy(&options.kmin, in, sizeof(options.kmin));
  memcpy(&options.kmax, in + 1, sizeof(options.kmax));
  options.minimize_size = in[2];
  enc = WebpopIncludedAnimNew(1, 1, &options, WEBP_MUX_ABI_VERSION);
  if (!enc) abort();
  out[0] = enc->options.kmin; out[1] = enc->options.kmax;
  out[2] = enc->size; out[3] = enc->start; out[4] = enc->count;
  out[5] = enc->flush_count;
  penalty = (uint64_t)enc->best_delta;
  out[6] = (uint32_t)penalty; out[7] = (uint32_t)(penalty >> 32);
  out[8] = enc->keyframe; out[9] = enc->count_since_key_frame;
  out[10] = enc->prev_candidate_undecided;
  WebpopIncludedAnimDelete(enc);
  return 11;
}

static unsigned AnimationAlternativeStatus(const WebPMuxFrameInfo* info, uint32_t* out) {
  int width = 0, height = 0;
  if (info->bitstream.size && !WebPGetInfo(info->bitstream.bytes, info->bitstream.size, &width, &height)) abort();
  out[0] = info->duration; out[1] = info->x_offset; out[2] = info->y_offset;
  out[3] = info->dispose_method; out[4] = info->blend_method;
  out[5] = width; out[6] = height; out[7] = info->bitstream.size;
  return 8;
}

// V2 shows both owned alternatives even if one has no encoded payload. Their
// duration fields can still differ (notably immediately after inserting a hold).
static unsigned AnimationCacheWindow(const WebPAnimEncoder* enc, uint32_t* out, unsigned available) {
  unsigned at, i;
  if (19 + 17 * enc->count > available) abort();
  at = AnimationCacheStatus(enc, out);
  const uint64_t penalty = (uint64_t)enc->best_delta;
  out[at++] = enc->options.kmin; out[at++] = enc->options.kmax;
  out[at++] = enc->size; out[at++] = enc->start; out[at++] = enc->flush_count;
  out[at++] = (uint32_t)penalty; out[at++] = (uint32_t)(penalty >> 32);
  out[at++] = enc->keyframe; out[at++] = enc->prev_candidate_undecided;
  for (i = 0; i < enc->count; ++i) {
    const EncodedFrame* frame = GetFrame(enc, i);
    out[at++] = frame->is_key_frame;
    at += AnimationAlternativeStatus(&frame->sub_frame, out + at);
    at += AnimationAlternativeStatus(&frame->key_frame, out + at);
  }
  return at;
}

// V3 appends the established V1 saved-config/canvas observations after the
// first submission. No selection logic is duplicated in this observer.
static unsigned AnimationCacheSnapshot(const WebPAnimEncoder* enc, uint32_t* out,
                                       unsigned available, int history) {
  unsigned at = AnimationCacheWindow(enc, out, available);
  if (history && enc->in_frame_count) {
    const uint64_t needed = 19 + (enc->count ? 2 * (uint64_t)enc->canvas_width * enc->canvas_height : 0);
    if (needed > available - at) abort();
    at += AnimationCacheExtended(enc, out + at);
  }
  return at;
}

static unsigned AnimationCacheTrials(uint32_t* out, unsigned available) {
  unsigned at = 0, i, j;
  uint64_t needed = 1;
  for (i = 0; i < animation_trial_count; ++i) needed += 12 + animation_trial_data[i].size;
  if (needed > available) abort();
  out[at++] = animation_trial_count;
  for (i = 0; i < animation_trial_count; ++i) {
    for (j = 0; j < 12; ++j) out[at++] = animation_trials[i][j];
    for (j = 0; j < animation_trial_data[i].size; ++j) out[at++] = animation_trial_data[i].bytes[j];
    WebPDataClear(&animation_trial_data[i]);
  }
  return at;
}

static unsigned AnimationFileOutput(const WebPData* data, uint32_t* out) {
  unsigned i;
  if (data->size > (1u << 19)) abort();
  out[0] = data->size;
  for (i = 0; i < data->size; ++i) out[i + 1] = data->bytes[i];
  return data->size + 1;
}

// Original public adds exercise timing and the capacity flush before CacheFrame.
// Mode 0 stops before decode-dependent OptimizeSingleFrame. Mode 1 runs the
// full public assemble. Mode 2 records both for a reference-only completion gate.
static unsigned AnimationCache(const uint32_t* in, unsigned n, uint32_t* out,
                               int near_lossless) {
  WebPAnimEncoderOptions options;
  WebPAnimEncoder* enc;
  WebPData data;
  uint32_t timestamp;
  unsigned frame, duration = 0, at = 8, written = 0;
  const int extended = n >= 8 && (in[2] & 4);
  const int window = n >= 8 && (in[2] & 8);
  if (n < 8 || !in[0] || !in[1] || in[0] > 96 || in[1] > 32 ||
      in[2] > 31 || ((in[2] & 16) && !window) || (!extended && in[2] > 1) || (window && n < 10) || in[3] > 2 || !in[4] || in[4] > 128 || in[5] > 1 ||
      !WebpopIncludedAnimOptionsInit(&options, WEBP_MUX_ABI_VERSION)) abort();
  options.minimize_size = in[2] & 1;
  options.allow_mixed = extended && (in[2] & 2);
  if (window) {
    memcpy(&options.kmin, in + 8, sizeof(options.kmin));
    memcpy(&options.kmax, in + 9, sizeof(options.kmax));
    at = 10;
  }
  if (in[5]) { options.anim_params.bgcolor = 0x4e0c2238; options.anim_params.loop_count = 65535; }
  enc = WebpopIncludedAnimNew(in[0], in[1], &options, WEBP_MUX_ABI_VERSION);
  if (!enc) abort();
  if (in[5]) {
    static const uint8_t icc[] = {1, 2, 3}, xmp[] = {4};
    // An empty WebPData copy loses its non-null presence; borrow static storage.
    const WebPData a = {icc, sizeof(icc)}, b = {icc, 0}, c = {xmp, sizeof(xmp)};
    if (WebPMuxSetChunk(enc->mux, "ICCP", &a, 1) != WEBP_MUX_OK ||
        WebPMuxSetChunk(enc->mux, "EXIF", &b, 0) != WEBP_MUX_OK ||
        WebPMuxSetChunk(enc->mux, "XMP ", &c, 1) != WEBP_MUX_OK) abort();
  }
  timestamp = in[6];  // High origin word is irrelevant to wrapping C intervals.
  for (frame = 0; frame < in[4]; ++frame) {
    WebPPicture current;
    WebPConfig config;
    unsigned size;
    int signed_timestamp;
    const unsigned header = extended ? 9 : near_lossless ? 6 : 5;
    if (n - at < header || in[at] >= (1u << 24) || in[at + 1] < in[0] || in[at + 1] > 128 ||
        (!extended && (in[at + 2] > 100 || in[at + 3] > 6 || in[at + 4] > 1)) ||
        !WebPPictureInit(&current) || !WebPConfigInit(&config)) abort();
    duration = in[at]; current.argb_stride = in[at + 1];
    config.lossless = 1; config.quality = in[at + 2]; config.method = in[at + 3]; config.exact = in[at + 4];
    if (extended) {
      config.lossless = in[at + 2];
      memcpy(&config.quality, in + at + 3, sizeof(config.quality));
      config.method = in[at + 4]; config.near_lossless = in[at + 5];
      config.exact = in[at + 6]; config.alpha_quality = in[at + 7];
      config.use_sharp_yuv = in[at + 8];
    } else if (near_lossless) {
      if (in[at + 5] > 100) abort();
      config.near_lossless = in[at + 5];
    }
    current.use_argb = 1; current.width = in[0]; current.height = in[1];
    at += header; size = (current.height - 1) * current.argb_stride + current.width;
    if (size > n - at) abort();
    current.argb = (uint32_t*)(in + at); at += size;
    memcpy(&signed_timestamp, &timestamp, sizeof(timestamp));
    if (window) {
      written += AnimationCacheSnapshot(enc, out + written, (1u << 20) - written, in[2] & 16);
      AnimationObserveBegin(1);
    }
    if (!WebpopIncludedAnimAdd(enc, &current, signed_timestamp, &config)) abort();
    animation_observe = 0;
    if (window) {
      written += AnimationCacheTrials(out + written, (1u << 20) - written);
      written += AnimationCacheSnapshot(enc, out + written, (1u << 20) - written, in[2] & 16);
    } else {
      written += AnimationCacheStatus(enc, out + written);
      if (extended) written += AnimationCacheExtended(enc, out + written);
    }
    timestamp += duration;
  }
  if (at != n) abort();
  if (window) written += AnimationCacheSnapshot(enc, out + written, (1u << 20) - written, in[2] & 16);
  {
    int signed_timestamp;
    memcpy(&signed_timestamp, &timestamp, sizeof(timestamp));
    if (!WebpopIncludedAnimAdd(enc, NULL, signed_timestamp, NULL)) abort();
  }
  WebPDataInit(&data);
  if (in[3] != 1) {
    enc->flush_count = enc->count;
    if (!FlushFrames(enc) ||
        WebPMuxSetCanvasSize(enc->mux, enc->canvas_width, enc->canvas_height) != WEBP_MUX_OK ||
        WebPMuxSetAnimationParams(enc->mux, &enc->options.anim_params) != WEBP_MUX_OK ||
        WebPMuxAssemble(enc->mux, &data) != WEBP_MUX_OK) abort();
  } else if (!WebpopIncludedAnimAssemble(enc, &data)) abort();
  if (window) written += AnimationCacheSnapshot(enc, out + written, (1u << 20) - written, in[2] & 16);
  else {
    written += AnimationCacheStatus(enc, out + written);
    if (extended) written += AnimationCacheExtended(enc, out + written);
  }
  if (window && data.size + 1 > (1u << 20) - written) abort();
  written += AnimationFileOutput(&data, out + written);
  WebPDataClear(&data);
  if (in[3] == 2) {
    if (!WebpopIncludedAnimAssemble(enc, &data)) abort();
    written += AnimationFileOutput(&data, out + written);
    WebPDataClear(&data);
  }
  WebpopIncludedAnimDelete(enc);
  return written;
}

unsigned WebpopAnimationCache(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCache(in, n, out, 0);
}

unsigned WebpopAnimationNearLosslessCache(const uint32_t* in, unsigned n, uint32_t* out) {
  return AnimationCache(in, n, out, 1);
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
