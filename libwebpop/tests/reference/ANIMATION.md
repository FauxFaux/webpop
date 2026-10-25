# Animation reference record

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

Each native leaf run passes six tests: 81 fixtures, 80,287 live comparisons
and the four portable checks. The next section records mux assembly and both
single-frame cleanup paths. Candidate/cache and optimized timing fixtures
remain open in the [animation plan](../../../docs/ANIMATION_PORT_PLAN.md).
Leaf and mux parity do not establish `WebPAnimEncoder` parity.

## Prepared-frame mux assembly (milestone 1)

The same pinned source, GCC 15.2.0-16ubuntu1 and CMake 4.2.3 configuration
recorded above generated `fixtures/animation_mux.txt` (33 cases) and
`fixtures/animation_single_frame.txt` (one reference-only completion gate).
The checkout remains clean. Review `target/native-oracle/provenance.txt` before
regeneration; no source, compiler, SIMD or threading setting changed.

`AnimationMux` wraps public `WebPMuxSetCanvasSize`, `WebPMuxSetAnimationParams`,
`WebPMuxSetChunk`, `WebPMuxPushFrame` and `WebPMuxAssemble`. The supplied still
bitstreams come from Rust's existing still API; C does **not** encode the raw
pixels carried in the request. Comparisons therefore use identical prepared
codec payloads and isolate assembly from codec parity. Rust's evaluator rebuilds
the opaque `EncodedFrame` using the same pixels/options and passes it to
`AnimationContainer`. Independent portable checks compare its nested chunks to
the original still chunks, using a checked RIFF walker rather than stripping a
fixed header length.

The word protocol is:

- Header: canvas width, height, background as `0xAARRGGBB` (serialized BGRA),
  loop count, metadata-presence mask (ICCP=1, EXIF=2, XMP=4).
- For each present metadata item in ICCP/EXIF/XMP order: byte count, then one
  word per byte. Present empty chunks retain a non-null borrowed pointer in C;
  `copy_data=0` preserves their presence, and storage lives through assembly.
- Frame count, followed by each frame: width, height, codec mode, byte stride,
  x/y origins, duration, ANMF control bits (dispose=1, replace=2), RGBA byte
  count, complete still-file byte count, RGBA bytes, still bytes.
- Codec mode 0 uses default lossless; 1 default lossy; 2 lossy quality 42.5 and
  alpha quality 50; 3 near-lossless strength 60 with default lossless effort;
  4 lossy with SharpYUV. RGB and alpha qualities keep their existing meanings.
- Output: one word per complete muxed file byte, without an extra length word.

The 193 live cases include all five codec modes, opaque/transparent VP8L,
VP8 and ALPH, all blend/dispose combinations, even nonzero origins, odd/skinny
rectangles, final-row padding omission, both single-frame mux representations,
metadata combinations and empty metadata, infinite/65535 loops, explicit BGRA,
zero/maximum durations, a 32-frame repeated sequence with zero intervals, axes
through `2^24`, offsets through `2^24-2`, and canvas area `2^32-1` without a raw
canvas allocation. Every complete output matches pinned WebPMux.

Portable tests independently inspect RIFF lengths, every VP8X flag/reserved
byte, canvas, metadata order/bytes, ANIM channels/loop, every ANMF rectangle,
duration and control bit, nested ALPH/VP8/VP8L payloads and zero padding. They
also check input/options rejection, unchanged builder/output on validation
failure, repeatable assembly, exact invisible RGB, and synthetic chunk/nested
ANMF/file size limits without multi-gigabyte allocation.

### Decode-dependent single-frame completion gate

`AnimationSingleFrame` takes the same request as `AnimationMux` and returns
`before byte count, before bytes, after byte count, after bytes`. It uses the
original static `OptimizeSingleFrame` through the include-source wrapper,
with lossless quality 70/method 4 and one output frame. The fixture contains an
opaque 1x1 rectangle at (2,2) on a transparent 4x4 internal canvas. Mux assembly
retains its 94-byte single-ANMF file; the optimizer decodes the rectangle,
re-encodes the canvas and chooses a 44-byte VP8L still.

This is **not** included in the Rust parity evaluator. The portable gate checks
Rust reproduces only the before file, independently decodes the recorded after
file with `image`, checks the changed pixel and transparent surrounding canvas,
and verifies the size improvement/removed animation wrapper. The live fixture
test compares both recorded C results. Exact production reconstruction remains
unsupported; there is no ignored mismatch or claim of optimized parity.

Regenerate only these two new fixture files after reviewing provenance:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_mux animation_mux_fixtures_match_pinned_reference -- --nocapture
```

Validation for this milestone:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation --test animation_mux -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test animation --test animation_mux -- --nocapture
cargo test -p libwebpop --features native-oracle --test native_oracle public_rgba -- --nocapture
cargo test -p libwebpop --features native-oracle --test lossy_public --test near_lossless_public --test sharpyuv_encoder -- --nocapture
cargo test --workspace
cargo fmt --all -- --check
```

## Raw RGBA full-frame sequences (milestone 2, 2026-10-25)

`AnimationEncoder` fixes the canvas in `new(width, height, AnimationOptions)`.
Global options contain an explicit `FullFrame` policy and existing owned
`AnimationContainerOptions`; the reserved `Optimized` policy is rejected.
`AnimationFrameOptions` selects `Lossless(LosslessOptions)`,
`NearLossless { strength, options }` or `Lossy(LossyOptions)` for each add.
Only the current method-4 lossy route is advertised. There is no mixed-candidate
heuristic; callers may explicitly change codecs between submitted frames.

`add_rgba` takes a complete immutable canvas, byte stride and `u64` start
timestamp. The first timestamp is an arbitrary origin. Consecutive timestamps
may be equal; every difference must fit 24 bits. `finish` requires the final
end timestamp. Empty input, descending timestamps, oversized intervals and
invalid layout/settings are recoverable validation errors. Runtime failures
after validation permanently fail the encoder; success permanently finishes it.
Both states reject further adds/finishes. All finish errors preserve output.
Only encoded frames are retained; finish releases payloads after assembly.
A single full-canvas frame follows ordinary mux cleanup into a still image.

`animation_sequence.rs` reuses the existing `AnimationMux` logical request,
wrapper and payload preparation. Its evaluator submits raw full-canvas pixels
to the sequence API and accumulates the request's durations from a `2^40` ms
origin. C receives exactly the same still-encoded payloads, (0,0) replacements,
no disposal and explicit durations. This deliberately compares against WebPMux,
not WebPAnimEncoder. Existing still-codec oracle suites establish codec parity.
No new native wrapper, decoder dependency or production C code is added.

`fixtures/animation_sequence.txt` contains 26 C-generated requests/outputs:
four odd/skinny geometries across all five existing codec configurations,
per-frame codec changes, repeated images, opaque/transparent pixels, omitted
final-row padding, zero/one/maximum durations, supplied odd/empty metadata,
explicit BGRA and loop count 65535; five single-frame cleanup cases; and a
40-frame repeated transparent sequence with zero intervals.

The existing `image` 0.25.10 / `image-webp` 0.2.4 decoder was inspected and
exercised through `AnimationDecoder::into_frames`. It returns every composited
canvas and exact stored millisecond delay, including zero; tests assert count,
dimensions, order and cumulative timing. Both Rust and C files pass through
that decoder. Every decoded frame compares to its corresponding still-codec
decode, and lossless visible RGB/alpha also compare to original input.
Full-canvas replacement makes decoder background conventions irrelevant.
Portable additional checks cover `u64::MAX`, arbitrary timestamp origins,
24-bit interval boundaries, empty/invalid state, validation recovery and output
preservation, stride arithmetic overflow, exact hidden RGB with all lossless
methods, near-lossless strengths 0/60/100 with exact on/off, color qualities
0/42.5/100, alpha qualities 0/100 and both SharpYUV settings.

The pin remains clean at `1844042d1ca751e62e61df7617dba19c98588339`.
Provenance reviewed before generating this new fixture file: Ubuntu x86_64,
GCC 15.2.0-16ubuntu1, CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off.
Existing fixture files are unchanged. Regenerate only the new sequence file:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_sequence animation_sequence_matches_pinned_mux_reference -- --nocapture
```

Validation commands for this milestone:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle animation -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

Full-frame raw sequence support is complete. Candidate selection, carryover,
merging/splitting, keyframes, optimized lossy filtering and decode-dependent
single-subrectangle re-encoding remain explicit optimizer completion gates.

## Borrowed subframes and rectangle routing (milestone 3, first step)

Recorded 2026-10-25 against the same clean pin, GCC 15.2.0-16ubuntu1 and
CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading disabled. The actual native
build provenance was reviewed before generating the new
`fixtures/animation_subframes.txt`; existing fixture files are unchanged.

`ArgbView::sub_view` implements the ARGB branch of `WebPPictureView` without
copying. Odd origins are retained, nested coordinates are relative to the
parent view, and the original canvas stride is preserved. `row` returns only
visible pixels and accepts no final-row padding requirement. Checked extents
use wide arithmetic, rejecting negative, empty or out-of-canvas rectangles.
These helpers do not impose codec dimension limits.

`SubFrameParams::new` is the allocation-free counterpart of
`SubFrameParamsInit`; Rust borrowed views need no explicit free.
`GetSubRect` preserves later keyframe guesses while minimizing first frames,
permits skipped empty rectangles or forces a 1x1 view at (0,0), then snaps
nonempty origins to even coordinates before constructing the borrowed view.
`GetSubRects` starts from the full canvas and seeds the lossy rectangle from
the already refined lossless rectangle. `should_try` and `empty_rect_allowed`
pass through unchanged. Empty views are represented by `None` and clear any
earlier view on successful reuse. Validation failures leave the supplied
rectangle/parameter object unchanged. Invalid qualities are rejected even
when a later keyframe would bypass minimization. Unlike the checked
`WebPAnimEncoderRefineRect` entry point, these routing helpers reject invalid
initial guesses instead of clipping them. A forced empty keyframe guess must
have (0,0) origin, matching the upstream assertion.

The existing include-source `animation.c` wrapper now exports
`WebpopAnimationSubframes`. It calls the original `WebPPictureView`,
`SubFrameParamsInit`, `GetSubRect`, `GetSubRects` and `SubFrameParamsFree`;
none of their algorithms are copied into the wrapper. `AnimationSubframes`
is registered in the existing Rust logical evaluator and C dispatcher.

The word protocol uses this fixed 15-word header, followed by source and
destination ARGB canvases:

```text
opcode, canvas width/height, source/destination pixel stride,
key flag, first flag, empty-allowed flag, lossless flag, quality f32 bits,
should-try flag, guess x/y/width/height
```

Each canvas has `(height - 1) * stride + width` words; there is no final-row
padding. Canvases use independent strides with unrelated padding. Signed
guess fields use two's-complement i32 words. Opcodes are:

| Opcode | Original operation | Output |
| --- | --- | --- |
| 0 | ARGB `WebPPictureView` | success, then view record if successful |
| 1 | `GetSubRect` | success, then view record if successful |
| 2 | `GetSubRects` | success, should-try, empty-allowed, lossless and lossy view records |

A view record contains rectangle x/y/width/height, view-present flag and,
when present, view width/height/stride followed by visible pixels in row order.
No addresses, padding or C object internals enter the protocol. Opcode 0 also
exercises invalid guesses through the original view validator; routing uses
valid guesses and qualities. Rust-only validation/rollback checks cover
unsupported routing input without invoking C assertions.

The 17,340 live cases cover direct views, every key/first/empty flag combination,
both should-try values, lossless and lossy single-rectangle routes, full and
partial initial guesses, empty guesses, first-frame forced holds, later
keyframes, qualities 0/24.5/75/98.5/100, hidden RGB, alpha-only changes,
nearly equal channels, odd centers, last pixels, border/opposite-corner changes,
unrelated opaque pixels and transparent initial canvases. Geometries include
1x1, skinny rows/columns and odd 17x19 canvases. Every rectangle, view dimension,
stride and visible pixel matches scalar C. The 47 compact committed fixtures
cover all routing flags, similarity and forced/skipped frames, odd-origin views
and direct-view rejection.

Portable independent checks enumerate every 3x3 change mask for small/large RGB
and alpha differences, both endpoint qualities, and all eight key/first/empty
combinations (24,576 cases). Bounding boxes come from independently enumerated
changed points and alpha-weighted channel bounds. Views are checked against
the original canvas pixel by pixel, including pointer identity to establish
borrowing. Additional tests cover nested views through the final pixel,
omitted last-row padding, a one-row `usize::MAX` stride, signed-extreme guesses,
invalid quality/geometry rollback and clearing previous views on skipped frames.

Regenerate only this new fixture file after reviewing pin/toolchain provenance:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_subframes animation_subframe_fixtures_match_pinned_reference -- --nocapture
```

Validation:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle animation -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

Only the subframe-view/routing item of milestone 3 is complete. No candidate
encoding, disposal selection, carryover masks, cache/duration merging or
keyframe selection is enabled; the public optimized policy remains rejected.
