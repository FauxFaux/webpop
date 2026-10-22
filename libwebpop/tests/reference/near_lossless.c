// Test-only access to the pinned standalone near-lossless implementation.
// Rename the external entry point so public encoding keeps using the normally
// built reference library. No algorithm is copied into this adapter.
#define VP8ApplyNearLossless WebpopIncludedVP8ApplyNearLossless
#include "src/enc/near_lossless_enc.c"
#include <limits.h>
#include <stdio.h>

#define NL_REQUIRE(x) do { if (!(x)) { fprintf(stderr, "invalid near-lossless request: %s\n", name); abort(); } } while (0)
#define NL_IS(x) (strcmp(name, #x) == 0)

int WebpopNearLossless(const char* name, const uint32_t* in, unsigned n,
                      uint32_t* out, unsigned* count) {
  *count = 1;
  if (NL_IS(FindClosestDiscretized) || NL_IS(ClosestDiscretizedArgb)) {
    NL_REQUIRE(n == 2 && in[1] >= 1 && in[1] <= 5);
    if (NL_IS(FindClosestDiscretized)) {
      NL_REQUIRE(in[0] <= 255);
      out[0] = FindClosestDiscretized(in[0], (int)in[1]);
    } else {
      out[0] = ClosestDiscretizedArgb(in[0], (int)in[1]);
    }
  } else if (NL_IS(IsNear)) {
    NL_REQUIRE(n == 3 && in[2] >= 1 && in[2] <= 32);
    out[0] = IsNear(in[0], in[1], (int)in[2]);
  } else if (NL_IS(IsSmooth)) {
    // Center, left, right, up, down, limit. Only initialized neighbors matter.
    uint32_t prev[3] = {0}, curr[3], next[3] = {0};
    NL_REQUIRE(n == 6 && in[5] >= 1 && in[5] <= 32);
    curr[1] = in[0]; curr[0] = in[1]; curr[2] = in[2];
    prev[1] = in[3]; next[1] = in[4];
    out[0] = IsSmooth(prev, curr, next, 1, (int)in[5]);
  } else if (NL_IS(NearLossless) || NL_IS(VP8ApplyNearLossless)) {
    const int pass = NL_IS(NearLossless);
    const unsigned start = pass ? 5 : 4;
    uint64_t required;
    unsigned width, height, stride;
    NL_REQUIRE(n >= start);
    width = in[0]; height = in[1]; stride = in[2];
    NL_REQUIRE(width >= 1 && width <= 16383 && height >= 1 && height <= 16383);
    NL_REQUIRE(stride >= width && stride <= INT_MAX);
    required = (uint64_t)(height - 1) * stride + width;
    NL_REQUIRE(required == n - start && (uint64_t)width * height <= (1u << 20));
    if (pass) {
      uint32_t* scratch;
      NL_REQUIRE(height >= 2 && in[3] >= 1 && in[3] <= 5 && in[4] <= 1);
      NL_REQUIRE(!in[4] || stride == width);
      scratch = (uint32_t*)calloc(3 * width, sizeof(*scratch));
      NL_REQUIRE(scratch != NULL);
      if (in[4]) memcpy(out, in + start, width * height * sizeof(*out));
      NearLossless((int)width, (int)height, in[4] ? out : in + start,
                   (int)stride, (int)in[3], scratch, out);
      free(scratch);
    } else {
      WebPPicture picture;
      NL_REQUIRE(in[3] <= 99 && WebPPictureInit(&picture));
      picture.width = (int)width;
      picture.height = (int)height;
      picture.argb_stride = (int)stride;
      // The pinned function reads, but does not mutate, picture.argb.
      picture.argb = (uint32_t*)(in + start);
      NL_REQUIRE(WebpopIncludedVP8ApplyNearLossless(&picture, (int)in[3], out));
    }
    *count = width * height;
  } else {
    return 0;
  }
  return 1;
}
