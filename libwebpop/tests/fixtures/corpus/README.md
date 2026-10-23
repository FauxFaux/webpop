`test_ref.ppm` is copied verbatim from `upstream/examples/test_ref.ppm` at
libwebp revision `1844042d1ca751e62e61df7617dba19c98588339`, under the
repository's upstream BSD license and patent terms. It is a 128×128 RGB P6 image.
SHA-256: `bf84f754ae60e8924b2a1416d9fd8f95e35d89742a691103ee1d7e013c21a723`.

`test_ref.webp` is the pinned scalar C `WebPEncodeLosslessRGBA` output for this
image, retained for portable byte comparisons. See the reference guide for
toolchain provenance and the `public_rgba_fixtures_match_pinned_reference`
regeneration command. Production Rust never invokes C or reads these fixtures.
