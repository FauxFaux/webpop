// Manual encode-only benchmark. Normal linked pinned C, no instrumented leaves.
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "src/webp/encode.h"

static double Now(void) {
  struct timespec t;
  if (clock_gettime(CLOCK_MONOTONIC, &t)) abort();
  return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}

int main(int argc, char** argv) {
  int w, h, aq, method, runs, run;
  float quality;
  size_t size;
  uint8_t* rgba;
  WebPMemoryWriter first;
  FILE* file;
  if (argc != 9) return 2;
  w = atoi(argv[2]); h = atoi(argv[3]); quality = strtof(argv[4], NULL);
  aq = atoi(argv[5]); method = atoi(argv[6]); runs = atoi(argv[7]);
  if (w < 1 || w > 16383 || h < 1 || h > 16383 || runs < 1 || runs > 100) return 2;
  size = (size_t)w * h * 4;
  rgba = (uint8_t*)malloc(size);
  file = fopen(argv[1], "rb");
  if (!rgba || !file || fread(rgba, 1, size, file) != size || fgetc(file) != EOF) abort();
  fclose(file);
  WebPMemoryWriterInit(&first);
  printf("[");
  for (run = 0; run <= runs; ++run) {
    WebPConfig config;
    WebPPicture pic;
    WebPMemoryWriter writer;
    double elapsed, started = Now();
    if (!WebPConfigPreset(&config, WEBP_PRESET_DEFAULT, quality) || !WebPPictureInit(&pic)) abort();
    config.method = method; config.alpha_quality = aq;
    if (!WebPValidateConfig(&config)) abort();
    pic.width = w; pic.height = h;
    WebPMemoryWriterInit(&writer);
    pic.writer = WebPMemoryWrite; pic.custom_ptr = &writer;
    if (!WebPPictureImportRGBA(&pic, rgba, w * 4) || !WebPEncode(&config, &pic)) abort();
    WebPPictureFree(&pic);
    elapsed = Now() - started;
    if (run == 0) { WebPMemoryWriterClear(&writer); continue; }
    printf("%s%.9f", run > 1 ? "," : "", elapsed);
    if (first.mem == NULL) first = writer;
    else {
      if (writer.size != first.size || memcmp(writer.mem, first.mem, first.size)) abort();
      WebPMemoryWriterClear(&writer);
    }
  }
  puts("]");
  file = fopen(argv[8], "wb");
  if (!file || fwrite(first.mem, 1, first.size, file) != first.size || fclose(file)) abort();
  WebPMemoryWriterClear(&first);
  free(rgba);
  return 0;
}
