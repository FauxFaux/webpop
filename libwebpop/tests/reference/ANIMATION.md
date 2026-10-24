# Animation leaf reference record

Source pin: `1844042d1ca751e62e61df7617dba19c98588339`, unchanged and clean.
Recorded 2026-10-24 on Ubuntu x86_64 with GCC 15.2.0-16ubuntu1, CMake 4.2.3,
Release `-O3 -DNDEBUG`, SIMD and threading disabled. The existing native build
now enables `WEBP_BUILD_LIBWEBPMUX` and links CMake's `libwebpmux` target.
No demux library or production C dependency is added.

`animation.c` includes the original `src/mux/anim_encode.c`, renaming every
external animation definition to avoid collisions with linked `libwebpmux`.
It calls the original helpers; it does not copy their algorithms. The wrapper
uses prepared straight-alpha ARGB pictures without owning their input buffers.
The Rust evaluator and deterministic corpus live in `support/animation.rs`,
registered with the existing logical request dispatcher.

## Logical protocol

`AnimationLeaves` uses the existing hexadecimal-u32 word transport. Its first
word is the opcode. Signed fields use i32 two's-complement words; quality is
the f32 bit pattern. Rectangles are `(x_offset, y_offset, width, height)`.

| Opcode | Following input | Output |
| --- | --- | --- |
| 0 | quality | `QualityToMaxDiff` |
| 1 | source pixel, destination pixel, max allowed difference | `PixelsAreSimilar` |
| 2 | length, source step, destination step, max difference, source words, destination words | lossless comparison, lossy comparison |
| 3 | canvas width/height, source/destination strides, rectangle, lossless flag, quality, snap flag, source words, destination words | success, resulting rectangle, empty flag, lossless blend eligibility, lossy blend eligibility |
| 4 | same as 3 (snap flag unused) | same as 3, using `WebPAnimEncoderRefineRect`; failures return only success=0 and the unchanged rectangle |
| 5 | rectangle | original empty flag, snapped rectangle |

For opcode 2 each buffer contains `length * step + 1` words. This leaves room
for C's final pointer advance, including zero steps. For opcodes 3/4 each
canvas has `(height - 1) * stride + width` words; stride is in pixels and no
final-row padding is required. Source and destination strides can differ.
Opcode 3 requires a valid contained rectangle, including empty extents;
opcode 4 accepts signed guesses, clips them and checks quality/positive extents.

## Coverage and fixtures

The 80,287 live cases cover row/column and zero-step comparisons; unequal
strides with unrelated padding; skinny, odd and square canvases; unchanged,
hidden-RGB, alpha-only, corner and border changes; translucent versus opaque
blending; empty rectangles; signed clipping through `i32::MIN`/`i32::MAX`;
invalid extent/quality rollback; quality 0–100 in hundredths and adjacent f32
values around all rounding thresholds. Both lossless and lossy rectangle and
blend heuristics match C. The latter do not enable blended lossy encoding.

`fixtures/animation.txt` contains 81 C-generated cases from `fixture_cases`,
including both comparison outcomes, clipping, invalid input, fractional
quality, transparent hidden RGB and even snapping. Review the source pin and
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation animation_fixtures_match_pinned_reference -- --nocapture
```

Portable independent checks bound every 3x3 change mask by enumerating changed
pixels, check even snapping preserves right/bottom edges, and compare similarity
to independently computed channel bounds across every alpha/difference pair.
They check both blending rules, invisible RGB, checked view layout/overflow and
unchanged rectangles on refinement validation errors.

Validation passed:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test animation -- --nocapture
cargo test -p libwebpop --features native-oracle --test native_oracle public_rgba_fixtures_match_pinned_reference -- --nocapture
cargo test --workspace
cargo fmt --all -- --check
```

Each native animation run passes six tests: 81 fixtures, 80,287 live comparisons
and the four portable checks. No mux-assembly, candidate/cache, timing, or
single-frame optimization fixtures exist yet. Both single-frame cleanup paths,
especially decode-dependent reconstruction, remain explicit requirements in
the [animation plan](../../../docs/ANIMATION_PORT_PLAN.md). Leaf parity alone
does not establish valid animation output or `WebPAnimEncoder` parity.
