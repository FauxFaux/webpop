// Copied verbatim (apart from this provenance header) from quad-image's
// webp-wasm/near-lossless.c at 1ee887300721ad754c5632b56349d5daad17c11e.
// Copyright (c) 2017 Chris West.
// Relicensed with the author's permission under the repository's BSD-3-Clause
// license in libwebpop/COPYING.
// Test-only wrapper, linked to the normally built pinned libwebp library.
// Lossless encoding with optional near-lossless pixel preprocessing.
#include <webp/encode.h>

size_t WebPEncodeRGBAWithNearLossless(const uint8_t* rgba, int width, int height,
                                     int stride, int near_lossless,
                                     uint8_t** output) {
  WebPConfig config;
  WebPPicture picture;
  WebPMemoryWriter writer;
  int ok;

  if (output == NULL) return 0;
  *output = NULL;
  // Match WebPEncodeLosslessRGBA's preset and RGBA import behavior.
  if (!WebPConfigPreset(&config, WEBP_PRESET_DEFAULT, 70.f)) return 0;
  config.lossless = 1;
  config.near_lossless = near_lossless;
  if (!WebPValidateConfig(&config) || !WebPPictureInit(&picture)) return 0;

  picture.use_argb = 1;
  picture.width = width;
  picture.height = height;
  picture.writer = WebPMemoryWrite;
  picture.custom_ptr = &writer;
  WebPMemoryWriterInit(&writer);
  ok = WebPPictureImportRGBA(&picture, rgba, stride) &&
       WebPEncode(&config, &picture);
  WebPPictureFree(&picture);
  if (!ok) {
    WebPMemoryWriterClear(&writer);
    return 0;
  }
  // Ownership passes to the caller, which releases it with WebPFree.
  *output = writer.mem;
  return writer.size;
}
