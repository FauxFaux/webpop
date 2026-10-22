# Repository Guidelines

## Library Layout & Entry Points

libwebp is a C library for WebP encoding, decoding, and container manipulation. Public installed headers are under `src/webp/`: `encode.h`, `decode.h`, `demux.h`, `mux.h`, `mux_types.h`, and `types.h`. Include them as `<webp/encode.h>`, etc. Codec implementations in `src/enc/`, `src/dec/`, `src/dsp/`, and `src/utils/` are internal, not stable integration APIs. `sharpyuv/` supplies color conversion; `examples/` contains command-line integrations, and `imageio/` contains their image-format helpers.

## Consumption Modes

Prefer direct library use. Follow `doc/building.md` for CMake or Autotools installation; Git-based Autotools builds require `autogen.sh` first. Installed CMake packages support `find_package(WebP)` and export targets including `WebP::webp` and `WebP::webpdemux`. pkg-config packages are `libwebp`, `libwebpdemux`, and `libwebpmux`; use their static-link metadata to include private dependencies such as `libsharpyuv`. Mux availability depends on build options, including `WEBP_BUILD_LIBWEBPMUX` for CMake.

For another runtime, reproduce the examples' public API calls through bindings. Replace CLI parsing, filesystem handling, and `imageio/` adapters with host equivalents. For codec feature porting, consult `doc/webp-container-spec.txt` and `doc/webp-lossless-bitstream-spec.txt` alongside internal source; those internals are reference implementation details.

## API & Data Flow

`WebPGetInfo` checks headers and obtains dimensions; it does not fully decode or validate an image. `WebPDecodeRGBA` consumes encoded bytes plus their length and returns packed RGBA pixels or NULL. Release its output with `WebPFree`. Simple `WebPEncodeRGBA` and `WebPEncodeLosslessRGBA` accept pixels, dimensions, and byte stride; they return encoded length (zero on failure) and allocated output, also released with `WebPFree`.

For configurable encoding, initialize `WebPConfig` and `WebPPicture`, check initialization results, and validate settings with `WebPValidateConfig`. Set picture dimensions and `use_argb = 1` before `WebPPictureImportRGBA` when preserving lossless RGB input. Initialize `WebPMemoryWriter`, assign `picture.writer = WebPMemoryWrite` and `picture.custom_ptr = &writer`, then call `WebPEncode`. Consume `writer.mem`/`writer.size`; inspect `picture.error_code` on failure. Release picture and writer allocations with `WebPPictureFree` and `WebPMemoryWriterClear`, including error paths.

`demux.h` exposes frame/metadata iteration and `WebPAnimDecoder`; `mux.h` exposes container editing and `WebPAnimEncoder`. Animation decoder input must outlive the decoder. Returned frame canvases belong to the decoder and expire on the next decode, reset, or deletion.

## Examples & Behavioral Evidence

`examples/cwebp.c` demonstrates encoding configuration and cleanup; `examples/dwebp.c` demonstrates advanced decoding. `tests/fuzzer/enc_dec_fuzzer.cc` checks dimensions and lossless pixel equality. Transparent RGB is preserved only with `config.exact = 1`; simple lossless APIs disable this by default. Fuzzer helpers are test-only.

## Versions & Compatibility

`configure.ac` declares 1.6.0; this checkout is commit `a1d89ff209ca01e7a87aca64317201890bac2749`, after tag `v1.6.0`. Consult `NEWS` and `ChangeLog` for compatibility changes. Header initialization wrappers enforce ABI compatibility; use matching headers and libraries. Review `COPYING` and `PATENTS` when redistributing.
