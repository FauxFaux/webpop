// Test-only access to static helpers in the original pinned source. Rename
// external entry points so the library's normal encoder remains the public
// oracle. No copies of the serialization algorithm live in this adapter.
#define VP8LEncodeStream WebpopIncludedVP8LEncodeStream
#define VP8LEncodeImage WebpopIncludedVP8LEncodeImage
#include "src/enc/vp8l_enc.c"

// Prepared ARGB inputs: width, height, stride, then operation-specific settings.
unsigned WebpopAnalysis(const char* name, const uint32_t* in, uint32_t* out) {
  unsigned count = 0, i, j;
  if (strcmp(name, "AnalysisSizing") == 0) {
    const int bits = GetHistoBits(in[2], in[3], in[0], in[1]);
    out[count++] = bits;
    out[count++] = GetTransformBits(in[2], bits);
  } else if (strcmp(name, "AnalyzeEntropy") == 0) {
    EntropyIx mode;
    int original_zero, transformed_zero;
    if (!AnalyzeEntropy(in + 5 + in[4], in[0], in[1], in[2], in[4] != 0,
                        in[4], in + 5, in[3], &mode, &original_zero,
                        &transformed_zero)) abort();
    out[count++] = mode;
    out[count++] = original_zero;
    out[count++] = transformed_zero;
  } else {
    VP8LEncoder enc;
    WebPPicture pic;
    WebPConfig config;
    // vp8l_enc.c undefines CRUNCH_CONFIGS_MAX at the end of the file.
    CrunchConfig configs[kNumEntropyIx + 2 * kPaletteSortingNum];
    int size, transformed_zero = 0;
    memset(&enc, 0, sizeof(enc));
    if (!WebPPictureInit(&pic) || !WebPConfigInit(&config)) abort();
    pic.width = in[0]; pic.height = in[1]; pic.argb_stride = in[2];
    pic.argb = (uint32_t*)(in + 5); pic.use_argb = 1;
    config.method = in[3]; config.quality = in[4];
    enc.pic = &pic; enc.config = &config;
    if (!EncoderAnalyze(&enc, configs, &size, &transformed_zero)) abort();
    out[count++] = enc.palette_size;
    for (i = 0; i < (unsigned)enc.palette_size; ++i) out[count++] = enc.palette_sorted[i];
    out[count++] = enc.histo_bits;
    out[count++] = enc.predictor_transform_bits;
    out[count++] = enc.cross_color_transform_bits;
    out[count++] = transformed_zero;
    out[count++] = size;
    for (i = 0; i < (unsigned)size; ++i) {
      out[count++] = configs[i].entropy_idx;
      out[count++] = configs[i].palette_sorting_type;
      out[count++] = configs[i].sub_configs_size;
      for (j = 0; j < (unsigned)configs[i].sub_configs_size; ++j) {
        out[count++] = configs[i].sub_configs[j].lz77;
        out[count++] = configs[i].sub_configs[j].do_no_cache;
      }
    }
  }
  return count;
}

// width, height, stride, method, quality, exact, prefix bits, prepared ARGB.
unsigned WebpopEncodeStream(const uint32_t* in, uint32_t* out) {
  WebPPicture pic;
  WebPConfig config;
  WebPAuxStats stats;
  VP8LBitWriter bw;
  unsigned count = 0, remaining = in[6];
  size_t i, size;
  memset(&stats, 0, sizeof(stats));
  if (!WebPPictureInit(&pic) || !WebPConfigInit(&config) ||
      !VP8LBitWriterInit(&bw, 0)) abort();
  pic.width = in[0]; pic.height = in[1]; pic.argb_stride = in[2];
  pic.use_argb = 1; pic.argb = (uint32_t*)(in + 7); pic.stats = &stats;
  config.method = in[3]; config.quality = in[4]; config.exact = in[5];
  config.lossless = 1; config.near_lossless = 100; config.thread_level = 0;
  while (remaining) {
    const unsigned bits = remaining > 32 ? 32 : remaining;
    const uint32_t mask = bits == 32 ? 0xffffffffu : (1u << bits) - 1;
    VP8LPutBits(&bw, 0x05ad5ad5u & mask, bits);
    remaining -= bits;
  }
  if (!WebpopIncludedVP8LEncodeStream(&config, &pic, &bw)) abort();
  out[count++] = stats.lossless_features;
  out[count++] = stats.histogram_bits;
  out[count++] = stats.transform_bits;
  out[count++] = stats.cross_color_transform_bits;
  out[count++] = stats.cache_bits;
  out[count++] = stats.palette_size;
  out[count++] = stats.lossless_size;
  out[count++] = stats.lossless_hdr_size;
  out[count++] = stats.lossless_data_size;
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[count++] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  return count;
}

void WebpopStoreHuffmanCode(VP8LBitWriter* bw, const HuffmanTreeCode* code) {
  HuffmanTree* scratch = (HuffmanTree*)calloc(3 * (code->num_symbols + 19), sizeof(*scratch));
  HuffmanTreeToken* tokens = (HuffmanTreeToken*)calloc(code->num_symbols + 1, sizeof(*tokens));
  if (scratch == NULL || tokens == NULL) abort();
  StoreHuffmanCode(bw, scratch, tokens, code);
  free(scratch);
  free(tokens);
}

void WebpopClearHuffmanTree(HuffmanTreeCode* code) {
  ClearHuffmanTreeIfOnlyOneSymbol(code);
}

void WebpopWriteHuffmanCodeWithExtraBits(VP8LBitWriter* bw,
                                       const HuffmanTreeCode* code,
                                       int index, int bits, int n_bits) {
  WriteHuffmanCodeWithExtraBits(bw, code, index, bits, n_bits);
}

unsigned WebpopAuxiliary(const uint32_t* in, uint32_t* out) {
  VP8LBitWriter bw;
  VP8LHashChain chain;
  VP8LBackwardRefs refs[2];
  WebPPicture pic;
  int percent = 0;
  size_t i, size;
  if (!WebPPictureInit(&pic) || !VP8LBitWriterInit(&bw, 0) ||
      !VP8LHashChainInit(&chain, in[0] * in[1])) abort();
  for (i = 0; i < 2; ++i) VP8LBackwardRefsInit(&refs[i], 256);
  VP8LPutBits(&bw, in[4], in[5]);
  if (!EncodeImageNoHuffman(&bw, in + 6, &chain, refs, in[0], in[1],
                            in[2], in[3], &pic, 0, &percent)) abort();
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[i] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  VP8LHashChainClear(&chain);
  for (i = 0; i < 2; ++i) VP8LBackwardRefsClear(&refs[i]);
  return (unsigned)size;
}

// width, height, quality, low_effort, cache bound (0=automatic), histogram
// bits, prefix bit count, trial count, (lz77, do_no_cache) pairs, then ARGB.
unsigned WebpopMainImage(const uint32_t* in, unsigned n, uint32_t* out) {
  VP8LBitWriter bw;
  VP8LHashChain chain;
  VP8LBackwardRefs refs[4];
  CrunchConfig config;
  WebPPicture pic;
  int cache = in[4], hdr, data, percent = 0;
  unsigned i, remaining = in[6], count = 0;
  size_t size, initial;
  (void)n;
  memset(&config, 0, sizeof(config));
  config.sub_configs_size = in[7];
  for (i = 0; i < in[7]; ++i) {
    if (in[8 + 2 * i] < 1 || in[8 + 2 * i] > 7 || in[9 + 2 * i] > 1) abort();
    config.sub_configs[i].lz77 = in[8 + 2 * i];
    config.sub_configs[i].do_no_cache = in[9 + 2 * i];
  }
  if (!WebPPictureInit(&pic) || !VP8LBitWriterInit(&bw, 0) ||
      !VP8LHashChainInit(&chain, in[0] * in[1])) abort();
  for (i = 0; i < 4; ++i) VP8LBackwardRefsInit(&refs[i], 256);
  while (remaining) {
    const unsigned bits = remaining > 32 ? 32 : remaining;
    const uint32_t mask = bits == 32 ? 0xffffffffu : (1u << bits) - 1;
    VP8LPutBits(&bw, 0x05ad5ad5u & mask, bits);
    remaining -= bits;
  }
  initial = VP8LBitWriterNumBytes(&bw);
  if (!EncodeImageInternal(&bw, in + 8 + 2 * in[7], &chain, refs,
                            in[0], in[1], in[2], in[3], &config, &cache,
                            in[5], initial, &hdr, &data, &pic, 0, &percent)) abort();
  out[count++] = cache;
  out[count++] = hdr;
  out[count++] = data;
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[count++] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  VP8LHashChainClear(&chain);
  for (i = 0; i < 4; ++i) VP8LBackwardRefsClear(&refs[i]);
  return count;
}

unsigned WebpopImageHeaders(const uint32_t* in, uint32_t* out) {
  WebPPicture pic;
  WebPMemoryWriter memory;
  VP8LBitWriter bw;
  size_t coded_size, i;
  if (!WebPPictureInit(&pic) || !VP8LBitWriterInit(&bw, 0)) abort();
  WebPMemoryWriterInit(&memory);
  pic.writer = WebPMemoryWrite;
  pic.custom_ptr = &memory;
  pic.width = in[0];
  pic.height = in[1];
  if (!WriteImageSize(&pic, &bw) || !WriteRealAlphaAndVersion(&bw, in[2])) abort();
  VP8LPutBits(&bw, in[3], in[4]);
  if (!WriteImage(&pic, &bw, &coded_size) || coded_size != memory.size) abort();
  for (i = 0; i < memory.size; ++i) out[i] = memory.mem[i];
  i = memory.size;
  WebPMemoryWriterClear(&memory);
  VP8LBitWriterWipeOut(&bw);
  return (unsigned)i;
}

extern void VP8LBackwardRefsCursorAdd(VP8LBackwardRefs*, PixOrCopy);

unsigned WebpopTokenStream(const uint32_t* in, unsigned n, uint32_t* out) {
  const unsigned width = in[0], bits = in[2], groups = in[4], tiles = in[6];
  const uint32_t* symbols = in + 7;
  const unsigned tile_width = bits ? VP8LSubSampleSize(width, bits) : 1;
  VP8LHistogramSet* histograms = VP8LAllocateHistogramSet(groups, in[3]);
  VP8LBackwardRefs refs, *group_refs = calloc(groups, sizeof(*group_refs));
  HuffmanTreeCode* codes = calloc(5 * groups, sizeof(*codes));
  VP8LBitWriter bw;
  WebPPicture pic;
  unsigned i, j, position = 0, count = 0;
  if (!histograms || !group_refs || !codes || !WebPPictureInit(&pic) || !VP8LBitWriterInit(&bw, 0)) abort();
  VP8LHistogramSetClear(histograms);
  VP8LBackwardRefsInit(&refs, 256);
  for (i = 0; i < groups; ++i) VP8LBackwardRefsInit(group_refs + i, 256);
  for (i = 7 + tiles; i < n; i += 3) {
    PixOrCopy v = in[i] == 0 ? PixOrCopyCreateLiteral(in[i+1]) :
                 in[i] == 1 ? PixOrCopyCreateCacheIdx(in[i+1]) :
                              PixOrCopyCreateCopy(in[i+1], in[i+2]);
    unsigned tile = bits ? ((position / width) >> bits) * tile_width + ((position % width) >> bits) : 0;
    unsigned group = (symbols[tile] >> 8) & 0xffff;
    VP8LBackwardRefsCursorAdd(&refs, v);
    VP8LBackwardRefsCursorAdd(group_refs + group, v);
    position += PixOrCopyLength(&v);
  }
  if (position != width * in[1]) abort();
  for (i = 0; i < groups; ++i) VP8LHistogramStoreRefs(group_refs + i, NULL, 0, histograms->histograms[i]);
  if (!GetHuffBitLengthsAndCodes(histograms, codes)) abort();
  for (i = 0; i < 5 * groups; ++i) {
    out[count++] = codes[i].num_symbols;
    for (j = 0; j < (unsigned)codes[i].num_symbols; ++j)
      out[count++] = codes[i].code_lengths[j] | ((uint32_t)codes[i].codes[j] << 8);
  }
  VP8LPutBits(&bw, 5 & ((1u << in[5]) - 1), in[5]);
  for (i = 0; i < 5 * groups; ++i) {
    WebpopStoreHuffmanCode(&bw, codes + i);
    ClearHuffmanTreeIfOnlyOneSymbol(codes + i);
  }
  if (!StoreImageToBitMask(&bw, width, bits, &refs, symbols, codes, &pic)) abort();
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  for (i = 0; i < VP8LBitWriterNumBytes(&bw); ++i) out[count++] = bw.buf[i];
  WebPSafeFree(codes[0].codes);
  free(codes);
  VP8LFreeHistogramSet(histograms);
  for (i = 0; i < groups; ++i) VP8LBackwardRefsClear(group_refs + i);
  free(group_refs);
  VP8LBackwardRefsClear(&refs);
  VP8LBitWriterWipeOut(&bw);
  return count;
}

// palette size, width, height, source stride, low effort, prefix bits,
// palette words, then positive-stride ARGB (no last-row padding).
unsigned WebpopPaletteTransform(const uint32_t* in, uint32_t* out) {
  VP8LEncoder enc;
  WebPPicture pic;
  VP8LBitWriter bw;
  unsigned i, count = 0, remaining = in[5];
  size_t size;
  memset(&enc, 0, sizeof(enc));
  if (!WebPPictureInit(&pic) || !VP8LBitWriterInit(&bw, 0)) abort();
  pic.use_argb = 1; pic.width = in[1]; pic.height = in[2];
  pic.argb_stride = in[3]; pic.argb = (uint32_t*)(in + 6 + in[0]);
  enc.pic = &pic; enc.palette_size = in[0];
  memcpy(enc.palette, in + 6, in[0] * sizeof(uint32_t));
  VP8LEncDspInit();
  if (!VP8LHashChainInit(&enc.hash_chain, in[0])) abort();
  for (i = 0; i < 2; ++i) VP8LBackwardRefsInit(&enc.refs[i], 256);
  if (!MapImageFromPalette(&enc)) abort();
  out[count++] = enc.current_width;
  for (i = 0; i < enc.current_width * in[2]; ++i) out[count++] = enc.argb[i];
  while (remaining) {
    const unsigned bits = remaining > 32 ? 32 : remaining;
    const uint32_t mask = bits == 32 ? 0xffffffffu : (1u << bits) - 1;
    VP8LPutBits(&bw, 0x05ad5ad5u & mask, bits);
    remaining -= bits;
  }
  {
    int percent = 0;
    if (!EncodePalette(&bw, in[4], &enc, 0, &percent)) abort();
  }
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[count++] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  VP8LHashChainClear(&enc.hash_chain);
  for (i = 0; i < 2; ++i) VP8LBackwardRefsClear(&enc.refs[i]);
  ClearTransformBuffer(&enc);
  return count;
}

int WebpopClampBits(const uint32_t* in) {
  return ClampBits(in[0], in[1], (int32_t)in[2], in[3], in[4], in[5]);
}

// width, height, requested bits, method, quality, low effort, exact, prefix
// bit count, then prepared ARGB. Return selected bits, residuals, stream bytes.
unsigned WebpopPredictorTransform(const uint32_t* in, uint32_t* out) {
  VP8LEncoder enc;
  WebPPicture pic;
  WebPConfig config;
  VP8LBitWriter bw;
  unsigned i, count = 0, remaining = in[7];
  const unsigned pixel_count = in[0] * in[1];
  int percent = 0, bits = 0;
  size_t size;
  memset(&enc, 0, sizeof(enc));
  if (!WebPPictureInit(&pic) || !WebPConfigInit(&config) ||
      !VP8LBitWriterInit(&bw, 0)) abort();
  config.method = in[3]; config.exact = in[6]; config.near_lossless = 100;
  pic.width = in[0]; pic.height = in[1];
  enc.pic = &pic; enc.config = &config;
  enc.predictor_transform_bits = (int32_t)in[2];
  enc.argb = (uint32_t*)malloc(pixel_count * sizeof(*enc.argb));
  enc.argb_scratch = (uint32_t*)calloc(4 * (in[0] + 1), sizeof(*enc.argb_scratch));
  enc.transform_data = (uint32_t*)malloc(pixel_count * sizeof(*enc.transform_data));
  if (!enc.argb || !enc.argb_scratch || !enc.transform_data ||
      !VP8LHashChainInit(&enc.hash_chain, pixel_count)) abort();
  memcpy(enc.argb, in + 8, pixel_count * sizeof(*enc.argb));
  for (i = 0; i < 2; ++i) VP8LBackwardRefsInit(&enc.refs[i], 256);
  while (remaining) {
    const unsigned num_bits = remaining > 32 ? 32 : remaining;
    const uint32_t mask = num_bits == 32 ? 0xffffffffu : (1u << num_bits) - 1;
    VP8LPutBits(&bw, 0x05ad5ad5u & mask, num_bits);
    remaining -= num_bits;
  }
  if (!ApplyPredictFilter(&enc, in[0], in[1], in[4], in[5], 0, &bw,
                          0, &percent, &bits)) abort();
  out[count++] = bits;
  for (i = 0; i < pixel_count; ++i) out[count++] = enc.argb[i];
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[count++] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  VP8LHashChainClear(&enc.hash_chain);
  for (i = 0; i < 2; ++i) VP8LBackwardRefsClear(&enc.refs[i]);
  free(enc.argb); free(enc.argb_scratch); free(enc.transform_data);
  return count;
}

// width, height, cross-color bits, quality, low effort, exact, preceding
// transforms (subtract-green=1, predictor=2), prefix bit count, then ARGB.
unsigned WebpopCrossColorTransform(const uint32_t* in, uint32_t* out) {
  VP8LEncoder enc;
  WebPPicture pic;
  WebPConfig config;
  VP8LBitWriter bw;
  unsigned i, count = 0, remaining = in[7];
  const unsigned pixel_count = in[0] * in[1];
  int percent = 0, bits = 0;
  size_t size;
  memset(&enc, 0, sizeof(enc));
  if (!WebPPictureInit(&pic) || !WebPConfigInit(&config) ||
      !VP8LBitWriterInit(&bw, 0)) abort();
  config.method = 6; config.exact = in[5]; config.near_lossless = 100;
  pic.width = in[0]; pic.height = in[1];
  enc.pic = &pic; enc.config = &config;
  enc.predictor_transform_bits = 6; enc.cross_color_transform_bits = in[2];
  enc.argb = (uint32_t*)malloc(pixel_count * sizeof(*enc.argb));
  enc.argb_scratch = (uint32_t*)calloc(4 * (in[0] + 1), sizeof(*enc.argb_scratch));
  enc.transform_data = (uint32_t*)malloc(pixel_count * sizeof(*enc.transform_data));
  if (!enc.argb || !enc.argb_scratch || !enc.transform_data ||
      !VP8LHashChainInit(&enc.hash_chain, pixel_count)) abort();
  memcpy(enc.argb, in + 8, pixel_count * sizeof(*enc.argb));
  for (i = 0; i < 2; ++i) VP8LBackwardRefsInit(&enc.refs[i], 256);
  while (remaining) {
    const unsigned num_bits = remaining > 32 ? 32 : remaining;
    const uint32_t mask = num_bits == 32 ? 0xffffffffu : (1u << num_bits) - 1;
    VP8LPutBits(&bw, 0x05ad5ad5u & mask, num_bits);
    remaining -= num_bits;
  }
  if (in[6] & 1) ApplySubtractGreen(&enc, in[0], in[1], &bw);
  if ((in[6] & 2) && !ApplyPredictFilter(&enc, in[0], in[1], in[3], in[4], in[6] & 1,
      &bw, 0, &percent, &bits)) abort();
  if (!ApplyCrossColorFilter(&enc, in[0], in[1], in[3], in[4], &bw, 0, &percent, &bits)) abort();
  out[count++] = bits;
  for (i = 0; i < pixel_count; ++i) out[count++] = enc.argb[i];
  VP8LPutBits(&bw, 0x5ad, 11);
  VP8LBitWriterFinish(&bw);
  size = VP8LBitWriterNumBytes(&bw);
  for (i = 0; i < size; ++i) out[count++] = bw.buf[i];
  VP8LBitWriterWipeOut(&bw);
  VP8LHashChainClear(&enc.hash_chain);
  for (i = 0; i < 2; ++i) VP8LBackwardRefsClear(&enc.refs[i]);
  free(enc.argb); free(enc.argb_scratch); free(enc.transform_data);
  return count;
}
