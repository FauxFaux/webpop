// Access the pinned static predictor helpers without copying their algorithms.
#define VP8LOptimizeSampling WebpopIncludedOptimizeSampling
#define VP8LResidualImage WebpopIncludedResidualImage
#define VP8LColorSpaceTransform WebpopIncludedColorSpaceTransform
#include "src/enc/predictor_enc.c"

unsigned WebpopPredictor(const uint32_t* in, unsigned n, uint32_t* out) {
  unsigned count = 0, i;
  const unsigned op = in[0];
  if (op == 0) {
    int min_mode;
    if (n < 4) abort();
    out[count++] = GetPlaneDiffMask(in + 3, n - 3, 1, in[1], in[2], &min_mode);
    out[count++] = min_mode;
  } else if (op == 1) {
    int64_t cost;
    if (n != 2053) abort();
    cost = PredictionCostSpatialHistogram(in + 1029, in + 5, in[1], in[2], in[3], in[4]);
    out[count++] = (uint32_t)cost;
    out[count++] = (uint32_t)((uint64_t)cost >> 32);
  } else if (op == 2) {
    const unsigned w = in[1], h = in[2], bits = in[3];
    const unsigned map_size = VP8LSubSampleSize(w, bits) * VP8LSubSampleSize(h, bits);
    uint32_t* pixels = (uint32_t*)malloc(w * h * sizeof(*pixels));
    uint32_t* scratch = (uint32_t*)calloc(4 * (w + 1), sizeof(*scratch));
    if (n != 6 + map_size + w * h || !pixels || !scratch) abort();
    memcpy(pixels, in + 6 + map_size, w * h * sizeof(*pixels));
    CopyImageWithPrediction(w, h, bits, in + 6, scratch, pixels, in[4], 1, in[5], 0);
    for (i = 0; i < w * h; ++i) out[count++] = pixels[i];
    free(pixels); free(scratch);
  } else if (op == 3) {
    const unsigned w = in[1], num = in[6];
    uint32_t* rows = (uint32_t*)malloc(2 * (w + 1) * sizeof(*rows));
    uint8_t* diffs = (uint8_t*)calloc(w, 1);
    if (n != 7 + 2 * (w + 1) || !rows || !diffs) abort();
    memcpy(rows, in + 7, 2 * (w + 1) * sizeof(*rows));
    GetResidual(w, in[4] + 2, rows + w + 1, rows, diffs, in[2], in[3], in[3] + num,
                in[4], 1, in[5], 0, out);
    count = num;
    for (i = 0; i < 2 * (w + 1); ++i) out[count++] = rows[i];
    free(rows); free(diffs);
  } else if (op == 4) {
    if (n < 1026) abort();
    memcpy(out, in + 2, 1024 * sizeof(*out));
    UpdateHisto(out, in + 1026, n - 1026, in[1]);
    count = 1024;
  } else if (op == 5) {
    int64_t cost;
    if (n != 259) abort();
    cost = PredictionCostBias(in + 3, in[1], in[2]);
    out[count++] = (uint32_t)cost;
    out[count++] = (uint32_t)((uint64_t)cost >> 32);
  } else if (op == 6 || op == 7) {
    const unsigned w = in[1], h = in[2], min_bits = in[3], max_bits = in[4];
    const unsigned map_size = VP8LSubSampleSize(w, min_bits) * VP8LSubSampleSize(h, min_bits);
    uint32_t* pixels = (uint32_t*)malloc(w * h * sizeof(*pixels));
    uint32_t* scratch = (uint32_t*)calloc(4 * (w + 1), sizeof(*scratch));
    uint32_t* map = (uint32_t*)malloc(map_size * sizeof(*map));
    WebPPicture pic;
    int percent = 0, bits = 0;
    if (n != 7 + w * h || !pixels || !scratch || !map || !WebPPictureInit(&pic)) abort();
    memcpy(pixels, in + 7, w * h * sizeof(*pixels));
    if (op == 6) {
      if (!WebpopIncludedResidualImage(w, h, min_bits, max_bits, in[5], pixels,
              scratch, map, 100, in[6], 0, &pic, 0, &percent, &bits)) abort();
    } else {
      uint32_t* modes[MAX_TRANSFORM_BITS + 1];
      uint32_t* best_mode;
      int b, min_mode;
      const uint32_t diff_mask = GetPlaneDiffMask(pixels, w, h, 1, in[6], &min_mode);
      for (b = min_bits; b <= (int)max_bits; ++b) {
        modes[b - min_bits] = (uint32_t*)calloc(map_size, sizeof(*map));
        if (!modes[b - min_bits]) abort();
      }
      GetBestPredictorsAndSubSampling(w, h, min_bits, max_bits, scratch, pixels,
          1, in[6], 0, min_mode, diff_mask, &pic, 0, &percent, modes, &bits, &best_mode);
      if (!bits) abort();
      memcpy(map, best_mode, VP8LSubSampleSize(w, bits) * VP8LSubSampleSize(h, bits) * sizeof(*map));
      out[count++] = bits;
      for (b = min_bits; b <= (int)max_bits; ++b) free(modes[b - min_bits]);
    }
    if (op == 6) out[count++] = bits;
    for (i = 0; i < (unsigned)(VP8LSubSampleSize(w, bits) * VP8LSubSampleSize(h, bits)); ++i) out[count++] = map[i];
    if (op == 6) for (i = 0; i < w * h; ++i) out[count++] = pixels[i];
    WebPPictureFree(&pic);
    free(pixels); free(scratch); free(map);
  } else { abort(); }
  return count;
}
