// Test-only binary transport for real images exceeding the logical word limit.
// The public wrapper still calls the normally built pinned scalar library.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "src/webp/encode.h"

size_t WebPEncodeRGBAWithNearLossless(const uint8_t*, int, int, int, int, uint8_t**);

int main(int argc, char** argv) {
  unsigned w, h, strength;
  size_t length, size;
  uint8_t *input, *saved, *output = NULL;
  FILE* file;
  if (argc != 6) return 1;  // input.rgba width height strength output.webp
  w = (unsigned)strtoul(argv[2], NULL, 10);
  h = (unsigned)strtoul(argv[3], NULL, 10);
  strength = (unsigned)strtoul(argv[4], NULL, 10);
  if (!w || w > 16383 || !h || h > 16383 || strength > 100) return 1;
  length = (size_t)w * h * 4;
  input = (uint8_t*)malloc(length);
  saved = (uint8_t*)malloc(length);
  if (!input || !saved) return 1;
  file = fopen(argv[1], "rb");
  if (!file || fread(input, 1, length, file) != length || fgetc(file) != EOF) return 1;
  fclose(file);
  memcpy(saved, input, length);
  size = WebPEncodeRGBAWithNearLossless(input, w, h, 4 * w, strength, &output);
  if (!size || memcmp(input, saved, length)) return 1;
  file = fopen(argv[5], "wb");
  if (!file || fwrite(output, 1, size, file) != size || fclose(file)) return 1;
  WebPFree(output);
  free(input);
  free(saved);
  return 0;
}
