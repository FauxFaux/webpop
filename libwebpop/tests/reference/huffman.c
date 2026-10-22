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
