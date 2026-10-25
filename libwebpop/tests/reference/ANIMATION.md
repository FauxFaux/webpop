# Animation reference record

Source pin: `1844042d1ca751e62e61df7617dba19c98588339`, unchanged and clean.
Recorded 2026-10-24 on Ubuntu x86_64 with GCC 15.2.0-16ubuntu1, CMake 4.2.3,
Release `-O3 -DNDEBUG`, SIMD and threading disabled. The existing native build
now enables `WEBP_BUILD_LIBWEBPMUX` and links CMake's `libwebpmux` target.
The initial leaf/mux milestones added no demux library. The lossless candidate
milestone below links test-only `webpdemux`; no production C dependency is added.

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

This step completes only subframe views/routing. The following section records
the subsequent lossless candidate work. Cache/duration merging and keyframe
selection remain disabled; the public optimized policy remains rejected.

## Lossless candidate selection (milestone 3, second step)

Recorded 2026-10-25 against the unchanged clean pin and existing GCC
15.2.0-16ubuntu1 / CMake 4.2.3 toolchain, Release `-O3 -DNDEBUG`, scalar and
synchronous. Provenance was reviewed before generating the new
`fixtures/animation_candidates.txt`; previous fixtures are unchanged.
The only native build addition is linking pinned `webpdemux` for all-frame
decoder verification. Production remains safe Rust without a decoder or C.

Checked `ArgbViewMut` supports inherited-stride nested views and row mutation.
`IncreaseTransparency` replaces identical **nonzero** ARGB words with zero,
marks packed rectangle-relative mask bytes as 1, and preserves other mask
bytes, trailing storage, row padding and outside pixels. An already-zero word
is not marked. `DisposeFrameRectangle` clears to transparent black, independently
of ANIM background metadata. Geometry, layout and mask validation precede writes.

`anim_encode/lossless.rs` ports the lossless subset of `EncodeCandidate`,
`GenerateCandidates`, `PickBestCandidate` and `SetFrame` (the exposed helper is
named `SetFrameLossless`). It accepts the actual carryover canvas, current
canvas, preceding frame rectangle and explicit key/first/undecided/minimize
controls. Only lossless strength 100 is enabled, with existing `LosslessOptions`
methods 0–6. The first frame must be evaluated as a keyframe. An unchanged
frame returns `None`; duration merging is left to the future cache.

Routing preserves first-frame minimization, forced 1x1 empty frames, later
keyframe bounds, suppression of background trials for key/undecided cases,
strict area choice outside minimize-size mode, NONE-before-BACKGROUND trials
in minimize-size mode, and strict encoded-byte selection (earlier wins ties).
Candidates encode independently owned cropped ARGB copies through the existing
typed VP8L payload boundary. This replaces `CopyCurrentCanvas` scratch restore
without exposing codec cleanup or transparency mutation to caller inputs or
later trials. Candidate comparison uses complete still-file size, as C does.
The result owns the selected payload, zero-duration/no-disposal current-frame
controls, optional preceding disposal and winning mask. Keyframe evaluation
does not change preceding disposal. An absent carryover mask means no pixels
carry over; a present mask covers the entire selected rectangle.

Codec-level `exact` does not promise invisible RGB after transparency
substitution. Near-lossless, carryover and cache updates are added below;
lossy/mixed selection, filter overrides, the optimized RGBA policy and deferred
keyframe decisions remain open.

### Candidate and mutation protocol

`AnimationLossless` is registered in the existing dispatcher/evaluator.
`animation.c` calls original `IncreaseTransparency`, `DisposeFrameRectangle`,
`EncodeCandidate` and `SetFrame`; selection algorithms are not copied into the
wrapper. Its fixed 18-word header is:

```text
opcode, canvas width/height, source/destination pixel strides,
rectangle x/y/width/height, flags, integer quality, method, exact,
preceding rectangle x/y/width/height, mask byte count
```

Flags are key=1, first=2, preceding-candidate-undecided=4, minimize-size=8,
direct-candidate-blend=16 and disposal-background=32. The header is followed by
source and destination ARGB canvases (each `(height - 1) * stride + width`
words), then one word per mask byte. Quality is lossless effort, not fidelity.

| Opcode | Original operation | Output |
| --- | --- | --- |
| 0 | `IncreaseTransparency` | modified flag, complete destination storage including padding, complete mask |
| 1 | `DisposeFrameRectangle` | complete destination storage including padding |
| 2 | lossless `EncodeCandidate` on the supplied rectangle | candidate-present=1, candidate record |
| 3 | lossless `SetFrame` | candidate-present flag, record if present |

A candidate record contains rectangle x/y/width/height, C blend enum (0 blend,
1 replace), preceding-disposal tag (0 untouched, 1 NONE, 2 BACKGROUND), carries
flag, mask length/mask bytes, complete still-file byte count and bytes.
Duration zero/current disposal NONE are asserted separately. The SetFrame
wrapper supplies a preceding cache slot solely for observing selected disposal;
it does not call CacheFrame, merge durations or compare optimized sequences.

There are 3,120 live cases across 1x1, skinny, odd and 16x16/32x17 canvases,
independent padded strides without final padding, methods 0–6, qualities
0/70/100, both exact settings, all supported key/first/undecided/minimize
controls, full/partial/empty preceding rectangles, unchanged input, hidden RGB,
alpha-only changes, fading/erased/moving sprites, opposite corners and random
pixels. Assertions require skipped (144), blended (306), replacement (1,422),
BACKGROUND-selected (18) and carrying (136) selections, including four winning
BACKGROUND masks. Both disposal alternatives can modify scratch pixels/masks;
the selected mask and payload match C after replacement of the earlier winner.
The 79 committed fixtures also include identical alternative sizes proving
NONE wins ties, partial/empty mutation rectangles, sentinel mask values/tails
and the explicit BACKGROUND winner with a nonempty mask.

### Exact decoding and compositing

`image-webp` 0.2.4's animated compositor was found to round some opaque blended
colors down by one: it applies the approximate alpha blend even for source
alpha 255. The pinned decoder's row routine copies those pixels directly.
For example, an opaque `[152,118,84,255]` blended pixel can become
`[151,117,83,255]` in that test decoder. This is not an encoder mismatch and
is not accepted through tolerances or ignored pixel checks. The existing
full-canvas replacement sequence tests still use its all-frame decoder.

Candidate portable checks independently walk RIFF/ANMF, decode each VP8L
payload with image's still decoder, dispose precisely the preceding rectangle
to zero, and composite every frame. Eligible lossless blended payloads contain
alpha 0 (hold) or 255 (copy); the checker asserts this property and uses exact
endpoint-alpha composition. Replacement payloads copy all alpha values.
Every canvas and cumulative timestamp is recorded; final visible RGB and alpha
must equal the requested canvas. Invisible RGB is canonicalized to zero only
in decoder observations, while exact codec/hidden-RGB behavior has separate
payload tests. Mutation tests independently enumerate alpha 0/1/127/254/255,
hidden nonzero RGB and prefilled mask preservation, with validation rollback.

`AnimationDecode` is a reference-only decoder request: file byte count followed
by bytes. `WebPAnimDecoder` runs in MODE_RGBA with threading disabled and returns
canvas width/height, frame count, loop count, then each cumulative timestamp
and full RGBA canvas, canonicalizing invisible RGB. It has no production or
general Rust demux API. Native tests compare all frames of 48 prepared
sequences to the portable compositor, including cropped preceding disposal,
blend holds and all-transparent/forced first frames. Candidate payloads already
compare byte-for-byte between C and Rust, so the checks apply to both outputs.

Regenerate only the new candidate fixtures after reviewing provenance:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_candidates animation_candidate_fixtures_match_pinned_reference -- --nocapture
```

Validation commands:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_candidates -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

The lossless blend/disposal/candidate-selection item is complete. Its canvas
updates are covered below; Rust CacheFrame/FlushFrames, timing merging/splitting
and all lossy/mixed/keyframe selection remain open.

## Lossless carryover canvas updates (2026-10-25)

`CopyIdenticalPixels` and `CopyMaskedPixels` are direct ports of the original
static helpers. `LosslessCandidate::update_canvases` extracts the accepted
candidate's canvas-update branches from `CacheFrame`: full-copy for keyframes;
otherwise preceding disposal, rectangle-local masked or full copy, then save
the original input in `prev_canvas`. Validation precedes either destination's
mutation. Pixel strides are independent, final padding is optional, and all
copies preserve invisible RGB. Nonzero mask bytes hold pixels; only zero copies.

Before selection, `CopyIdenticalPixels` restores words identical in the previous
and current originals into `canvas_carryover`. On a skipped frame the original
and preceding rectangle do not advance. The carried canvas is an optimization
history, not a decoded canvas; it retains original input values, even if codec
cleanup or encoding altered the corresponding output. These low-level helpers
do not own timestamps, cache entries or preceding-frame output controls.

### Protocol and corpus

`AnimationCarryover` has the seven-word header `(opcode, width, height,
a_stride, b_stride, destination_stride, mask_byte_count)`, followed by three
ARGB canvases and one word per mask byte. Each canvas contains exactly
`(height - 1) * stride + width` words. Opcode 0 calls original
`CopyIdenticalPixels(a, b, destination)`; opcode 1 calls original
`CopyMaskedPixels(a, mask, destination)`. Both return complete destination
storage including untouched padding. Mask tails include sentinel bytes.

`AnimationCarryoverSequence` takes `(width, height, minimize_size, frame_count)`,
then for each input `(ARGB stride, lossless effort quality, method, exact)` and
its strided ARGB words without final padding. Default keyframe insertion stays
disabled. The C adapter performs the original pre-cache `CopyCurrentCanvas`,
calls original `CacheFrame`, serializes the selected candidate, preceding
rectangle and both canvases, then calls original `FlushFrames` to retain its
normal lookbehind. It does not duplicate selection or canvas-update algorithms.

For each submitted input the output is a candidate-present flag, the candidate
record described above if present, preceding rectangle x/y/width/height, packed
`prev_canvas` and packed `canvas_carryover`. Duration is zero and current-frame
disposal NONE; timing is deliberately absent from this protocol. Complete
payload and mask bytes are compared, including after earlier masks, disposal
and codec changes. Rust invokes the ported leaves, `SetFrameLossless` and the
winning candidate's canvas updates in the same order.

The 768 live cases contain 96 separately strided copy-helper cases and 672
eight-frame sequences (5,376 transitions). They cover 1x1, skinny, odd,
16x16/32x17 canvases, methods 0–6, effort 0/70/100, exact on/off, minimize-size,
empty/constant/repeated inputs, moving/erased sprites, semitransparent fades,
hidden RGB, opposite corners and random colors. Codec options and stride change
within sequences. Coverage assertions require skips (1,456), BACKGROUND
selection (246), carrying candidates (784) and BACKGROUND with carryover (28).

`fixtures/animation_carryover.txt` records 12 helper cases and 20 sequences
(32 fixtures), generated exclusively from C. Portable checks enumerate exact
word equality and mask values 0/1/7/255. Deliberately distinct synthetic
previous/carryover canvases independently check disposal-before-mask order,
held pixels, rectangle exteriors, full-canvas keyframe copies, original-input
preservation and validation rollback of both destinations. Pure lossless
sequences often have matching original/carryover values, so those synthetic
histories exercise retention explicitly.

Fifteen multi-frame fixture sequences are assembled with unit durations for
accepted candidates and the selected disposal on the preceding accepted frame.
The shared independent RIFF/VP8L compositor checks every canvas against its
original input and exact stored timing. All 15 also match pinned
`WebPAnimDecoder`. Single-accepted-frame sequences remain in the state/payload
corpus; the all-frame checks here require at least two accepted frames.

Source pin, scalar CMake settings and compiler remain unchanged: GCC
15.2.0-16ubuntu1, Release `-O3 -DNDEBUG`, SIMD/threading off. The upstream
checkout was verified clean before generation; provenance remains in
`target/native-oracle/provenance.txt`. After reviewing those records, regenerate
only this fixture file with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_carryover animation_carryover_fixtures_match_pinned_reference -- --nocapture
```

Validation commands (all passed):

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_carryover -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

Carryover state/payload parity alone does not establish optimized full-file
parity. The following milestone adds keyframe-disabled lossless caching and
timing; keyframe selection, near-lossless/lossy/mixed candidates and
decode-dependent single-frame reconstruction remain open. The public RGBA
sequence API still rejects `Optimized`.

## Lossless cache and duration merging (2026-10-25)

`mux::anim_encode::LosslessAnimationCache` integrates `SetFrameLossless`, the
ported canvas updates and the keyframe-disabled branches of `CacheFrame`,
`FlushFrames` and `IncreasePreviousDuration`. The first input is forced even
when transparent; later identical inputs skip candidate insertion while their
intervals accumulate in the pending frame. Changes retain exact zero durations.
The most recent frame remains cached until its duration and preceding-disposal
choice are known. Flushed payloads remain buffered in `AnimationContainer`.

The pin allocates two cache slots when keyframes are disabled. If accumulated
duration plus the next interval reaches `2^24`, the previous duration is left
unchanged and a transparent blended 1x1 hold is inserted with the supplied
interval as its duration. The hold uses the original fixed VP8L bytes, prepared
directly as a typed payload; it is not re-encoded or parsed from RIFF. The
preceding rectangle becomes `(0,0,1,1)` and the frames-since-keyframe counter
increments. Before encoding the next input, old frames flush to free a slot.
Final timing can trigger the same insertion without a new source frame.

Only lossless strength 100 is accepted, with methods 0–6, effort 0–100, exact
on/off, minimize-size and supplied container metadata. Keyframe insertion stays
disabled. Freezing/rebasing an already selected deferred keyframe on overflow
belongs to the future keyframe cache implementation and is not claimed here.

`add_argb` accepts immutable checked ARGB views with independent strides and
optional final padding. Two owned canvases retain original/carryover pixels;
temporary candidate copies/masks remain isolated as in the preceding milestone.
Input timestamps are non-decreasing `u64` with arbitrary origin; every interval
must fit 24 bits. `finish` always needs explicit final timing. Canvas, codec and
timing validation happens before mutation and permits retry. Subsequent encoding,
allocation or assembly failures are terminal; all finish errors preserve output.
Successful finish releases canvases and buffered payloads and rejects reuse.
Error indexes count submitted frames, including merged inputs.

### Protocol and compatibility boundaries

`AnimationCache` is registered in the existing evaluator and C dispatcher.
Its eight-word header is:

```text
canvas width, canvas height, minimize_size, assemble_mode,
input frame count, metadata flag, timestamp origin low/high words
```

Each frame follows as `(duration_ms, ARGB stride, lossless effort quality,
method, exact)` then `(height - 1) * stride + width` ARGB words. The metadata
flag supplies loop 65535, background RGBA `(12,34,56,78)`, ICCP `[1,2,3]`, present
empty EXIF and XMP `[4]`; otherwise default white background/infinite loop and
absent metadata are used. Empty EXIF borrows static non-null C storage, following
the prepared-mux wrapper's presence convention. Source pixels remain unchanged.

The adapter calls original `WebPAnimEncoderAdd` for every source and a final
NULL-frame timestamp. It transports only the low origin word to C by copying its
bits into the signed timestamp, preserving equivalent unsigned wrapping
intervals, including crossings of `INT_MAX` and `UINT32_MAX`. Rust keeps the full
wide origin and performs no signed wrap emulation. The C source/configuration
and algorithms remain unchanged.

After each submitted input and after final assembly, ten output words record:

```text
input count, flushed output count, cached count, frames since keyframe,
preceding rectangle x/y/width/height, pending duration present/value
```

Then a byte count and complete output bytes follow. Mode 0 invokes original
`FlushFrames` and the original mux setup/assembly calls, stopping immediately
before `OptimizeSingleFrame`. Mode 1 uses original public
`WebPAnimEncoderAssemble`, including its single-frame handling. Mode 2 is a
separate reference-only completion gate: it records mode 0 output and then an
additional byte count/file from full public C assembly. The ordinary Rust
evaluator explicitly rejects mode 2. No expected optimized single-frame bytes
are replaced or ignored in a passing full-file parity corpus.

### Evidence and remaining gates

There are 1,680 live cases with eight inputs each (13,440 submitted-frame
snapshots plus 1,680 final states). Of these, 1,470 compare complete bytes against
public animation assembly; 210 deliberately use the pre-reconstruction
boundary. All controls, counters, durations, payload and file bytes match.
Coverage assertions include 133 split-before-changed-frame transitions and
2,639 adds that merge without inserting an encoded frame. Inputs include 1x1,
skinny, odd, 16x16/32x17 canvases, all lossless methods, effort 0/70/100, exact
on/off, minimize-size, metadata presence, repeats, zero/max intervals,
transparency/hidden RGB, sprites, fades, disposal/masks and opposite corners.
Codec settings change between inputs.

`fixtures/animation_cache.txt` records 32 C-generated cases (316 input frames),
including final-duration overflow, exact `2^24-1` versus `2^24` merged thresholds,
capacity flushing immediately before changes, 16-frame disposal histories and
96 maximum-duration repeated inputs. Portable checks independently compare
piecewise source/output playback intervals and every visible RGBA canvas; they
sum stored timing exactly without expanding a millisecond timeline. The shared
RIFF/VP8L compositor checks all animated frames, including holds and zero-delay
changes. Twenty-nine animated fixture files also match original
`WebPAnimDecoder`; the three full-canvas stills are independently decoded too.
Validation tests exercise descending/excessive intervals, recovery with unchanged
status/output, submitted-index errors after merging, wide origins near
`u64::MAX`, and terminal finish behavior.

The separate mode-2 native gate submits eight identical transparent 8x7 canvases.
They merge into one 90-byte cropped ANMF file before optimization; original C
decodes/re-encodes it to a 34-byte full-canvas still. Rust exactly matches the
90-byte representation, and the test explicitly asserts that the optimized
file differs and decodes correctly. This confirms that merging can reach the
existing single-frame completion gate; it does not complete reconstruction.

The pin and toolchain are unchanged: clean upstream revision
`1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1, CMake Release
`-O3 -DNDEBUG`, SIMD/threading off. Review `target/native-oracle/provenance.txt`
before regenerating only the new fixture file:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_cache animation_cache_fixtures_match_pinned_reference -- --nocapture
```

Validation commands:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_cache -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

The following milestone adds near-lossless routing. Lossy/mixed candidates,
bounded/all-keyframe policies and deferred cache selection, decode-dependent
single-frame reconstruction and the public RGBA Optimized policy remain open.
The VP8L cache is a supported subset, not a claim of complete animation parity.

## Near-lossless VP8L animation (2026-10-25)

`encode_near_lossless_candidate`, `set_frame_near_lossless` and
`LosslessAnimationCache::add_near_lossless_argb` accept signed strength 0–100
with the existing `LosslessOptions`. The original `EncodeCandidate`,
`SetFrameLossless` and `add_argb` call the same implementations at strength 100;
their APIs and committed output fixtures are unchanged. The shared VP8L
candidate types retain their names. Strength validation happens before cache
duration/canvas/state changes, with the existing invalid-strength encoding
error and submitted frame indexes.

The original animation path passes `WebPConfig.near_lossless` into each codec
trial. Rectangle refinement, blend eligibility, transparency substitution and
mask selection still compare original ARGB words. Only then does the existing
VP8L encoder route the selected rectangle through palette bypass, predictor
quantization or standalone preprocessing. The Rust port follows that order and
does not preprocess the full canvas in advance. Disposal alternatives each get
an independent candidate copy, preserving masks and the first winner on ties.

Near-lossless reconstruction is intentionally absent from carryover tracking:
accepted frames save original input words, masked pixels retain earlier
original words, and `CopyIdenticalPixels` restores repeated input words before
selection. Thus an unchanged source can merge even though its last decode
differs. Changing its codec strength to 100 does not force those pixels to be
re-encoded. Strength 100 remains the lossless codec route, not a promise to
restore loss from previously carried near-lossless frames. `exact` is still
codec-level: it disables predictor quantization/transparent cleanup while
standalone preprocessing and animation transparency substitution can alter
pixels. Intermediate alpha and visible RGB can change below 100.

### Protocol and fixture provenance

`AnimationNearLossless` prefixes the existing 18-word `AnimationLossless`
request with one strength word. Operations 2/3 call original `EncodeCandidate`
or `SetFrame`, respectively, with `config.near_lossless` set. Candidate records,
disposal tags, mask and full-payload serialization are unchanged. The original
lossless request still selects strength 100 through the same C adapter.

`AnimationNearLosslessCache` uses the eight-word `AnimationCache` global header
and output layout, extending each frame header from five words to six:

```text
duration_ms, ARGB stride, lossless effort quality, method, exact, strength
```

Strided original ARGB words follow. Strength and other codec options can change
on every input. The wrapper sets the original config field before public
`WebPAnimEncoderAdd` and retains explicit final NULL-frame timing, the same
metadata conventions, low-word wrapping C timestamps and mode 0/1/2 assembly
boundaries. Shared C adapters preserve the old request layouts; no upstream
code, selection algorithm or near-lossless implementation is copied or edited.
The test geometry cap was widened to cover 63/64/65-pixel preprocessing cases.

`fixtures/animation_near_lossless.txt` contains 64 C-generated cases: 40 isolated
candidate cases (including eight direct EncodeCandidate cases) and 24 cached
eight-frame sequences. They record strengths 0/60/100, exact on/off, palette
and non-palette inputs, masks and disposal alternatives, direct cropped
payloads, transparent sequences, zero/max intervals and overflow holds.
All previous lossless and standalone near-lossless fixtures remain unchanged.

Source pin and scalar toolchain were reviewed before generation: clean revision
`1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1, CMake Release
`-O3 -DNDEBUG`, SIMD/threading off, near-lossless enabled. Actual compiler and
build flags remain recorded in `target/native-oracle/provenance.txt`. Regenerate
only this fixture file after reviewing those records:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_near_lossless animation_near_lossless_fixtures_match_pinned_reference -- --nocapture
```

### Routing and decoded-pixel evidence

The 4,893 live C comparisons contain 3,773 candidate selections across 1x1,
skinny, 19x17 and 63/64/65x5 canvases, methods 0–6 and strengths
0/19/20/39/40/59/60/79/80/99/100. Another 1,120 eight-frame sequences compare
8,960 submitted-frame states and final output, including 980 complete public
animation files and 140 explicitly before single-frame reconstruction.
Original comparisons, disposal controls, strict candidate selection, winning
masks, complete payloads, cache counters and output bytes match exactly.

Portable candidate checks rebuild the selected rectangle from original pixels
and the winning transparency mask, then compare its complete payload and decode
to the already validated prepared near-lossless RGBA API. They assert alpha
endpoint preservation and establish active RGB/intermediate-alpha changes.
Strength-100 payloads retain the independent lossless pixel checks.

Routing regressions separately establish:

- Palette candidates bypass quantization, with identical 0/100 bytes.
- A predictor-selected image with `exact` has identical 0/100 bytes; analysis
  confirms every trial uses prediction. Predictor quantization is bypassed.
- A non-palette, non-predictor 65x5 noisy image changes visible RGB and
  intermediate alpha even with `exact`, proving active standalone processing.
- Its 63x5 cropped view has identical 0/100 bytes because the standalone small
  image shortcut sees the selected rectangle's dimensions.
- An opaque noisy 65x5 rectangle becomes a three-color palette after unchanged
  interior pixels become transparent holds. The 323 mask bytes and 0/100
  identity establish substitution before codec routing.
- A noisy original repeat merges while its earlier decoded alpha differs;
  a later strength-100 repeat still merges. Original words drive the history.

All 24 cached fixture animations are independently traversed as RIFF/ANMF,
decoded as VP8L still rectangles and composited with preceding disposal and
exact endpoint-alpha blending. Eligible VP8L blend inputs have alpha 0/255;
near-lossless preserves those endpoints. Replacement frames can contain
quantized intermediate alpha. Every composited canvas and cumulative timestamp
matches pinned `WebPAnimDecoder` exactly, without pixel tolerances. Stored total
timing is checked independently, including transparent overflow holds. The
tests deliberately do not compare near-lossless composited pixels to original
inputs as if they were lossless. Wholly strength-100 cache cases instead compare
complete states and files to the existing lossless cache.

Validation commands:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_near_lossless -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

At this VP8L milestone, lossy/mixed candidates, filtering overrides,
deferred/bounded/all-keyframe selection and decode-dependent single-frame
reconstruction remained open. The lossy filter integration follows below. The
public RGBA Optimized policy remains rejected. This extends the supported VP8L
subset without claiming complete pinned animation parity.

## Lossy candidate filter integration (2026-10-25)

`mux::anim_encode::encode_lossy_candidate` ports the lossy branch of
`EncodeCandidate` independently of lossy rectangle selection, flattening,
carryover and caching. It accepts a checked borrowed ARGB subframe, rectangle,
existing `LossyOptions` and blending flag. `LossyCandidate` owns typed VP8/ALPH
output, rectangle and initial controls; `encoded_size` includes the complete
still representation used by upstream size comparisons. Codec configuration
remains private. Replacement candidates use default filter strength 60;
blended candidates use zero. Autofilter stays disabled. `InitVP8Encoder` also
selects VP8 profile 2 when filtering is disabled, versus default profile 0.
The override reaches setup, filter adjustment and the frame header. Method
mapping, direct/token loops, alpha effort and SharpYUV reuse the still pipeline.
No general filter settings were added to public `LossyOptions`.

### Protocol and provenance

`AnimationLossyCandidate` takes ten header words:

```text
width height ARGB_stride x_offset y_offset quality_f32_bits method alpha_quality SharpYUV use_blending
```

Strided ARGB words follow, excluding final-row padding. Width/height are positive,
origins even and below `2^24`. The wrapper copies the picture and calls the
original `EncodeCandidate` from the included pinned animation source. It does
not duplicate the filter override. The response is:

```text
x_offset y_offset width height blend_method duration dispose_method file_byte_count complete_still_bytes...
```

Each byte occupies a u32 word; blend/dispose use pinned mux enum values.
Initial duration is zero and disposal is NONE. Invalid codec configuration
returns no words. The shared dispatcher and oracle build are unchanged apart
from this new request; no upstream source is modified.

`fixtures/animation_lossy_candidates.txt` contains 56 C-generated records:
seven methods, four input/configuration combinations, paired replacement and
blended encoding. Geometries cover 1x1, 3x5 and 17x19; inputs include opaque,
smooth alpha and random RGBA, fractional quality, quantized alpha, SharpYUV
and its small-dimension fallback. Source pin/toolchain were reviewed before
generation: clean `1844042d1ca751e62e61df7617dba19c98588339`, GCC
15.2.0-16ubuntu1, Release `-O3 -DNDEBUG`, SIMD/threading off and near-lossless on.
Actual flags remain in `target/native-oracle/provenance.txt`. Existing default
still fixtures are unchanged. Regenerate only this new fixture file with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_lossy_candidates animation_lossy_candidate_fixtures_match_pinned_reference -- --nocapture
```

### Validation and boundaries

The live corpus compares 4,704 complete candidate outputs: seven geometries
(1x17, 17x1, 3x5, 4x4, 15x16, 17x19, 33x17), six alpha/color patterns,
all methods 0–6, four color/alpha quality pairs, both SharpYUV settings and both
blend controls. Every complete file matches original C and independently
decodes with `image`. RIFF/chunk lengths, order/padding, VP8X/ALPH flags,
dimensions and exact/quantized alpha use the existing independent still checks.
An existing independent arithmetic reader parses VP8 partition 0, including
absolute segment features and global filter strength. All blended strengths
must be zero, with profile 2; replacements retain profile 0 and match the public
still API exactly. Paired fixtures demonstrate changed decoded RGB with each
method while ALPH bytes and decoded alpha remain identical across the override.
Portable checks also cover invalid configuration/geometry, codec dimension
limits, independently strided nested views, missing final-row padding and source
immutability.

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_lossy_candidates -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test animation_lossy_candidates -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

This milestone isolated candidate encoding. Lossy SetFrame, flattening and
carryover follow below; mixed trials, keyframe selection and single-frame
reconstruction remain open. Public RGBA Optimized remains unavailable.

## Lossy selection, flattening and carryover (2026-10-25)

`FlattenSimilarBlocks`, `SetFrameLossy`, `LossyCandidateOptions` and
`LossyCandidate::update_canvases` implement VP8-only candidate selection and the
corresponding original-word CacheFrame updates. Each disposal trial uses a fresh
copy of the current canvas. Blend eligibility and rectangle refinement precede
flattening; the codec sees the selected crop only after global-grid mutation.
The existing filter override, methods 0–6, alpha quality and SharpYUV are reused.

Flattening starts each 8×8 block strictly after the rectangle origin, even when
that origin is aligned, and visits only whole blocks aligned to the original
canvas. All 64 source pixels must be opaque and similar to the destination.
Replacement RGB is the truncated average of source channels, with alpha zero;
all 64 mask bytes become one. Other destination words, padding and mask bytes
remain untouched. A crop-local implementation would produce different masks
and bytes for a rectangle at (2,2); a dedicated fixture and independent check
cover that case. Empty/skinny rectangles make no changes. Safe Rust validates
views, quality, rectangle and mask before mutation.

SetFrame preserves NONE-before-BACKGROUND evaluation, lossy-area selection,
minimize-size trials and strict first-minimum byte selection. Empty changes use
lossy similarity: even quality 100 allows channel difference one at alpha 255.
First frames force nonempty output, later keyframe evaluation uses the whole
canvas, and an undecided preceding candidate excludes background disposal.
Selected masks/disposal remain paired with the winning payload. Keyframe
insertion policy, mixed-codec eligibility and timestamp caching are separate.

Canvas updates use original input, never decoded VP8 pixels or average colors.
Before selection, exact original repeats restore carryover words. On an accepted
subframe, dispose the preceding rectangle, keep masked pixels, copy unmasked
originals and save the entire original input as `prev_canvas`. Keyframes copy
the whole original into both canvases. A skipped frame retains the last accepted
original and rectangle even if the new original was merely similar. VP8L and
VP8 share the same validated update implementation; existing VP8L fixtures and
defaults are unchanged.

### Protocol and provenance

`AnimationLossy` prefixes the existing 18-word `AnimationLossless` request with
`(alpha_quality, SharpYUV)`. The original quality word contains f32 bits; method,
flags, rectangles, source/destination strides and mask length retain their
positions in the original portion. Opcode 0 calls original FlattenSimilarBlocks
and returns changed flag, the complete strided destination and every supplied
mask byte. Opcode 3 calls original SetFrame with lossless/mixed disabled and
uses the existing candidate response: present flag, rectangle, blend, preceding
disposal, carries flag, packed winning mask and complete still bytes. The common
C adapter preserves all older lossless/near-lossless request layouts. Additional
mask backing storage keeps the original helper's grid-pointer arithmetic inside
the allocation for empty/skinny leaf cases; only supplied mask bytes are observed.

`AnimationLossyCarryoverSequence` has global header
`(width, height, minimize_size, input_count)`. Each input contains
`(ARGB_stride, quality_f32_bits, method, alpha_quality, SharpYUV)` then strided
ARGB without final-row padding. The shared adapter calls original
CopyCurrentCanvas/CacheFrame/FlushFrames with keyframes disabled. Its response
is the existing per-input candidate-or-skip record followed by preceding
rectangle, all `prev_canvas` words and all `canvas_carryover` words. It observes
the original state without copying its algorithm. This protocol has no supplied
timestamps and does not claim complete lossy WebPAnimEncoderAssemble parity.

`fixtures/animation_lossy_carryover.txt` contains 73 C-generated records: eight
flatten operations, 43 selections and 22 eight-input sequences. Source pin and
toolchain were reviewed before generation: clean
`1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1, Release
`-O3 -DNDEBUG`, SIMD/threading off, near-lossless enabled. Actual flags remain
in `target/native-oracle/provenance.txt`; no upstream source changed. Regenerate
only this new file after reviewing those records:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_lossy_carryover animation_lossy_carryover_fixtures_match_pinned_reference -- --nocapture
```

### Independent checks and native comparisons

All 5,088 live cases match C: 1,392 flatten operations, 2,772 selections and
924 eight-input sequences (7,392 transitions). Inputs cover skinny and partial
block geometries, distinct padded strides, missing final-row padding, all
methods, fractional quality, alpha quantization, SharpYUV, first/later keyframe
evaluation, undecided previous candidates and both size policies. Every mutated
word and mask byte, selected control, complete codec file and resulting canvas
is compared. The state corpus records 2,604 skipped inputs, including 994 with
different originals, 840 background disposals, 560 masked candidates and 21
background-plus-mask winners. It observes 34,368 held pixels differing from the
current original and 27,648 exact-repeat restorations of carried pixels.

Portable block checks independently enumerate canvas squares, test each of 64
bad-pixel positions across 16 origins, and establish source-average truncation,
opaque-only eligibility, bounds, untouched padding/mask tails and validation
rollback. Candidate checks rebuild the selected globally flattened rectangle
and compare the isolated VP8 encoding. Empty preceding rectangles exercise
equal-size trials and first-winner mask/disposal preservation. Independent
original-word transitions verify skip behavior, restoration and disposal-before-
mask order, including raw invisible RGB and drift in carried blocks.

For playback, accepted candidates receive unit durations and the next selected
disposal is assigned to the preceding frame. All 18 fixture sequences with
multiple accepted frames are assembled, their complete VP8/ALPH still rectangles
decoded with `image`, and composited independently with replacement/blending and
preceding disposal. The compositor implements the pinned decoder's straight-
alpha integer rounding and transparent-background/keyframe shortcuts. Every
canvas and cumulative timestamp matches pinned WebPAnimDecoder exactly, with
no pixel tolerance. Optimizer carryover words are never used as decoded pixels;
lossy output has no source-pixel equality guarantee. The four one-frame cases
become stills and do not have animation timing.

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_lossy_carryover -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

At this milestone the low-level cache remained VP8L-only. Mixed selection/config
reversal follows below; lossy/mixed timestamp merging and overflow holds,
deferred keyframes, single-frame reconstruction and RGBA Optimized remain open.

## Mixed candidates and shared configuration (2026-10-25)

`SetFrameMixed` ports mixed SetFrame/GenerateCandidates using the existing VP8L
and VP8 trial generators. `MixedEncodingConfig` reuses `LossyOptions` for method,
quality and VP8 alpha/SharpYUV fields, with a primary codec selector, VP8L
near-lossless strength and shared exact flag. `reversed` flips only the selector,
retaining all other fields. Cache integration must save both submitted configs
even on a skipped input; this stateless helper does not own timestamp history.
`MixedCandidateOptions` adds the existing per-evaluation frame/disposal flags.
The winner reports its actual codec independently of the submitted primary and
keeps its complete payload, rectangle, preceding disposal and carryover mask.
Original-word canvas updates reuse the existing validated implementation.

The primary determines which codec's change-rectangle area selects disposal in
ordinary mode. It does not change codec order or choose the winner. Each chosen
disposal alternative counts colors in its original lossless crop, before either
transparency operation or codec cleanup. VP8L is eligible below 194 colors and
VP8 from 31; minimize-size bypasses extraction and tries both. Evaluation order
is NONE VP8L, NONE VP8, BACKGROUND VP8L, BACKGROUND VP8, omitting excluded trials.
An empty rectangle from either codec skips before any trials, including similar
but different originals when primary is VP8L. Equal sizes retain the first trial
and its mask/disposal. Fresh inputs isolate each trial's mutations and cleanup.

Quality remains f32 for refinement, both saved configs and VP8. The VP8L stream
converts it to integer effort; its preceding >=50 and ==100 analysis conditions
also agree with truncation for validated 0–100 inputs. Shared exact bypasses
transparent-area cleanup in VP8 as well as transparent cleanup/predictor
quantization in VP8L. Animation substitution and standalone near-lossless still
apply. A private VP8 exact flag carries this behavior without changing public
LossyOptions or the default still output. Both codec configurations validate
before palette or empty-frame shortcuts. Mixed encoding has no lossless canvas
guarantee, regardless of primary codec or exact.

### Original-code observation and protocols

The external include-source wrapper redirects only anim_encode.c's WebPEncode
calls through a test observer that invokes the normally linked original encoder.
While mixed SetFrame/CacheFrame runs, it captures the actual invocation config,
dimensions and complete memory-writer file, before loser buffers are released.
No eligibility or candidate-selection algorithm is duplicated in the observer.
Recording is disabled for existing requests and all other stages; the four-trial
bound is specific to the current keyframe-policy-disabled contract.

`AnimationMixed` prefixes the original 18-word candidate request with
`(primary_lossless, near_lossless, alpha_quality, SharpYUV)`. The original quality
word holds f32 bits, exact retains its original position, and opcode is 3.
After original SetFrame, output starts with last_config and last_config_reversed,
each seven words `(lossless, quality_f32_bits, method, near_lossless, exact,
alpha_quality, SharpYUV)`, then trial_count and each trial:

```text
lossless width height quality_f32_bits method near_lossless exact alpha_quality SharpYUV filter_strength autofilter file_byte_count complete_still_bytes...
```

Each byte occupies one u32. The existing candidate-or-skip response follows,
including winning mask, preceding disposal and complete winner file.

`AnimationMixedCarryoverSequence` retains the four-word global carryover header
`(width, height, minimize_size, input_count)`. Each input supplies eight words
`(ARGB_stride, quality_f32_bits, method, primary_lossless, near_lossless, exact,
alpha_quality, SharpYUV)` followed by strided ARGB without final-row padding.
Original CopyCurrentCanvas/CacheFrame/FlushFrames produce, per submitted input,
the mixed configs/trials/winner record above, then preceding rectangle and both
complete original/carryover canvases. Config snapshots include skipped inputs.
This protocol has no timestamps and does not claim complete mixed public
WebPAnimEncoderAssemble parity. All older request layouts remain unchanged.

### Fixtures, independent checks and results

`fixtures/animation_mixed.txt` contains 75 C-generated cases: 65 selections and
ten eight-input sequences. Pin and toolchain were reviewed before generation:
clean `1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1,
Release `-O3 -DNDEBUG`, SIMD/threading off, near-lossless enabled. Actual flags
remain in `target/native-oracle/provenance.txt`; upstream is unmodified. Review
those records before regenerating only this new file:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test animation_mixed animation_mixed_fixtures_match_pinned_reference -- --nocapture
```

The 4,237 live cases comprise 3,901 selections and 336 eight-input sequences
(2,688 transitions). Every trial's config, size and file, including losers,
matches C; all 9,008 files independently decode with expected dimensions. There
are 5,219 VP8L and 3,789 VP8 invocations, 52 distinct-stream ties, 4,083 VP8L
winners and 784 VP8 winners. Final masks/disposal and both raw canvases match.

Portable independent checks cover:

- Every palette size 1–260, including hidden RGB under zero alpha, distinct
  strides/padding and minimize-size bypass. C fixtures straddle 30/31 and
  193/194, and cover 256/257 colors, both primaries and both size policies.
- A 194-color original crop becomes fewer than 31 colors after transparency;
  observed original C still tries only VP8. Eligibility precedes mutation.
- A transparent hidden-RGB exterior and a small opaque change make primary
  VP8L choose BACKGROUND and primary VP8 choose NONE, although both winners
  are VP8L. Area selection follows the primary, not the eventual codec.
- Quality 61.5 keeps an eight-level change, while truncating to 61 would skip
  it. The float survives geometry/reversal; only VP8L effort is truncated.
- Shared exact changes VP8 bytes on transparent noisy regions with every
  method and both SharpYUV settings, preserving alpha. Exact-false bytes match
  the existing public still API. Explicit C fixtures capture both exact values.
- Both codecs validate even if palette or skipping would avoid them. Reversal
  round-trips every supported field. Original-word state transitions retain
  masks, preceding disposal, restoration and skipped-original semantics.
- A 30-color minimize-size trial at method 1, quality 3 produces distinct
  118-byte VP8L/VP8 files. VP8L wins the equal size even with VP8 primary,
  anchoring codec order and strict tie preservation in a committed C fixture.

Accepted candidates in ten fixture sequences receive unit durations, then the
existing independent still-decoder/compositor applies the selected preceding
disposal and straight-alpha rounding. Every composited canvas and cumulative
timestamp matches pinned WebPAnimDecoder exactly without tolerance, across VP8L,
VP8 and codec switches. These unit-duration checks do not implement timestamp
merging for skipped mixed inputs or single-frame reconstruction.

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --features native-oracle --test animation_mixed -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

The low-level cache remains VP8L-only. Lossy/mixed timestamp caching, fixed
overflow holds, deferred keyframes, decode-dependent single-frame reconstruction
and public RGBA Optimized remain open.
