// Test-only access to static helpers in the original pinned source. Rename
// external entry points so the library's normal encoder remains the public
// oracle. No copies of the serialization algorithm live in this adapter.
#define VP8LEncodeStream WebpopIncludedVP8LEncodeStream
#define VP8LEncodeImage WebpopIncludedVP8LEncodeImage
#include "src/enc/vp8l_enc.c"

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
