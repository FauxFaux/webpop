// Test-only configurable lossy adapter. Complete files always use the normally
// linked pinned C encoder; observed/included loops are separate requests.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "src/webp/encode.h"

unsigned WebpopLossyMethodEncode(const uint32_t* in, unsigned n, uint32_t* out) {
  WebPConfig config;
  WebPPicture pic;
  WebPMemoryWriter writer;
  float quality;
  uint8_t* storage;
  unsigned i, count = 0;
  memcpy(&quality, in + 3, sizeof(quality));
  if (!WebPConfigPreset(&config, WEBP_PRESET_DEFAULT, quality)) return 0;
  config.alpha_quality = (int32_t)in[4];
  config.method = (int32_t)in[5];
  config.use_sharp_yuv = in[6];
  if (!WebPValidateConfig(&config)) return 0;
  if (!WebPPictureInit(&pic)) abort();
  storage = (uint8_t*)malloc(n - 7 + 1);
  if (!storage) abort();
  for (i = 7; i < n; ++i) storage[1 + i - 7] = in[i];
  pic.width = in[0];
  pic.height = in[1];
  // Ordinary conversion occurs during import. Sharp conversion needs ARGB
  // until WebPEncode routes it, as in the production Rust adapter.
  pic.use_argb = config.use_sharp_yuv;
  WebPMemoryWriterInit(&writer);
  pic.writer = WebPMemoryWrite;
  pic.custom_ptr = &writer;
  if (!WebPPictureImportRGBA(&pic, storage + 1, in[2]) ||
      !WebPEncode(&config, &pic) || writer.size > (1u << 20)) abort();
  for (i = 0; i < writer.size; ++i) out[count++] = writer.mem[i];
  for (i = 7; i < n; ++i) if (storage[1 + i - 7] != in[i]) abort();
  WebPMemoryWriterClear(&writer);
  WebPPictureFree(&pic);
  free(storage);
  return count;
}
