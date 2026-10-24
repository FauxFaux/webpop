`test_ref.ppm` is copied verbatim from `upstream/examples/test_ref.ppm` at
libwebp revision `1844042d1ca751e62e61df7617dba19c98588339`, under the
repository's upstream BSD license and patent terms. It is a 128×128 RGB P6 image.
SHA-256: `bf84f754ae60e8924b2a1416d9fd8f95e35d89742a691103ee1d7e013c21a723`.

`test_ref.webp` is the pinned scalar C `WebPEncodeLosslessRGBA` output for this
image, retained for portable byte comparisons. See the reference guide for
toolchain provenance and the `public_rgba_fixtures_match_pinned_reference`
regeneration command. Production Rust never invokes C or reads these fixtures.

`test_ref.lossy.webp` is the pinned scalar C `WebPEncodeRGBA` output at color
quality 75, default preset/method 4 and exact alpha (2,074 bytes).
SHA-256: `69b51cca94353c41bf9ff01e3995c86d0d5f377c998e72b937af7818e1c66dfa`.

`test_ref.alpha.webp` uses the same RGB sample, with alpha set to
`(17*x + 13*y) & 255`, color quality 75 and alpha quality 70 through the copied
custom C wrapper. It covers VP8X/ALPH and quantized transparency.
SHA-256: `6b6797d481f1921f8590c21209bee39d36001fc451d0e41490482fa279465956`.

Both were generated on 2026-10-24 with the unchanged pin, GCC 15.2.0-16ubuntu1,
CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off, x86_64 Linux.
The [lossy reference record](../../reference/LOSSY.md) records their reviewed
provenance and regeneration:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_public real_public_fixtures_match_pinned_c
```
