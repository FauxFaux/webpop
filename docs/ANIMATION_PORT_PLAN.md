# Animation and multi-frame encoding plan

Planned 2026-10-24; remaining work replanned 2026-10-25 against pinned libwebp
`1844042d1ca751e62e61df7617dba19c98588339`. Prepared-frame containers and the
full-frame RGBA sequence API are usable. Candidate selection and shared timestamp
caching support all three codec routes with keyframes disabled.
Keyframe normalization/storage and ordinary-duration deferred selection are
implemented. Keyframe/overflow integration, the public optimized RGBA policy
and sequence CLI remain open.

This expands the [encoder completion plan](ENCODER_COMPLETION_PLAN.md), building
on the completed [lossless](LOSSLESS_PORT_PLAN.md),
[near-lossless](NEAR_LOSSLESS_PORT_PLAN.md), [lossy](LOSSY_PORT_PLAN.md) and
[additional lossy methods](LOSSY_METHODS_PLAN.md) ports. Production decoding
remains excluded pending discussion; this revision does not authorize a change.

## Implemented baseline

Completed milestone detail, fixture counts, protocols and validation results
live in the [animation reference record](../libwebpop/tests/reference/ANIMATION.md)
and git history. Preserve that evidence rather than repeating it in this plan.
Milestone numbers 0–5 retain their original meaning; remaining work is split below.

| Area | Implemented boundary | Main implementation/tests |
| --- | --- | --- |
| 0. Contract/oracle foundation | Scalar pinned mux/animation wrappers, committed fixtures, independent compositing and native all-frame decoding; optimizer contract/protocol extensions remain below | `tests/reference/animation.c`, `tests/reference/ANIMATION.md`, `tests/support/animation*.rs` |
| 1. Container/metadata | Typed VP8L or VP8/ALPH payloads; checked RIFF assembly, supplied metadata and full-canvas single-frame cleanup; byte parity with matching WebPMux inputs | `src/mux/{mod,muxinternal,muxedit}.rs`, `tests/animation_mux.rs` |
| 2. Full-frame RGBA | Per-frame lossless, near-lossless and lossy methods 0–6; explicit final timing; replacements without merging; checked state/error contract | `src/mux/sequence.rs`, `tests/animation_sequence.rs` |
| 3. VP8L optimizer subset | Checked ARGB subviews, geometry/blend leaves, candidates, original-word carryover, near-lossless strengths and duration merging/overflow holds with keyframes disabled | `src/mux/anim_encode.rs`, `anim_encode/{lossless,cache}.rs`, `tests/animation{,_subframes,_candidates,_carryover,_cache,_near_lossless}.rs` |
| 4/4a. VP8/mixed candidates and cache | Blended VP8 filter override, FlattenSimilarBlocks, lossy/mixed SetFrame, palette eligibility, config reversal, trial order/ties, original-word carryover and shared keyframe-disabled timestamp caching/holds | `anim_encode/{lossy,mixed}.rs`, `tests/animation{_lossy_candidates,_lossy_carryover,_mixed,_codec_cache}.rs` |
| 4b. Keyframe normalization/storage | Ordered option sanitization, checked typed controls, bounded owned alternatives and expanded C observer | `anim_encode/keyframes.rs`, `tests/animation_keyframes.rs` |
| 4c. Deferred selection | Ordinary-duration keyframe windows across VP8L/VP8/mixed routes, signed penalties, ties, disposal and prefix flushing; enabled overflow remains gated | `anim_encode/cache.rs`, `tests/animation_keyframes.rs` |

Paths in the table are relative to `libwebpop/`; candidate module paths are
relative to `libwebpop/src/mux/`. Existing candidate generators and canvas
updates are shared by the cache. The shared
`AnimationCache` owns two original/carryover canvases, saved submitted/reversed
configs and bounded cache slots; `LosslessAnimationCache` adapts its established VP8L
methods to that owner. Its owned slots can hold both subframe/keyframe
alternatives and normalized windows reserve up to 31 slots. Existing constructors
retain the disabled two-slot policy; `AnimationCache::new_with_keyframes` enables
ordinary-duration selection. It rejects duration splits before mutation until
keyframe/overflow integration (4d) lands.

There is no production decoder or C/Wasm dependency. The low-level cache stops
before decode-dependent single-frame reconstruction. `AnimationEncoder::new`
still rejects `AnimationEncodingPolicy::Optimized`. Candidate/carryover sequence
tests with unit durations do not establish timestamp-cache or public API parity.

## Scope and compatibility boundaries

Use the pin as the algorithm and byte oracle:

- [`anim_encode.c`](../upstream/src/mux/anim_encode.c) and
  [`animi.h`](../upstream/src/mux/animi.h): candidates, carryover, keyframes,
  caching, timestamps and finalization.
- [`mux.h`](../upstream/src/webp/mux.h),
  [`mux_types.h`](../upstream/src/webp/mux_types.h),
  [`muxedit.c`](../upstream/src/mux/muxedit.c),
  [`muxinternal.c`](../upstream/src/mux/muxinternal.c) and
  [`muxi.h`](../upstream/src/mux/muxi.h): controls, assembly and validation.
- The pinned [container specification](../upstream/doc/webp-container-spec.txt).

Full-frame sequences compare bytes with WebPMux given identical payloads and
controls. Optimized sequences compare with WebPAnimEncoder, including rectangle
selection, merging and ties, within an explicitly stated supported subset.
Keep these test families and public policies separate. Existing still APIs,
full-frame output and their defaults remain stable.

Production stays safe Rust with fallible allocations. Keep codec logic in
`enc/`, output/cache logic in `mux/`, and upstream names where porting algorithms.
Carry upstream WEBP_INLINE annotations as plain `#[inline]`. Reuse typed payloads
from the existing still-writer boundaries; never strip a fixed RIFF header or
nest a whole still file in ANMF. Arbitrary encoded-input parsing, general
mux editing/demuxing, metadata extraction, animated input import, GIF/APNG/video
and audio remain outside this work. The CLI may reuse existing still loaders.
SIMD, worker support and advanced still options are separate projects.

Two boundaries must remain explicit:

- Blended VP8 candidates use autofilter off, filter strength zero and profile 2;
  replacements retain existing defaults. Shared mixed exact handling is internal
  and does not extend the public still options. Preserve the existing fixtures.
- OptimizeSingleFrame can decode a remaining rectangle, re-encode a full canvas
  with saved configurations and select it if smaller. This is distinct from
  already implemented full-canvas mux cleanup. One submitted input or many
  merged inputs can reach it. See the discussion gate below; original input is
  not generally equivalent to decoded VP8/near-lossless pixels.

## Contracts to preserve and finish

| Surface | Contract |
| --- | --- |
| `AnimationOptions` | Encoding policy and owned container options; dimensions are passed to `AnimationEncoder::new`; optimizer options still need a checked mapping |
| `AnimationFrameOptions` | Lossless, near-lossless or lossy still settings; mixed primary/reversal mapping remains to be defined for optimized input |
| `add_rgba` | Immutable full-canvas straight-alpha RGBA, positive byte stride, no retained caller borrow, unchanged input |
| `finish` | Required explicit final u64 timestamp and `&mut Vec<u8>` output; no implicit average duration |
| Errors/state | Zero-based submitted-frame indexes, including skips; finish refers to the final input; validation failures allow retry, post-validation allocation/encoding/assembly failures are terminal |

Preserve optional final-row padding through the checked importer. Optimized
encoding owns all canvases, masks, configurations and deferred payloads it needs
after an add. Caller subrectangles are not complete display frames. Prepared
encoded rectangles continue to use the separate container builder.

Timestamps are non-decreasing u64 with arbitrary first origin and intervals in
`0..=0x00ff_ffff` milliseconds. Zero durations are accepted; no browser-delay
normalization is applied. Use checked differences and accumulation. The C
adapter preserves equivalent intervals through signed timestamp wraparound;
Rust does not emulate that signed representation. Empty finish is an error;
successful finish releases buffers and permanently rejects adds/repeated finish.
Every finish error preserves caller output. Flushing only moves encoded frames
to the buffered container; it does not stream bytes or bound total output memory.

Lossless effort and VP8 fidelity quality have different meanings. Mixed mode
preserves f32 quality for geometry/VP8 and truncates only for VP8L effort. Validate
all participating codec configurations before skipping/eligibility shortcuts.
Config reversal changes only the primary codec. Near-lossless operates on each
selected rectangle after transparency substitution; strengths below 100 may
change RGB and intermediate alpha. Original input words drive carryover and
repeat detection. Changing strength to 100 does not re-encode unchanged carried
pixels. Codec exact does not preserve invisible canvas RGB removed by blending.

Retain checked container invariants and their existing independent tests:

- RIFF/WEBP, VP8X, optional ICCP, ANIM, ordered ANMF, EXIF and XMP; actual feature
  flags, reserved zeros, nested sizes and zero padding. Metadata is supplied
  bytes: None omits a chunk, Some(empty) preserves a present empty chunk.
- Explicit RGBA background with BGRA serialization, 16-bit loop count (zero is
  infinite), 24-bit durations and even frame origins serialized divided by two.
  The prepared builder rejects odd origins; upstream mux rounds them down.
  The optimizer refines/expands rectangles before even snapping.
- Apply disposal to the preceding rectangle. Internal clearing is transparent
  black and is distinct from ANIM background metadata/viewer conventions.
- VP8X axes through 2^24 and area at most 2^32-1; pinned mux offsets below 2^24;
  raw codec axes restricted to 16383. Larger prepared canvases need no raw canvas
  allocation; oversized raw-input tiling is outside this plan.
- Check containment, nested/padded payload sizes and RIFF limit 2^32-2 before
  narrowing/allocation, including on 32-bit hosts. Test synthetic huge sizes.
- A lone full-canvas frame can become a still, retaining supplied metadata and
  losing playback duration/loop. Assembly is repeatable and preserves output
  on failure; adds validate before mutation.

## How to execute the remaining work

The work packages below are acceptance boundaries, not equal effort estimates.
Keyframes contain several coupled transitions; single-frame reconstruction has
an unresolved scope dependency. Checkbox count and total diff size are poor
progress measures. No current evidence establishes a need for a broad rewrite.

Before implementing each package:

1. Write the acceptance case list, expected observable state and comparison
   boundary in the reference record. Include a small named deterministic corpus
   for the new branch and explicit unsupported cases. State whether each file
   is compared with public assembly or the pre-reconstruction boundary.
2. Identify the production entry points, state ownership and existing helpers
   to reuse. Keep shared cache/timing behavior in one internal implementation;
   preserve supported low-level entry points through adapters as needed. Extract
   only what this package needs. SetFrame loop consolidation, new public codec
   APIs and unrelated still-encoder changes need their own justification.
3. Extend the existing C wrapper/evaluator to observe original functions and
   internal state. Settle the protocol before expanding large generated sweeps;
   keep older requests/fixtures valid. Do not mirror selection logic in C.
4. Finish portable cases, focused pinned comparisons and the independent
   playback checks for this boundary. Resolve mismatches before broad builds.
   Compare unchanged VP8L behavior when extracting shared state.
5. Run the required integration checks after the focused comparisons pass.
   Review new source/toolchain changes before fixture generation. Update this
   plan with completed boundaries and link evidence in the reference record.

Record implementation/review time, build time and test time separately for the
next packages. Report production, test/oracle and fixture growth separately.
Use focused targets while iterating; measure build/test costs before changing
shared test infrastructure or reducing coverage. Re-run checks after changes
that invalidate their results; avoid unchanged broad reruns within one package.

## Remaining delivery order

| Package | Depends on | Delivery boundary |
| --- | --- | --- |
| 4a. Shared cache and codec integration — complete | Existing candidate routes | VP8L/VP8/mixed timing and holds, keyframes disabled |
| 4b. Keyframe normalization and storage — complete | 4a | Effective options, bounded owned candidate slots and observer protocol |
| 4c. Deferred selection — complete | 4b | Candidate decisions and cache flushing without overflow interactions |
| 4d. Keyframes plus duration overflow | 4c | Complete keyframe timing/cache branch integration |
| 4e. Public optimized RGBA | 4a; 4d for keyframe controls | Checked public policy and settings for an advertised subset |
| Single-frame discussion gate | Existing reference evidence | A separately discussed scope decision; no decoder change is assumed |
| 5a. Sequence CLI | Existing milestone 2; 4e for optimizer switches | Useful full-frame frontend, then supported optimized controls |
| 5b. Resource baseline | A usable measured route | Paired timing/size/memory evidence for that route |

The CLI and an initial full-frame resource baseline can proceed independently
of keyframes or the decoding discussion. A keyframe-disabled public optimizer
can ship after 4a and 4e with an explicit subset contract. Complete keyframe
support requires 4d. Neither release implies full pinned animation completion.

### 4a. Shared timestamp cache for lossy and mixed candidates

Completed: generalized the keyframe-disabled cache while preserving its supported
VP8L behavior. See the [4a reference record](../libwebpop/tests/reference/ANIMATION.md#4a-shared-cache-and-codec-integration-2026-10-25)
for the protocol, named corpus, comparison boundaries and validation evidence.
Keyframe policy, public RGBA wiring and single-frame reconstruction remain outside
this package.

- [x] Define one internal cache owner for previous/carryover canvases, pending
  payload/controls, saved submitted/reversed configs, timestamps and counters.
  Candidate generators continue to own codec-specific trials and return their
  controls/masks. Preserve existing public LosslessAnimationCache methods,
  status, validation/state behavior and VP8L bytes through the integration.
- [x] Route VP8-only and mixed inputs through existing SetFrame helpers, with
  per-input codec/method/quality/strength/exact/alpha/SharpYUV changes. Save both
  mixed configs even on a skipped input; saved primary is independent of the
  winning codec. Restore identical original words before selection and apply
  previous disposal before mask copying. Skips retain the accepted original
  canvas/rectangle while counting the submitted input and merging its timing.
- [x] Port the fixed VP8/ALPH 1x1 overflow hold alongside existing VP8L bytes.
  Choose VP8L when the saved last config is lossless or mixed mode permits it;
  otherwise choose VP8. Duration accumulation precedes the next SetFrame, so
  the incoming codec must not replace the saved config used for this choice.
  Preserve the previous duration and assign the new interval to the hold;
  reset the preceding rectangle and flush before exceeding cache capacity.
- [x] Compare counters, pending controls/duration, both saved configs, both raw
  canvases and complete files after adds/final timing. Extend AnimationCache's
  existing protocol rather than creating another oracle stack. Keep explicit
  reference-only reconstruction cases outside the public-assembly parity set.

Settle these named acceptance families before changing the cache; turn each into
deterministic requests with expected state and byte/playback comparisons:

| Family | Required observation |
| --- | --- |
| Repeat versus similarity skip | Exact repeats, merely similar VP8 inputs and mixed skips with VP8L primary merge timing while retaining the accepted original/rectangle; submitted indexes still advance |
| Saved config on skip | Method/quality/primary/strength changes on skipped inputs update saved configs without forcing a frame; strength 100 does not repair prior near-lossless pixels |
| Candidate history | Both codec winners, preceding disposal, masks and unchanged-word restoration match original C across accepted/skip/accepted histories |
| Duration threshold | Zero/max intervals; a merged total of 2^24-1 updates the pending duration, while 2^24 retains the old duration and appends the fixed hold carrying the new interval |
| Hold before codec switch | Lossless-to-lossy and reverse switches on the add that splits use the preceding saved config for the hold; mixed mode permits VP8L regardless of the last winner |
| Capacity and finish | Repeated holds, a split before a change and a split at finish flush in order, reset the rectangle and preserve exact total duration |
| API and container | Wide origins, metadata, invalid codec/layout/timing retries, submitted-index errors and terminal finish behavior preserve their existing contracts |

Independent playback checks compare piecewise intervals and every canvas
without expanding a millisecond timeline. Add targeted method/alpha/SharpYUV
variants at the dispatch boundary; retain the existing exhaustive codec suites.

### 4b. Keyframe option normalization and candidate storage

Completed: effective policy, owned candidate storage and observer protocol are
implemented, with disabled-keyframe output unchanged. See the
[4b reference record](../libwebpop/tests/reference/ANIMATION.md#4b-keyframe-normalization-and-candidate-storage-2026-10-25).
Enabled policies now support ordinary-duration encoding through the separate
low-level constructor (4c). Duration overflow remains gated until 4d.

- [x] Define distinct disabled, all-keyframes and bounded interval controls and
  their checked mapping. Port SanitizeEncoderOptions ordering: minimize-size
  disables insertion; input kmax=1 normalizes to kmin=kmax=0; nonpositive kmax
  disables; remaining kmin adjustments preserve the half-window rule and the
  30-frame window cap. Test effective values without printing verbose warnings.
- [x] Define owned subframe/keyframe alternatives, chosen flags and durations;
  selected keyframe index, signed best penalty, frames-since-keyframe,
  previous-candidate-undecided, previous rectangle and flush boundary. Capacity
  follows the pin's normalized kmax-kmin+1, with at least two slots. Check
  arithmetic/allocation, retain the initial penalty sentinel and explicit
  no-selected-keyframe state, and preserve payload ownership during moves/releases.
- [x] Extend observations to show both alternatives/controls, selection and
  flush state before and after operations, plus effective options. The mixed
  observer's four-trial bound describes one SetFrame evaluation; a CacheFrame
  that evaluates both alternatives requires separate records or a larger bound.
  Record all calls/losers without changing original C selection.

Acceptance covers normalization branches and ordering, negative/extreme upstream
interval inputs in the mapping tests, all-keyframe and disabled effective values,
the 30-frame window versus allocated slot count, and unchanged disabled output.
Document how typed policies map to upstream values; finalize their public shape
in 4e. Storage extraction alone does not complete keyframe support.

### 4c. Deferred keyframe selection and ordinary flushing

Completed: CacheFrame/FlushFrames keyframe transitions with all 4a codec routes
at the ordinary-duration boundary. See the [4c reference record](../libwebpop/tests/reference/ANIMATION.md#4c-deferred-selection-and-ordinary-flushing-2026-10-26).
The separate low-level constructor rejects enabled-keyframe duration splits
before mutation. Full keyframe support remains incomplete until 4d; public
RGBA Optimized remains gated.

- [x] Port first-frame, kmin subframe-only, candidate-window and kmax reset paths,
  including all-keyframes. Settle skips before attempting the keyframe variant;
  skipped inputs do not advance the accepted-frame keyframe distance, although
  submitted indexes/timestamps and saved configs advance as in the pin.
- [x] Calculate KeyFramePenalty as signed complete still-file byte-size
  difference. Negative penalties are valid. Ordinary candidate ties retain the
  earlier trial; keyframe penalty comparisons use <= and favor the later
  candidate, demoting the previous choice. Keep these two tie rules distinct.
- [x] Preserve previous disposal updates for both alternatives when undecided,
  disallow background-disposal trials when the preceding rectangle is uncertain,
  and save prev_rect only when known. Update original/carryover canvases using
  the branch selected by the pin; defer only the encoded alternatives.
- [x] Flush only committed prefixes, emit the selected variant, rebase retained
  indexes and release losers. Finish must correctly drain a partially filled
  window and a selected candidate before kmax. Preserve output on errors.

Acceptance covers below/at/above kmin and kmax, zero/equal/negative penalties,
early and late winners, replacement of an earlier selection, skips inside a
window, repeated flush/rebase cycles, changed disposal/masks after an undecided
candidate, codec/config switches, all-keyframes and minimize-size normalization.
Compare both alternatives and transitions, complete files at the supported
assembly boundary, and independent/native composited canvases and timing.

### 4d. Deferred keyframes interacting with duration overflow

Boundary: integrate IncreasePreviousDuration with the full keyframe cache.
The two-slot disabled-keyframe tests alone cannot establish this behavior.

- [ ] Increase the pending subframe and keyframe durations together on ordinary
  merging. On overflow, insert the fixed hold selected from saved config, freeze
  an already chosen keyframe, rebase count_since_key_frame as in the pin, clear
  best penalty/selected index/undecided state, reset prev_rect to 1x1 and flush
  the committed prefix before caching another candidate.
- [ ] Compare states immediately before/after splitting, flushing, the next
  candidate and final assembly. Verify retained payload controls and that a
  previously chosen keyframe is not reconsidered after the split.

Acceptance crosses below/at the duration limit with no selected keyframe, an
undecided selected candidate, and earlier retained selections; split at cache
capacity, near interval boundaries, before a changed input and on final timing;
repeated skips/holds; both fixed hold codecs and mixed winner/config switches.
Verify exact total duration and all output canvases. Only then mark deferred
keyframe cache/timing support complete.

### 4e. Public optimized RGBA contract and integration

Boundary: connect already validated optimizer routes to AnimationEncoder;
retain FullFrame as the default and preserve its bytes/state behavior.

- [ ] Finalize checked optimizer options: minimize-size, allow-mixed and the
  supported keyframe policies, with disabled insertion, minimize_size=false and
  allow_mixed=false by default. Retain white background/infinite loop defaults.
  Document which routes are available if keyframe support is staged.
- [ ] Define per-frame mapping to the shared mixed config, including primary
  selection, f32 quality, near-lossless/exact and dormant reversed-codec fields.
  Reuse still option types where their meanings fit; do not silently round VP8
  quality or promise a common fidelity scale across codecs. Reject unsupported
  combinations before import, timing mutation or encoding, including on repeats.
- [ ] Reuse checked RGBA import with owned optimizer state. Preserve arbitrary
  timestamp origin, explicit end timing, submitted-index errors, validation
  retry, terminal failures and unchanged output on finish errors. Test full-frame
  and optimized policies independently through the public API.
- [ ] Add compiled examples and document settings, near-lossless/mixed/exact
  caveats, buffered output and the single-frame parity exception. Match supported
  public WebPAnimEncoder configurations at the advertised assembly boundary.

Acceptance includes padded/short buffers, unchanged inputs, invalid settings on
skipped frames, per-frame codec changes, defaults, supplied metadata, repeats,
zero/max durations, wide timelines, all state transitions and final timing.
A public optimized subset may retain the valid cropped single-ANMF result;
its documented reconstruction exception remains an open completion gate.

### Single-frame reconstruction: discussion gate

- [ ] Discuss the production decoding boundary before deciding an approach.
  Existing reference fixtures demonstrate 94-byte ANMF to 44-byte still and
  merged transparent inputs producing 90-byte ANMF versus a 34-byte C still.
  Preserve these explicit reference-only gaps and the Rust pre-optimization
  representation. Test-only decoding remains available for validation.
- [ ] Record the agreed scope, dependency/API implications, exact pixel
  requirements and follow-up acceptance plan after that discussion. Until then,
  do not add a production decoder dependency, port a decoder, treat retained
  input as equivalent reconstruction, or mark this branch complete.

Any later approved reconstruction plan must account for VP8L, near-lossless,
VP8/ALPH, canvas offsets/clearing, saved configs changed by skipped inputs,
mixed reversal, equal-size retention, metadata, final size choice and failure
behavior. These are discussion inputs, not approval to implement decoding.
Ordinary full-canvas mux cleanup remains complete and needs no decoder.

### 5a. Sequence CLI

The full-frame frontend can ship now; add optimizer controls after 4e.

- [ ] Add a separate cwebpop animation subcommand/binary with an explicit ordered
  duration/path interface, for example `cwebpop animate --frame 100 first.png
  --frame 40 second.png --loop 0 -o out.webp`. Settle the parser shape and its
  compatibility tests before changing existing single-image argument handling.
- [ ] Require equal canvases and explicit durations; accumulate timestamps with
  checked arithmetic and load one image at a time. Reject animated inputs until
  a deliberate multi-frame loader exists. Do not invent frame rate/glob order.
- [ ] Validate options/unsupported combinations before opening input/output.
  Preserve stdout output with diagnostics on stderr and avoid replacing an
  output file on failure. Keep all existing still invocations compatible.
- [ ] Test order/timing including zero duration, dimension mismatch, missing or
  malformed frames/options, overflow, animated-input rejection, stdout/stderr
  and failed-output preservation. Add supported codec/optimizer controls,
  update cwebpop/README.md and library examples. GIF/APNG import and metadata
  copying are separate work.

### 5b. Timing, size and memory baseline

- [ ] Define a fixed corpus: moving sprites, transparent fades, screenshots,
  near-static and photographic frames, long repeated inputs and duration holds.
  Record settings/dimensions/frame counts and build/toolchain provenance.
- [ ] Compare repeated release encode timings, encoded sizes, peak working
  memory and total buffered output with pinned scalar C. Match codecs, methods,
  quality/alpha, keyframe and thread policies. FullFrame uses the corresponding
  mux route; optimized subsets use the declared WebPAnimEncoder boundary.
- [ ] Verify correctness before timing; report unsupported reconstruction cases
  separately. Distinguish whole-process RSS from isolated encoder allocation
  evidence, working canvases/candidate windows from growing buffered output,
  and codec time from orchestration where measurements allow it. Performance
  is initially informational. Profile actual allocations/copies before changes.

## Validation and completion gates

Use committed C fixtures, independent structure/compositing checks and live
pinned comparisons through the existing harness. Candidate playback uses the
independent still decoder plus exact compositor and native WebPAnimDecoder;
image-webp's animated compositor has a documented one-unit rounding discrepancy
on some blended pixels. Do not hide that with tolerances. Full-frame replacement
checks may continue using its animation decoder. Inspect stored delays directly
as well as cumulative playback; keep viewer/background conventions separate.

The acceptance lists above extend the retained baseline matrix: 1x1, odd/skinny
and padded images; empty/one/two/many frames; transparency and hidden RGB;
alpha 0/1/254/255 and overlapping fades; odd-origin/edge/corner changes; both
blend/disposal modes and loop-boundary clearing; metadata/flags/padding/size
bounds; codec methods/settings;
unchanged input/output on errors and all sequence state transitions. New
integration cases must combine relevant branches rather than only repeating
isolated helper checks. Fixture counts alone are not acceptance criteria.

During implementation use explicit test targets so filtering does not build
all workspace test binaries. For example, when 4a changes cache integration:

```sh
cargo test -p libwebpop --test animation_cache --test animation_near_lossless
cargo test -p libwebpop --features native-oracle --test animation_cache --test animation_near_lossless -- --nocapture
```

Add the actual new lossy/mixed cache target and affected candidate suites to
those focused commands when introduced. Run their release comparisons where
float/config/selection behavior is involved. After focused acceptance passes,
run the integration gate once for the final implementation:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop --release --features native-oracle animation -- --nocapture
cargo test -p cwebpop
cargo test --workspace
cargo test --workspace --features native-oracle -- --nocapture
cargo fmt --all -- --check
```

Record exact commands/results in the reference record, including the comparison
boundary and any still-open branch. Documentation-only plan updates need link,
consistency and diff checks; they do not require rebuilding the encoder.

Basic full-frame library support is complete; its CLI remains 5a. Keyframe-disabled
optimized support requires 4a and 4e; advertised keyframe support additionally
requires 4b–4d. Full pinned animation completion still requires resolving the
single-frame discussion gate and implementing its agreed follow-up, plus all
advertised settings, CLI and resource evidence. Update the parent completion
checklist at those boundaries. Valid playback alone does not close parity gaps.
