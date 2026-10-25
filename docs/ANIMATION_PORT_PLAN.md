# Animation and multi-frame encoding plan

Planned 2026-10-24 against pinned libwebp
`1844042d1ca751e62e61df7617dba19c98588339`. Isolated pixel/rectangle leaves,
checked prepared-frame container assembly, full-frame raw RGBA sequences and
their native oracles are implemented; the animation optimizer remains open.
This expands the container/animation work in the
[encoder completion plan](ENCODER_COMPLETION_PLAN.md), building on the completed
[lossless](LOSSLESS_PORT_PLAN.md), [near-lossless](NEAR_LOSSLESS_PORT_PLAN.md)
and [lossy](LOSSY_PORT_PLAN.md) ports.

## Implemented foundations (2026-10-24)

[`src/mux/anim_encode.rs`](../libwebpop/src/mux/anim_encode.rs) ports
`ComparePixelsLossless`, `PixelsAreSimilar`, `ComparePixelsLossy`,
`IsEmptyRect`, `QualityToMaxDiff`, `MinimizeChangeRectangle`,
`SnapToEvenOffsets`, `WebPAnimEncoderRefineRect`,
`IsLosslessBlendingPossible` and `IsLossyBlendingPossible`. Checked borrowed
ARGB views accept separate pixel strides without final-row padding. Refinement
clips signed guesses using wide arithmetic and leaves them unchanged on
validation failure. These are low-level helpers under `mux::anim_encode`,
separate from the public sequence API.

The existing native oracle now links pinned scalar `libwebpmux` and includes
the original animation source in an external wrapper with renamed exports.
The lossless candidate milestone also links test-only `webpdemux` for exact
all-frame decoding checks; production remains independent of C and decoding.
There are 81 committed C fixtures and 80,287 live leaf comparisons, including
adjacent float values around quality thresholds. Independent checks exhaust
3x3 change masks and alpha/channel-difference pairs. See the
[animation reference record](../libwebpop/tests/reference/ANIMATION.md) for
protocol, provenance and validation commands/results.

The public prepared-frame layer now exposes `EncodedFrame`,
`AnimationContainer`, `AnimationContainerOptions`, `BackgroundColor`,
`ContainerFrameOptions`, `FrameBlend`, `FrameDispose` and `MuxError` from the
crate root. `EncodedFrame` provides checked lossless, near-lossless and lossy
RGBA encoding with methods 0–6, reusing `LosslessOptions`/`LossyOptions`, including alpha
quality and SharpYUV. It captures typed codec output directly at the still-writer
boundary; it does not parse RIFF input. Existing still options/defaults and
output bytes are preserved.

`muxinternal.rs`/`muxedit.rs` assemble RIFF/VP8X/ICCP/ANIM/ANMF/EXIF/XMP with
checked canvas area, even frame origins, containment, duration, chunk and file
sizes and fallible allocation before output mutation. Background has explicit
RGBA channels and BGRA serialization. Metadata is owned supplied bytes;
`None` omits a chunk and `Some(Vec::new())` writes a present empty chunk. The
builder owns only encoded rectangles, so valid canvases larger than codec
limits need no raw canvas allocation. Adds validate before mutation;
`assemble(&self, &mut Vec<u8>)` is repeatable and preserves output on errors.
This output-only builder accepts durations directly; the stateful sequence
layer below supplies timestamps, finish transitions and encoding policy.
A public rustdoc example demonstrates two prepared lossless frames.

There are 33 committed mux fixtures and 193 live complete-byte comparisons to
`WebPMux` supplied with exactly the same encoded payloads. Portable checks walk
every chunk and nested ANMF, inspect all controls/flags/padding, compare payloads
to the still writers, and exercise synthetic size bounds without large
allocations. Full-canvas single-frame mux cleanup is covered with every codec,
alpha and metadata. A separate reference-only fixture records the original
`OptimizeSingleFrame` decoding a single subrectangle and replacing its 94-byte
ANMF representation with a 44-byte full-canvas still. Rust reproduces the before
representation; the recorded after result is independently decoded, outside
the Rust parity corpus. Exact reconstruction remains an open completion gate.

The public raw-frame layer (2026-10-25) exposes `AnimationEncoder`,
`AnimationOptions`, `AnimationEncodingPolicy`, `AnimationFrameOptions` and
`AnimationError`. `new(width, height, options)` fixes the canvas;
`add_rgba(rgba, stride, timestamp_ms, frame_options)` immediately encodes each
full canvas as a replacement at (0,0), without disposal. Frame options select
lossless, near-lossless or lossy methods 0–6 using the existing still settings.
Global options reuse `AnimationContainerOptions` for loop/background/metadata.
`FullFrame` is the default policy; `Optimized` is explicitly rejected before
encoding. Repeats and zero durations remain separate submitted frames.

Timestamps are `u64`, non-decreasing with arbitrary origin; each interval must
fit 24 bits. `finish(end_timestamp_ms, output)` always requires explicit final
timing. Validation errors permit retry; encoding/allocation/assembly failures
after validation mark the encoder failed. Successful finish releases buffered
payloads and rejects subsequent operations. Finish errors preserve output.
No raw inputs are retained; encoded output remains buffered until assembly.
One submitted frame becomes a still and loses playback timing/loop settings.

There are 26 new committed C mux fixtures and live complete-byte sequence
comparisons. The existing independent `image`/`image-webp` animation decoder
returns every composited canvas and exact stored delay, including zero.
Both Rust and C output are decoded and compared to the corresponding still
codec decode; lossless alpha and visible RGB also compare directly to input.
Portable tests cover wide timestamps through `u64::MAX`, maximum/zero intervals,
validation recovery, terminal state, exact hidden RGB, all lossless methods,
near-lossless strengths and lossy color/alpha/SharpYUV settings. The reference
record documents the supported contract and fixture provenance.

Checked borrowed ARGB subframe views and `GetSubRect`/`GetSubRects` routing are
now implemented (2026-10-25). Views preserve the canvas stride without copying,
support nested rectangles and omit final-row padding. `SubFrameParams` owns
only rectangle controls and optional borrowed views. Routing preserves first
frame versus later keyframe behavior, empty skipping versus forced 1x1 frames,
even-offset expansion and lossless-first seeding of the lossy rectangle.
There are 47 committed scalar C fixtures and 17,340 live comparisons of
rectangles, view dimensions/strides and every visible view pixel. Independent
checks cover all 3x3 change masks and 24,576 flag/quality/difference combinations,
plus checked nested views, validation rollback and clearing stale skipped views.

The lossless candidate path is implemented (2026-10-25) under
`mux::anim_encode`: checked mutable views, `IncreaseTransparency`,
`DisposeFrameRectangle`, lossless `EncodeCandidate`/`PickBestCandidate` and
`SetFrameLossless` (the lossless subset of `SetFrame`/`GenerateCandidates`).
It evaluates previous-disposal alternatives in pinned order, applies area
selection or minimize-size trials, preserves the earlier candidate on equal
byte size and returns the winning encoded rectangle, blend control,
preceding-frame disposal and carryover mask. Independent candidate copies
prevent transparency/codec cleanup from contaminating later trials. Lossless
methods 0–6 and codec-level `exact` are supported. Near-lossless is extended
below; lossy and mixed candidates remain deferred. `exact` does not preserve
invisible RGB discarded by transparency substitution during blending.

There are 79 committed C fixtures and 3,120 live comparisons of mutations,
candidate controls/masks and complete payload bytes against original C helpers
and lossless `SetFrame`. Independent portable RIFF/VP8L decoding and exact
endpoint-alpha compositing match 48 all-frame native `WebPAnimDecoder` runs.
The `image-webp` animated compositor rounds some opaque blended pixels down
by one; it remains suitable for full-frame replacements, while candidate tests
use its still decoder plus the independent compositor and pinned decoder.
This decoder limitation is documented, not hidden behind pixel tolerances.

Lossless carryover canvas updates are implemented (2026-10-25):
`CopyIdenticalPixels`, `CopyMaskedPixels` and
`LosslessCandidate::update_canvases` port the canvas-update branches extracted
from `CacheFrame`. Unchanged original words restore the carryover canvas before
selection; accepted candidates apply the selected preceding disposal, preserve
masked pixels and the rectangle exterior, then save the original input as
`prev_canvas`. Keyframes copy the full original canvas, including invisible RGB;
skipped frames leave the previous original and rectangle unchanged. Checked
separately strided destinations preserve padding and validate before mutation.

There are 32 committed C fixtures and 768 live comparisons: 96 copy-helper
cases and 672 eight-frame sequences (5,376 transitions) against original
`CacheFrame`/`FlushFrames` snapshots. Every selected rectangle, blend, preceding
disposal, winning mask, complete payload and both resulting canvases match.
Fifteen assembled sequences also match the independent compositor and native
all-frame decoder exactly. Synthetic carryover histories independently check
disposal-before-mask order, retained pixels and validation rollback.

Lossless caching and duration merging with keyframes disabled are implemented
(2026-10-25) as the low-level `mux::anim_encode::LosslessAnimationCache`.
`add_argb` consumes checked borrowed ARGB views with per-frame lossless options;
the cache owns two original/carryover canvases, two candidate slots and buffered
container output. It retains one pending frame so the next candidate can select
its disposal. Skipped frames merge timing; an overflowing merged duration
inserts the pin's fixed transparent 1x1 lossless hold, resets the preceding
rectangle and flushes before another candidate can fill the cache. Final timing
is explicit, validation errors permit retry, and post-validation failures are
terminal. Frame indexes count submitted inputs, including skips.

`LosslessAnimationCacheOptions` supplies minimize-size and owned container
settings. `LosslessCacheStatus` reports cache counters, preceding rectangle and
pending duration for intermediate comparisons; flushing still buffers encoded
output and does not stream bytes. The RGBA sequence API continues to reject
`Optimized`; lossy/mixed routes and keyframe settings remain open. Near-lossless
uses the same cache with the extension below.

There are 32 cache fixtures and 1,680 live timing/cache/file comparisons across
13,440 submitted frames: 1,470 compare complete files to public
`WebPAnimEncoderAssemble`, and 210 explicitly stop before decode-dependent
single-frame optimization. Twenty-nine timed fixture sequences match both the
independent compositor and pinned all-frame decoder. A separate native gate
merges eight transparent inputs into one 90-byte cropped animation; C's decode
and re-encode optimization reduces it to a 34-byte still. Rust preserves the
90-byte pre-optimization representation, keeping this parity branch open.

Near-lossless VP8L candidates and cached sequences are implemented (2026-10-25).
`encode_near_lossless_candidate`, `set_frame_near_lossless` and
`LosslessAnimationCache::add_near_lossless_argb` accept signed strength 0–100
alongside existing lossless effort/method/exact options. Strength 100 preserves
the existing candidate and cache entry points byte-for-byte. Validation precedes
cache mutation. Rectangles, eligibility and masks use original input words;
transparency substitution precedes the existing codec's palette/predictor/
standalone routing on each selected rectangle. Raw canvases are never globally
preprocessed. Original input, rather than decoded pixels, still drives carryover
and repeat merging, even when a later input changes strength to 100.

There are 64 new C fixtures and 4,893 live near-lossless comparisons: 3,773
candidate selections and 1,120 eight-frame cache sequences (8,960 submitted
inputs). Of the sequence cases, 980 compare full public animation assembly and
140 explicitly stop before single-frame reconstruction. Twenty-four timed
fixture animations match the independent compositor and pinned all-frame
decoder exactly. Portable tests establish actual RGB/intermediate-alpha changes,
palette bypass, exact predictor bypass versus active standalone preprocessing,
the crop-size shortcut and transparency-induced palette routing. Strength below
100 has no lossless canvas guarantee; switching to 100 does not force unchanged
pixels from earlier near-lossless frames to be encoded again. The decode-based
single-frame gate and public RGBA `Optimized` policy remain open.

The isolated lossy `EncodeCandidate` path is implemented (2026-10-25) as
`mux::anim_encode::encode_lossy_candidate`, returning a `LossyCandidate` with
typed VP8/ALPH payload, rectangle, initial controls and complete encoded size.
Blended candidates use internal filter strength zero and VP8 profile 2;
replacement candidates retain strength 60 and profile 0. Autofilter remains off.
Both paths reuse method 0–6, alpha quality and SharpYUV propagation. Public still
options, prepared full-frame encoding and their default bytes are unchanged.
This helper encodes a supplied checked ARGB rectangle; it does not select one,
check blend eligibility, flatten pixels or update carryover.

There are 56 new C fixtures and 4,704 live comparisons against the pin's original
`EncodeCandidate`, covering initial controls and complete payloads. Every file
is independently decoded. The existing independent VP8 arithmetic reader checks
profile and global/absolute segment filter levels, requiring all blended levels
to be zero. Paired fixtures establish actual decoded RGB changes for every
method, identical alpha payloads and decoded alpha across the override, and
replacement identity with the public still encoder. Lossy selection/carryover,
mixed trials and keyframes remain open.

## Scope and delivery order

Deliver useful multi-frame encoding before completing the animation optimizer:

1. A checked output container layer for internally encoded frame payloads,
   frame rectangles, timing, loop/background settings and supplied metadata.
2. A raw RGBA sequence encoder that writes full-canvas replacement frames using
   the existing still encoders. This is the first usable animation release.
3. A direct scalar port of `WebPAnimEncoder` selection and caching, starting
   with lossless, then lossy/mixed candidates and keyframe controls.
4. A CLI frontend for an ordered sequence of image files with explicit timing.

The first release needs no new pixel codec, general advanced still-image API,
SIMD or worker implementation. Optimized lossy candidates do need an additional
filter-control integration described below. Keep the existing still APIs and
their defaults unchanged. A WebP animation contains independently coded VP8 or
VP8L rectangles; there is no VP8 inter-frame video coding to port.

The completed [additional lossy methods plan](LOSSY_METHODS_PLAN.md) owns methods 0–6,
direct/token loop selection, trellis, method-dependent alpha effort and the
shared `LossyOptions`/`encode_lossy_rgba` API. Animation consumes those codec
routes; it does not duplicate their algorithms or create another lossy options
type. Its opaque/transparent still-file gates now pass for every method;
prepared-frame and full-canvas sequence routes carry all methods, with method 4
remaining the default. Mixed mode requires the selected method and
settings to be supported by every candidate codec, even when a heuristic might
ultimately choose only one; reject unsupported combinations before encoding.

That plan deliberately fixes the default filter settings. Animation's blended
lossy filter override is now a separate internal integration, preserving those
still-image defaults and their fixtures. Likewise, adapt its shared
single-partition result at the encoded-frame payload boundary below; animation
does not select the direct/token loop or change coefficient-partition policy.

Production remains safe Rust, without C/Wasm dependencies. Input to the library
is prepared pixels, not GIF/APNG/video or another WebP container. General
demuxing, editing existing files, image decoding and metadata extraction remain
outside this plan. The CLI can reuse its existing still-image loaders. Animated
input import, video timestamps and audio are separate frontend projects.

## Reference and compatibility boundaries

Use the pin as the algorithm and byte oracle. References:

- [`mux.h`](../upstream/src/webp/mux.h) and
  [`mux_types.h`](../upstream/src/webp/mux_types.h): animation options and enums.
- [`anim_encode.c`](../upstream/src/mux/anim_encode.c) and
  [`animi.h`](../upstream/src/mux/animi.h): rectangles, candidates, carryover,
  keyframes, caching, timestamps and finalization.
- [`muxedit.c`](../upstream/src/mux/muxedit.c),
  [`muxinternal.c`](../upstream/src/mux/muxinternal.c) and
  [`muxi.h`](../upstream/src/mux/muxi.h): output validation and chunk assembly.
- The pinned [container specification](../upstream/doc/webp-container-spec.txt)
  and [published specification](https://developers.google.com/speed/webp/docs/riff_container).

There are two distinct byte comparisons. Full-frame sequence output must match
pinned `WebPMux` given identical encoded payloads and frame controls. The
optimized encoder must match pinned `WebPAnimEncoder` for supported settings,
including selected rectangles, frame merging and candidate ties. Full-frame
assembly alone does not establish `WebPAnimEncoder` parity. Keep the two test
families and API policies explicit; do not silently change full-frame output
when the optimizer lands.

Two upstream behaviors need special treatment:

- **Single-frame optimization depends on decoding.** Mux cleanup can unwrap a
  lone full-canvas frame without decoding; implement that directly. Separately,
  `OptimizeSingleFrame` calls `FrameToFullCanvas`/`DecodeFrameOntoCanvas`,
  decodes a remaining subrectangle, re-encodes the full canvas, and selects it
  if smaller. One input frame, or many merged identical frames, can reach this
  branch. Re-encoding original input is not generally equivalent, especially
  for lossy/near-lossless. Keep this optimizer milestone open until exact
  reconstruction is available or a separate decoder scope decision is made.
  Earlier releases may retain the valid single-ANMF representation, but must
  document this unsupported parity branch and must not claim complete pinned
  animation parity. Test its existence from the first oracle milestone.
- **Lossy blended candidates disable filtering.** `EncodeCandidate` sets
  `autofilter=0` and `filter_strength=0` for blended lossy frames. The public
  still route's fixed default filter cannot supply this behavior. Add and test
  the minimal internal config-bearing route before enabling those candidates;
  no requirement to finish every advanced still option first.

## Rust API and ownership

The full-frame contract is implemented below. Optimizer-specific settings and
normalization remain future work in milestone 0 and milestones 3–4.

| Surface | Contract |
| --- | --- |
| `AnimationOptions` | Encoding policy and owned `container: AnimationContainerOptions` for loop/background/metadata; canvas dimensions are arguments to `AnimationEncoder::new` |
| `AnimationFrameOptions` | Enum: `Lossless(LosslessOptions)`, `NearLossless { strength, options: LosslessOptions }`, `Lossy(LossyOptions)` |
| `AnimationEncoder::add_rgba` | Immutable full-canvas RGBA view, positive byte stride, start timestamp in integer milliseconds, frame options |
| `AnimationEncoder::finish` | Explicit end timestamp and final `&mut Vec<u8>`; determines the last duration and assembles one file |
| `AnimationError` | Canvas/policy/state/empty errors; encoding, timing and container errors carry zero-based frame indexes (finish refers to the final frame) |

Start with full-canvas straight-alpha RGBA input. Borrow input only during
`add_rgba`; own every canvas, mask and encoded candidate needed afterward. Use
the existing checked strided importer, including optional final-row padding and
unchanged-input guarantees. Do not accept caller subrectangles as if they were
complete display frames. A separate container builder can take explicit encoded
rectangles and blend/dispose controls; the optimizer chooses those controls
from complete display frames.

Use integer timestamps with a wide cumulative range (`u64`) and checked
differences. Accept non-decreasing timestamps, including zero durations; never
invent a minimum browser delay or convert through floating point. Require each
supplied interval to fit `0..=0x00ff_ffff` milliseconds. A duration convenience
adapter may accumulate timestamps with checked arithmetic. Support an arbitrary
first timestamp: it establishes the origin, not a delay before the first frame.
The Rust API need not emulate signed C timestamp wraparound; oracle adapters
must preserve equivalent intervals when exercising the pin's wrapping arithmetic.

Require explicit final timing in the Rust API instead of upstream's optional
average-duration estimate. Document this API difference and supply the final
NULL-frame timestamp in C comparisons. Empty input is an error. After successful
finish, adding or finishing again is a state error. Validate an add before
mutation; validation failures leave the encoder usable. If an allocation or
encoding failure occurs after state changes, mark it failed and reject further
operations. This avoids promising costly rollback of all candidate buffers.
Finish errors leave the caller's output unchanged, following current APIs.

Expose a typed full-frame versus optimized policy. Start full-frame only and
reject unavailable policies. For optimized options, preserve upstream defaults:
white background (`0xffffffff`), infinite loop (`0`), keyframe insertion disabled,
`minimize_size=false`, `allow_mixed=false`. Define background channels explicitly
and serialize BGRA; do not confuse this with the public RGBA input layout.
Represent all-keyframes, disabled keyframes and bounded intervals distinctly.
Port `SanitizeEncoderOptions` normalization (including its 30-frame window cap)
into the mapping from upstream-style interval controls and test the effective
values. Avoid printing upstream `verbose` messages from the library.

Use existing still configuration types where possible. Lossless effort quality
and lossy fidelity quality have different meanings; mixed mode must follow the
pin's config reversal and candidate overrides, not interpret them as a common
visual quality scale. Near-lossless uses the completed VP8L route and retains
its intermediate-alpha caveat. `exact` must not promise preservation of invisible
canvas RGB after animation blending or carryover optimization; specify and test
its narrower codec-level meaning before exposing it on optimized input.

## Output layer and validation

Add `src/mux/` with `muxinternal.rs`, `muxedit.rs` and `anim_encode.rs`, preserving
upstream function names for ported algorithms. Put safe API types/exports in
`mux/mod.rs` and re-export the intended animation API from the crate root.
Use Rust vectors/enums rather than C linked lists and copy/free switches.
Keep codec logic in `enc/`; carry upstream `WEBP_INLINE` as plain `#[inline]`.

Introduce an internal typed encoded-frame payload: VP8L, or VP8 with optional
ALPH, dimensions and alpha information. Factor it from the existing VP8L
`WriteImage` and VP8 `AssembleWebP` boundaries without changing still output.
Do not nest complete RIFF files inside ANMF or remove a fixed number of header
bytes: lossy-alpha stills already contain VP8X/ALPH. If a public prepared-frame
route is exposed, produce its opaque payload through checked encoding methods;
arbitrary caller-supplied WebP parsing remains excluded.

Implement and test these output rules against the pinned sources:

- Write RIFF/WEBP, VP8X, optional ICCP, ANIM, ordered ANMF frames, then EXIF and
  XMP. Set feature flags from the actual output, including alpha in any frame.
  Metadata is supplied bytes, with no profile conversion or extraction.
- ANMF contains its 16-byte header and either VP8L, or optional ALPH followed by
  VP8. Chunk lengths exclude padding; every odd payload gets a zero pad byte.
  Zero reserved fields and encode blend/dispose bits with upstream polarity.
- Frame origins are even and serialized divided by two; width/height are
  serialized minus one. The checked rectangle builder rejects odd origins.
  Upstream `WebPMuxPushFrame` rounds them down; make this Rust API difference
  explicit. The optimizer instead expands/refines bounds before even snapping
  so changed pixels cannot be lost. Validate origin plus extent inside canvas.
- Durations are 24 bits; loop count is 16 bits, with zero meaning infinite.
  Apply disposal to the previous frame's rectangle before rendering the next.
  Keep format background metadata distinct from the pin's transparent internal
  canvas clearing; viewers can treat the background color as a hint.
- Keep container and codec limits separate. VP8X allows axes through `2^24`
  and area at most `2^32-1`; the pinned mux further limits frame offsets to
  less than `2^24` pixels. Existing codecs accept frame axes only through
  16383. Initially restrict raw full-canvas input to the existing codec limits;
  a prepared-rectangle builder may use larger valid canvases without allocating
  a full pixel canvas. Do not promise oversized raw-canvas tiling.
- Check payload, nested chunk, padded size and total RIFF arithmetic before
  allocation/narrowing, including 32-bit hosts. Enforce the WebP file limit of
  `2^32-2` bytes and existing fallible allocation conventions. Test synthetic
  boundary sizes without allocating multi-gigabyte buffers.
- Preserve pinned mux cleanup for a lone full-canvas frame, including removing
  animation chunks/flags while retaining supplied metadata. Document that a
  sequence may become a still image and then has no playback duration/loop.

## Milestones and acceptance

### 0. Contract and native oracle

- [ ] Finalize the types, timing/state rules and supported codec matrix above.
  Full-frame types and timing/state rules are finalized and tested for lossless,
  near-lossless and lossy methods 0–6. Optimizer settings/keyframe normalization remain open.
- [x] Extend the existing `native-oracle` CMake build: enable
  `WEBP_BUILD_LIBWEBPMUX` and link `libwebpmux` for animation wrappers. Link
  `webpdemux` only if needed for test inspection/decoding. Preserve scalar,
  synchronous configuration and the unmodified upstream checkout.
- [ ] Add `tests/reference/animation*.c`, logical requests in the existing
  dispatcher/evaluator, and `tests/reference/ANIMATION.md` with pin, toolchain,
  protocol and fixture provenance. Wrap public mux/animation APIs and selected
  static helpers outside the submodule, following the existing include-source
  wrapper convention without duplicate linked definitions. Leaves, output mux
  and the reference-only decode-dependent single-frame path now have wrappers
  and provenance; lossless candidate selection and all-frame decoding now have
  wrappers too. Carryover sequences observe original CacheFrame/FlushFrames;
  default lossless cache/timing and full-file protocols are now implemented.
  Keyframe-control protocols and lossy/mixed candidates remain open.
- [ ] Record fixtures for raw mux assembly, optimized rectangles/candidates,
  default and explicit keyframe options, duration merging/splitting and both
  single-frame cleanup paths. Capture the decode-dependent case as an open
  completion requirement, not an ignored mismatch in a passing parity corpus.
  Raw mux assembly, both single-frame paths, isolated lossless candidate
  selection, carryover-state sequences and default lossless optimized files
  with duration merging/splitting are now recorded. Explicit keyframe options
  and lossy/mixed optimized files remain open.

### 1. Container assembly and metadata

- [x] Factor typed codec payload output and port output-only mux assembly.
- [x] Add checked canvas/frame controls, global animation settings and metadata.
- [x] Compare complete bytes to `WebPMux` for identical prepared payloads;
  independently inspect every field, padding byte and calculated size.
- [x] Re-run still-image parity after factoring writers. This milestone also
  supplies the completion plan's still-image metadata output foundation.

### 2. Usable raw-frame encoding

- [x] Implement the stateful RGBA API and explicit final timestamp, initially
  encoding every submitted canvas at (0,0), no blend, no disposal. Use current
  lossless, near-lossless and method-4 lossy codecs, including alpha quality.
- [x] Preserve submitted timing, including repeated images and zero durations.
  Full-frame mode does not merge repeats; optimized mode will.
- [x] Decode all output frames with an independent animation decoder, checking
  sequence, canvas and cumulative timing. Compare lossless visible pixels and
  alpha to input; compare lossy/near-lossless to the corresponding reference
  codec decode. Byte comparison uses the matching C mux assembly route.
- [x] Add a public usage example and document supported settings, memory and
  the distinction from optimized `WebPAnimEncoder` output.

This is the minimum useful release. A sequence iterator can feed it without
retaining every raw input image. Encoded frames remain buffered until assembly;
neither this API nor cache flushing promises constant-memory output streaming.
The implementation is `src/mux/sequence.rs`, with a compiled public usage
example. Tests live in `tests/animation_sequence.rs`; see the milestone-2
section of the [reference record](../libwebpop/tests/reference/ANIMATION.md).

### 3. Lossless animation optimizer

- [x] Port pixel comparison, change-rectangle minimization/refinement, empty
  rectangle handling and even-offset expansion as isolated leaves.
- [x] Port subframe views and `GetSubRect`/`GetSubRects` routing. Checked borrowed
  ARGB views, `SubFrameParams` and both routing helpers match 47 committed
  fixtures and 17,340 live scalar C comparisons; independent exhaustive bounds
  and borrowed/nested-view tests verify geometry, stride and visible pixels.
- [x] Port blend eligibility, `IncreaseTransparency`, disposal alternatives,
  candidate encoding and strict size/tie selection in upstream evaluation order.
  Lossless subset: 79 fixtures, 3,120 live C comparisons, selected masks and
  previous disposal, 48 exact all-frame decoder comparisons. Lossy/mixed
  extensions belong to milestone 4; cache integration remains below.
- [x] Port the pin's `prev_canvas`, `canvas_carryover`, candidate/best carryover
  masks, `CopyIdenticalPixels` and `CopyMaskedPixels` updates. Comparing only
  adjacent original inputs is insufficient after earlier pixels are carried.
  Lossless canvas updates match 32 fixtures and 768 live helper/sequence cases,
  including 5,376 frame transitions and 15 exact all-frame decoder checks.
  These low-level updates do not yet implement Rust cache/timing policy.
- [x] Port lossless `SetFrame`, `CacheFrame`, `FlushFrames` and duration
  accumulation with keyframe insertion disabled. `SetFrameLossless` is now
  integrated into `LosslessAnimationCache`; repeats merge without changing
  total duration. Port
  `IncreasePreviousDuration`'s transparent 1x1 hold frame when a merged duration
  would reach `2^24`, including the two-slot capacity flush and count/rectangle
  updates. Deferred-keyframe freezing/rebasing on a split remains in milestone 4.
- [x] Compare supported keyframe-disabled lossless intermediates and full files
  with pinned animation. There are 32 fixtures, 1,680 live cases (1,470 complete
  public animation files and 210 before single-frame reconstruction), 13,440
  submitted-frame cache snapshots and 29 timed all-frame decoder checks. The
  decode-dependent single-frame completion gate remains explicitly open.
- [x] Add near-lossless with its actual routing and decoded-pixel evidence;
  do not apply an independent lossless pixel guarantee below strength 100.
  Per-candidate strength and the shared cache match 64 fixtures, 4,893 live C
  cases and 24 all-frame decoder checks. Original-word carryover and strength
  changes on repeats are covered; strength-100 output is unchanged.

### 4. Lossy, mixed mode and keyframes

- [x] Add the internal lossy filter override and compare it independently to C,
  preserving the methods plan's default-filter fixtures. Reuse its validated
  method propagation and method-dependent alpha job for every enabled method.
  Isolated VP8 EncodeCandidate now matches 56 fixtures and 4,704 live original-C
  comparisons, including profile 2, zero filter levels and exact alpha-path
  preservation. Rectangle selection and lossy carryover remain below.
- [x] Port `QualityToMaxDiff`, alpha-weighted similarity and lossy rectangle/blend
  checks as isolated leaves, preserving floating evaluation and integer
  truncation at heuristic thresholds.
- [ ] Port `FlattenSimilarBlocks` and lossy carryover updates.
- [ ] Port mixed candidate eligibility (palette thresholds, both codecs in
  minimize-size mode), config reversal and winner selection. Ordinary mixed
  mode does not unconditionally try both codecs on every frame.
- [ ] Port `KeyFramePenalty`, candidate caching, deferred keyframe selection,
  `kmin`/`kmax`, all-keyframes mode and minimize-size disabling of insertion.
  Include `IncreasePreviousDuration` freezing/rebasing an already selected
  deferred keyframe before a hold-frame split, with cache-boundary tests.
  A VP8 intra frame in a cropped/blended ANMF is not automatically a full-canvas
  animation keyframe.
- [ ] Verify output bytes, candidate order, selected disposal on the preceding
  frame, masks and cache transitions for supported settings. Resolve the
  decode-dependent single-frame optimization before declaring this a complete
  `WebPAnimEncoder` port; retain its explicit completion gate otherwise.

### 5. CLI and resource baseline

The sequence CLI can ship immediately after milestone 2. Add optimizer switches
and comparative measurements as milestones 3–4 become available.

- [ ] Add a separate `cwebpop` animation subcommand/binary, keeping current
  single-image invocations compatible. Proposed interface: `cwebpop animate
  --frame 100 first.png --frame 40 second.png --loop 0 -o out.webp`, where each
  frame argument is a duration in milliseconds followed by its image path.
  Document a concrete equivalent if parser constraints require another spelling.
- [ ] Require equal canvas dimensions and explicit durations; use ordered
  arguments rather than filesystem/glob ordering or an invented frame rate.
  Load one image at a time with existing loaders. Reject animated input in this
  sequence frontend until a deliberate multi-frame loader is implemented.
- [ ] Expose supported codec settings and optimizer controls as they land;
  reject unsupported combinations before opening input/output. Support stdout
  with diagnostics on stderr, and avoid replacing an output file on failure.
- [ ] Add CLI sequence/timing/error tests and update `cwebpop/README.md` and
  library examples. GIF/APNG import, frame extraction and input metadata copying
  are not prerequisites.
- [ ] Record release timing, encoded size, peak working memory and total
  buffered output versus pinned scalar C on moving sprites, transparent fades,
  screenshots, near-static sequences and photographic frames. Include long
  repeated-frame sequences. Separate full-frame and optimized measurements;
  correctness is mandatory, performance measurements are initially informational.

## Validation matrix and completion gates

Use portable committed C fixtures, independent structural/compositing checks,
and live pinned comparisons. Extend the current harness rather than introducing
a second oracle stack. An independent animation decoder must return every
composited canvas and timing, not only the first image; verify that capability
before choosing the existing `image` test dependency or extending dev tooling.
Also compare Rust and reference output through the same decoder. Account for
decoder background conventions and zero-delay normalization separately from
stored ANMF durations. These are test dependencies, not production decoding.

Cover at least:

- 1x1, odd/skinny/padded inputs, one/two/many frames, empty input and inconsistent
  dimensions; unchanged source and unchanged output on failure.
- Identical frames, all-transparent sequences, hidden RGB changes, changes on
  odd origins, last-row/column and opposite-corner changes, sprites leaving old
  areas, alpha 0/1/254/255, fades and semitransparent overlaps.
- Both blend modes and both disposal modes, clear/reset across loops, lossy ALPH
  and VP8L transparency, mixed codec winners, palette heuristic boundaries,
  equal-size ties, and single-frame collapse with/without metadata.
- Zero duration, nonzero first timestamp, final duration, descending timestamps,
  `2^24-1` accepted intervals, `2^24` rejected intervals, merged-duration overflow,
  wide cumulative timelines and arithmetic overflow. Test split insertion at
  cache boundaries and after a keyframe candidate has been selected.
- Keyframe disabled/all/bounded policies, interval normalization and cache cap,
  minimize-size, per-frame config changes, every advertised codec setting,
  malformed geometry/options, size bounds and invalid state transitions.

Run focused tests with `animation` in their names as each milestone lands:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop animation
cargo test -p libwebpop --features native-oracle animation -- --nocapture
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test -p cwebpop
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

Basic multi-frame support is complete at milestones 0–2, with CLI delivery in
milestone 5. Optimized support additionally requires milestones 3–4 and measured
coverage of their branch matrix. Full pinned animation completion additionally
requires resolving the single-frame decode dependency and any still settings
not yet advertised. Update this plan and the parent completion checklist with
implementation, fixture and benchmark evidence; do not mark omitted branches
complete on the strength of valid playback alone.
