# libwebpop: bottom-up lossless encoder port

## Target and compatibility

Port the code reachable from `WebPEncodeLosslessRGBA` in the checked-out
[`upstream/`](../upstream), preserving function names and source layout where
practical. The result is a Rust library that takes prepared RGBA pixels and
returns an owned buffer containing a complete still-image WebP file.

Reference revision: `1844042d1ca751e62e61df7617dba19c98588339`, obtained from
`git -C upstream rev-parse HEAD`. This is the source of truth for this plan.
The older revision mentioned in `AGENTS.md` and `ALGORITHM.md` is not the current
checkout. Keep the upstream submodule pinned throughout the initial port.

Suggested Rust API, retaining the upstream entry-point name:

```rust
pub fn WebPEncodeLosslessRGBA(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<Vec<u8>, WebPEncodingError>;
```

Validate dimensions in `1..=16383`, `stride >= width * 4`, and input length at
least `(height - 1) * stride + width * 4`, using checked arithmetic. The slice
starts at the first row and uses a positive byte stride. Supporting C's negative
stride requires a separate representation with an explicit starting offset;
leave that and C ABI compatibility for later. Zero-copy input is not required:
upstream imports RGBA into owned ARGB working storage, which transforms mutate.

The initial compatibility target is the upstream simple API's defaults:

| Setting | Value | Consequence |
| --- | --- | --- |
| `lossless`, `pic.use_argb` | `1` | VP8L encoder; no YUV conversion |
| `quality` | `70` | Compression effort, not lossy image quality |
| `method` | `4` | Full analysis, not the method-0 shortcut |
| `near_lossless` | `100` | No near-lossless quantization |
| `exact` | `0` | RGB beneath alpha zero may change |
| `thread_level` | `0` | Execute the stream worker synchronously |
| `image_hint` | `WEBP_HINT_DEFAULT` | Normal lossless heuristics |
| Stats, progress hook, extra info | Unset | No public callback/statistics framework required |

These values come from [`picture_enc.c`](../upstream/src/enc/picture_enc.c)
and [`config_enc.c`](../upstream/src/enc/config_enc.c). They still reach
image-dependent palettes, all four transform types, color caches, several LZ77
strategies, traceback refinement, and histogram clustering. A minimal valid
VP8L writer alone would not complete this port.

Aim for byte-for-byte agreement with a scalar C build of the pinned revision
under a recorded toolchain/configuration. Independently require valid output,
correct dimensions, exact alpha, and exact RGB wherever input alpha is nonzero.
Record any platform-specific numerical divergence instead of quietly accepting
it. Decoding successfully is necessary but cannot establish algorithm parity.

## Current repository

- Milestones 1 and 2 are complete: `libwebpop` has endian/bit helpers, checked
  positive-stride RGBA geometry, pixel lane arithmetic, prefix coding,
  hash/color-cache primitives, all 14 scalar predictors and residual kernels,
  subtract-green/cross-color kernels and histogram collectors, palette index
  packing, fixed-point log/entropy/streak/vector DSP, the lossless bit writer,
  and Huffman construction, compression, and serialization. The public Rust
  encoder is not implemented; tokens/reference storage and histogram primitives
  (milestone 3) are next.
- The remaining milestone-1 DSP passed 317,625 exact native C comparisons with
  seed `6d2b79f5`, including all log lookup/integer-approximation inputs,
  floating-point fallbacks, exhaustive byte-pair averaging/half predictors,
  selector ties, and seeded arrays. Compact C outputs are committed as portable
  fixtures. No numerical divergence was observed on the recorded toolchain.
- Milestone 2 passed 11,852 exact native C comparisons with the same seed and
  toolchain, covering writer flush/growth boundaries, clone/reset/swap trials,
  empty/single-symbol trees, comparator ties, histogram smoothing, depth-limit
  retries, RLE boundaries, simple/full headers, both trimmed-length forms, and
  code/extra-bit writes. There are 100 additional portable C fixtures, plus
  independent bit-sequence and Huffman header/symbol round trips. Allocation
  failures propagate through Rust `Result`; scratch buffers and tree arrays are
  owned. Histogram-driven Huffman glue and image token writing remain milestone 5.
- [The native oracle guide](../libwebpop/tests/reference/README.md) describes
  the selected scalar C backend, pinned build, committed fixtures, seeded
  comparisons, provenance, and mismatch replay. It is opt-in via
  `cargo test --features native-oracle`; normal builds never compile C.
- [`tests/integration/main.rs`](../libwebpop/tests/integration/main.rs) already
  invokes a bundled C encoder in Wasmtime and decodes its output using `image`.
  Keep this working while developing the Rust encoder.
- That Wasm binary is documented as a scalar, lossless-only v1.6.0 build from
  another repository. It is useful existing evidence, but is not established as
  an exact oracle for the current upstream commit. The current checkout includes
  additional palette and transform-selection code. Both reference backends
  now use the same synthetic image cases and independent decoding checks.
- [`ALGORITHM.md`](ALGORITHM.md) concerns lossy encoding and is background, not
  the implementation plan for this target.

## Function dependency map

This is a source-derived dependency tree with repeated subgraphs referenced by
letters. It represents dependencies, not a literal execution trace: conditional
alternatives appear together, and shared descendants are expanded once. Small
accessors, allocation/cleanup helpers, and some internal search helpers are
grouped below. Thus it is a practical porting map, not a claim of an exhaustive
compiler-generated call graph.

File paths below are relative to `upstream/src/`. `[conditional]` means reachable
for some valid RGBA images with the default settings, not an optional feature
that can be omitted from the finished port.

```text
WebPEncodeLosslessRGBA                                      enc/picture_enc.c
└── Encode
    ├── WebPConfigPreset → WebPConfigInitInternal             enc/config_enc.c
    │   └── WebPValidateConfig
    ├── WebPPictureInit → WebPPictureInitInternal             enc/picture_enc.c
    ├── WebPMemoryWriterInit / WebPMemoryWrite                enc/picture_enc.c
    ├── WebPPictureImportRGBA                                enc/picture_csp_enc.c
    │   └── Import [ARGB branch]
    │       ├── WebPValidatePicture / CheckStride
    │       ├── WebPPictureAlloc [ARGB allocation]             enc/picture_enc.c
    │       └── RGBA → 0xAARRGGBB packing                     dsp/lossless.c
    │           └── VP8LConvertBGRAToRGBA_C [little-endian aligned path]
    └── WebPEncode [lossless branch]                          enc/webp_enc.c
        ├── WebPValidateConfig / WebPValidatePicture
        ├── WebPReplaceTransparentPixels                     enc/picture_tools_enc.c
        │   └── WebPAlphaReplace → AlphaReplace_C             dsp/alpha_processing.c
        └── VP8LEncodeImage                                  enc/vp8l_enc.c
            ├── VP8LBitWriterInit                            [B]
            ├── WriteImageSize / WriteRealAlphaAndVersion
            │   └── VP8LPutBits                              [B]
            ├── WebPPictureHasTransparency                   enc/picture_csp_enc.c
            │   └── CheckNonOpaque → WebPHasAlpha32b
            │       └── HasAlpha32b_C                        dsp/alpha_processing.c
            ├── VP8LEncodeStream
            │   ├── VP8LEncoderNew → VP8LEncDspInit           [A]
            │   ├── EncoderAnalyze                          [H]
            │   ├── EncoderInit
            │   │   └── VP8LHashChainInit / VP8LBackwardRefsInit [D]
            │   └── worker Execute → EncodeStreamHook [synchronous]
            │       ├── AllocateTransformBuffer / MakeInputImageCopy
            │       ├── PaletteSort                          [G, conditional]
            │       ├── EncodePalette → EncodeImageNoHuffman [F, conditional]
            │       ├── MapImageFromPalette                  [G, conditional]
            │       ├── ApplySubtractGreen                   [conditional]
            │       │   └── VP8LSubtractGreenFromBlueAndRed_C [A]
            │       ├── ApplyPredictFilter                   [I, conditional]
            │       ├── ApplyCrossColorFilter                [J, conditional]
            │       └── EncodeImageInternal                  [K]
            └── WriteImage
                ├── VP8LBitWriterFinish / VP8LBitWriterNumBytes [B]
                ├── WriteRiffHeader → PutLE32
                └── writer → WebPMemoryWrite [header, data, padding]
```

The importer uses a converter named for BGRA because swapping red and blue also
converts RGBA bytes into little-endian ARGB words. Preserve the logical integer
layout across architectures; do not reinterpret an arbitrary byte slice as
aligned `u32` storage.

```text
[A] Scalar arithmetic and DSP                                dsp/lossless_common.h
├── VP8LAddPixels / VP8LSubPixels [independent byte lanes]
├── VP8LSubSampleSize / VP8LNearLosslessBits
├── VP8LPrefixEncode / VP8LPrefixEncodeBits [and NoLUT variants]
│   └── BitsLog2Floor / lookup tables                        utils/utils.h
├── VP8LFastLog2 / VP8LFastSLog2 / DivRound / RightShiftRound
│   ├── kLog2Table / kSLog2Table                              dsp/lossless_enc.c
│   └── FastLog2Slow_C / FastSLog2Slow_C
├── ShannonEntropy_C / CombinedShannonEntropy_C               dsp/lossless_enc.c
├── VP8LBitEntropyInit / VP8LBitsEntropyUnrefined
├── GetEntropyUnrefined_C / GetCombinedEntropyUnrefined_C
├── ExtraCost_C / AddVector_C / AddVectorEq_C / VectorMismatch_C
├── VP8LSubtractGreenFromBlueAndRed_C / VP8LTransformColor_C
│   └── ColorTransformDelta / U32ToS8
├── VP8LCollectColorRedTransforms_C / VP8LCollectColorBlueTransforms_C
├── VP8LBundleColorMap_C
└── PredictorSub0_C … PredictorSub13_C                        dsp/lossless_enc.c
    ├── VP8LPredictor0_C … VP8LPredictor13_C                   dsp/lossless.c
    │   └── Average2/3/4, Select, ClampedAddSubtractFull/Half
    └── VP8LSubPixels

[B] Lossless bit writer                                      utils/bit_writer_utils.{c,h}
├── VP8LBitWriterInit → VP8LBitWriterResize
├── VP8LPutBits → VP8LPutBitsFlushBits / VP8LPutBitsInternal
├── VP8LBitWriterFinish / VP8LBitWriterNumBytes
└── VP8LBitWriterClone / Reset / Swap / WipeOut

[C] Huffman construction and storage
├── VP8LCreateHuffmanTree                                    utils/huffman_encode_utils.c
│   ├── OptimizeHuffmanForRle
│   ├── GenerateOptimalTree → CompareHuffmanTrees / SetBitDepths
│   └── ConvertBitDepthsToSymbols → ReverseBits
├── VP8LCreateCompressedHuffmanTree                          utils/huffman_encode_utils.c
│   └── CodeRepeatedValues / CodeRepeatedZeros
├── GetHuffBitLengthsAndCodes                                enc/vp8l_enc.c
│   └── VP8LCreateHuffmanTree [five alphabets per histogram]
├── StoreHuffmanCode                                         enc/vp8l_enc.c
│   └── StoreFullHuffmanCode [unless simple/empty tree]
│       ├── VP8LCreateCompressedHuffmanTree / VP8LCreateHuffmanTree
│       ├── StoreHuffmanTreeOfHuffmanTreeToBitMask
│       └── StoreHuffmanTreeToBitMask
├── ClearHuffmanTreeIfOnlyOneSymbol
└── StoreImageToBitMask
    ├── WriteHuffmanCode / WriteHuffmanCodeWithExtraBits
    ├── PixOrCopy accessors / reference cursors               [D]
    └── VP8LPrefixEncode [lengths and plane-coded distances] / VP8LPutBits

[D] Tokens, caches, matching, and backward references
├── PixOrCopyCreateLiteral / CreateCacheIdx / CreateCopy      enc/backward_references_enc.h
├── VP8LBackwardRefsInit / VP8LBackwardRefsCursorAdd            enc/backward_references_enc.c
│   └── BackwardRefsNewBlock / cursor iteration / clear / clone / swap
├── VP8LColorCacheInit / Clear / Copy                         utils/color_cache_utils.c
│   └── VP8LHashPix / cache lookup, insert, contains, set      utils/color_cache_utils.h
├── VP8LDistanceToPlaneCode / BackwardReferences2DLocality     enc/backward_references_enc.c
├── VP8LHashChainInit / VP8LHashChainFill
│   └── GetMaxItersForQuality / GetWindowSizeForHashChain
│       GetPixPairHash64 / FindMatchLength → VP8LVectorMismatch [A]
└── VP8LGetBackwardReferences → GetBackwardReferences
    ├── BackwardReferencesRle / BackwardReferencesLz77
    ├── BackwardReferencesLz77Box [small-palette trial]
    ├── CalculateBestCacheSize / BackwardRefsWithLocalCache
    │   └── VP8LHistogramEstimateBits / VP8LHistogramCreate   [E]
    ├── VP8LBackwardReferencesTraceBackwards                  enc/backward_references_cost_enc.c
    │   ├── BackwardReferencesHashChainDistanceOnly
    │   │   ├── CostModelBuild → ConvertPopulationCountTableToBitEstimates
    │   │   │   └── VP8LHistogramStoreRefs / VP8LFastLog2      [E, A]
    │   │   ├── GetLiteralCost / GetCacheCost / GetLengthCost / GetDistanceCost
    │   │   └── CostManager* / cost interval maintenance
    │   ├── TraceBackwards
    │   └── BackwardReferencesHashChainFollowChosenPath
    └── BackwardReferences2DLocality

[E] Histograms and clustering                               enc/histogram_enc.c
├── VP8LHistogramInit / Create / StoreRefs
│   └── HistogramAddSinglePixOrCopy → prefix codes / token accessors
├── VP8LAllocateHistogram / VP8LAllocateHistogramSet [owned Rust storage]
├── VP8LBitsEntropy / VP8LHistogramEstimateBits
│   └── BitsEntropyRefine / PopulationCost / InitialHuffmanCost / FinalHuffmanCost
│       └── scalar log, entropy, streak, and extra-bit costs  [A]
└── VP8LGetHistoImageSymbols
    ├── HistogramBuild / HistogramCopyAndAnalyze / ComputeHistogramCost
    ├── HistogramAnalyzeEntropyBin / HistogramCombineEntropyBin
    ├── HistogramCombineStochastic → MyRand / histogram cost helpers
    ├── HistogramCombineGreedy → HistoQueue* / histogram cost helpers
    └── HistogramRemap

[F] Auxiliary-image encoder                                 enc/vp8l_enc.c
└── EncodeImageNoHuffman
    ├── VP8LHashChainFill / VP8LGetBackwardReferences          [D]
    ├── VP8LHistogramStoreRefs [one histogram]                [E]
    ├── GetHuffBitLengthsAndCodes / StoreHuffmanCode           [C]
    └── StoreImageToBitMask                                  [C]

[G] Palette path
├── GetColorPalette / PrepareMapToPalette / SearchColorNoIdx  utils/palette.c
│   └── VP8LHashPix / PaletteCompareColorsForQsort
├── PaletteSort                                              utils/palette.c
│   ├── PaletteSortMinimizeDeltas → PaletteColorDistance
│   ├── PaletteSortModifiedZeng
│   │   └── CoOccurrenceBuild / CoOccurrenceFindMax
│   └── PaletteMinLARefineColors → PaletteMinLARefine
│       └── MinLACutProfile / MinLABestSlot / co-occurrence helpers
├── EncodePalette                                            enc/vp8l_enc.c
│   └── VP8LSubPixels [delta palette] → EncodeImageNoHuffman   [F]
└── MapImageFromPalette → ApplyPalette                       enc/vp8l_enc.c
    ├── SearchColorGreedy / ApplyPaletteHash0/1/2 / mapping helpers
    └── VP8LBundleColorMap_C [packed indices]                 [A]

[H] Configuration analysis                                  enc/vp8l_enc.c
└── EncoderAnalyze
    ├── GetColorPalette                                      [G]
    ├── GetHistoBits / GetTransformBits → ClampBits
    ├── AnalyzeEntropy
    │   ├── AddSingle / AddSingleSubGreen / HashPix
    │   └── VP8LBitsEntropy / VP8LFastLog2 / VP8LSubPixels    [E, A]
    └── AddPaletteSortingConfigs [conditional palette trials]

[I] Predictor transform                                     enc/predictor_enc.c
└── ApplyPredictFilter                                       enc/vp8l_enc.c
    ├── VP8LResidualImage
    │   ├── GetPlaneDiffMask
    │   ├── GetBestPredictorsAndSubSampling
    │   │   ├── ComputeResidualsForTile → GetResidual
    │   │   ├── GetBestPredictorForTile → PredictionCostSpatialHistogram
    │   │   │   └── PredictionCostBias / entropy DSP          [A]
    │   │   └── VP8LOptimizeSampling
    │   └── CopyImageWithPrediction → GetResidual
    │       └── PredictBatch / predictor DSP / VP8LSubPixels  [A]
    └── EncodeImageNoHuffman [predictor image]                [F]

[J] Cross-color transform                                   enc/predictor_enc.c
└── ApplyCrossColorFilter                                    enc/vp8l_enc.c
    ├── VP8LColorSpaceTransform
    │   ├── GetBestColorTransformForTile
    │   │   ├── GetBestGreenToRed → GetPredictionCostCrossColorRed
    │   │   └── GetBestGreenRedToBlue → GetPredictionCostCrossColorBlue
    │   │       └── color histogram collectors / PredictionCostCrossColor [A]
    │   ├── CopyTileWithColorTransform → VP8LTransformColor_C [A]
    │   └── VP8LOptimizeSampling
    └── EncodeImageNoHuffman [multiplier image]               [F]

[K] Main image encoder                                      enc/vp8l_enc.c
└── EncodeImageInternal
    ├── VP8LHashChainFill / VP8LGetBackwardReferences          [D]
    ├── VP8LGetHistoImageSymbols                              [E]
    ├── GetHuffBitLengthsAndCodes                            [C]
    ├── VP8LOptimizeSampling                                 enc/predictor_enc.c
    ├── EncodeImageNoHuffman [histogram-symbol image, conditional] [F]
    ├── StoreHuffmanCode / StoreImageToBitMask                [C]
    └── bit-writer Clone / Reset / Swap [retain best trial]   [B]
```

`EncodeImageNoHuffman` **does use Huffman coding**. Its narrower role is encoding
an auxiliary image without a spatial Huffman-selection image or color cache.
Palette tables, predictor maps, cross-color maps, and histogram-symbol maps all
reuse it. This is the useful first integration point above the leaf modules.

The graph has shared dependencies: histogram *primitives* feed LZ77 cost
selection, while histogram *clustering* consumes completed backward references.
Implement those portions separately rather than treating whole source files as
indivisible work units.

## Rust layout and translation rules

Mirror the relevant upstream basenames beneath `libwebpop/src/`:

```text
src/
  lib.rs
  webp/{encode,format_constants,types}.rs
  enc/{config_enc,picture_enc,picture_csp_enc,picture_tools_enc,webp_enc}.rs
  enc/{vp8l_enc,vp8li_enc,backward_references_enc,backward_references_cost_enc}.rs
  enc/{histogram_enc,predictor_enc}.rs
  dsp/{lossless,lossless_common,lossless_enc,alpha_processing}.rs
  utils/{utils,color_cache_utils,bit_writer_utils,huffman_encode_utils,palette}.rs
```

Add files as their functions are ported. Preserve private helper names as well as
public ones; use a scoped `non_snake_case` allowance for upstream names. Preserve
constants, tables, tie-breaking rules, and algorithm order. Attach upstream file
and function references to ported code. Carry the applicable upstream copyright,
BSD license, and patent notices with derived code.

Use slices, owned vectors, explicit indices, and `Result` rather than raw-pointer
ownership. Header declarations belong with their implementation module unless a
separate header-derived module, such as `vp8li_enc`, improves correspondence.
Keep algorithmic state recognizable (`VP8LHistogram`, `PixOrCopy`, `VP8LEncoder`,
`HuffmanTreeCode`, etc.). There is no need to emulate C struct padding, allocator
block layout, alignment tricks, mutable global dispatch, or frees.

For the first implementation, bind DSP names directly to scalar functions and
use immutable predictor tables. `VP8LDspInit`, `VP8LEncDspInit`, and
`WebPInitAlphaProcessing` can become unnecessary or trivial compatibility
helpers. Execute `EncodeStreamHook` directly; a full worker abstraction is not a
prerequisite. Keep memory writer behavior recognizable using an owned output
vector, without requiring public writer callbacks.

Use wrapping operations only where the C algorithm relies on modular arithmetic
(pixel lanes, hashes, some unsigned cost arithmetic); use checked arithmetic for
dimensions, allocation lengths, offsets, and file sizes. Preserve signed-byte
conversions and signed shifts in cross-color math. The pinned code uses fixed
point entropy/cost calculations, with floating-point logarithms in large-value
fallbacks: do not replace it with a generic floating-point cost model.

## Reference testing: choose the backend after a small spike

Define the comparison data first: inputs and logical outputs for a named
function, including mutated buffers, lengths, selected modes, and errors. Avoid
serializing pointers, padding, allocation capacity, or linked-list addresses.
Use fixed random seeds and record upstream revision, build flags, toolchain,
and fixture provenance. Shared test cases should not depend on C execution
being native or Wasm.

| Backend | Advantages | Costs |
| --- | --- | --- |
| Local scalar C build | Direct access to pinned source; straightforward wrappers for internal functions; convenient debugging | Requires a C toolchain and repeatable build configuration on each host |
| Wasm build from pinned source | Consistent guest architecture and arithmetic environment; portable test binary; existing Wasmtime integration | Requires rebuilding/exporting internal wrappers, guest-memory marshaling, and artifact provenance; cannot inspect leaves through the existing single public export |

Initial preference: local scalar C for leaf comparisons, while keeping the
existing Wasm public-API regression. This is provisional. In milestone 0, try
the same small leaf wrapper and public encode case with the available backends,
then choose one primary pinned-source oracle. Do not build two complete leaf
harnesses as a prerequisite for porting. If Wasm wins, produce an instrumented
build from this pinned checkout, rather than assuming the bundled binary matches.

For static C helpers, compile test-only wrappers in the same translation unit
as the original source, exporting only the operations needed by tests. Keep
these wrappers outside `upstream/` and leave production Rust independent of C.
Whichever backend is selected, a normal Rust build must not compile or execute
the reference encoder. Keep committed fixtures for fast portable tests; use the
live oracle for broader differential testing.

Compare in layers:

1. **Leaves:** exact integers, arrays, bit lengths, codes, and emitted bits;
   exhaust small input domains and use seeded cases for larger ones.
2. **Subgraphs:** exact token sequences, cache choices, histograms, predictor
   maps, palette orderings, transformed pixels, and complete auxiliary streams.
3. **Public API:** full encoded bytes against the pinned oracle, plus independent
   decoding with the existing Rust decoder. The older Wasm build can check
   semantic compatibility without requiring identical compressed bytes.

For `exact=0`, compare alpha everywhere and RGB where alpha is nonzero. Do not
assert zero RGB at every transparent decoded pixel: `WebPEncode` initially
replaces transparent input colors with zero, but `GetResidual` may subsequently
choose predictor-compatible colors for them. Exact byte agreement with the
matching oracle still covers that behavior.

## Implementation order and acceptance gates

Each milestone ends with independently testable functions. Integrate parents
only after their dependencies pass. Preserve original files even when a
milestone implements just one part of a file.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 | Rename crate; add `lib.rs`; record pin; oracle backend spike; reusable fixtures and comparison harness | — | Existing Wasm regression passes; one wrapped leaf and one public encoding compared using the chosen pinned-source backend |
| 1 | Constants, endian helpers, checked geometry, pixel arithmetic, prefix codes, hash/cache primitives, scalar predictor/color kernels and log/entropy DSP `[A]` | 0 | Differential edge cases, small-domain exhaustive checks, fixed-seed arrays; lookup and slow paths covered |
| 2 (complete) | Lossless bit writer `[B]`; Huffman tree generation, compression, and serialization `[C]` except histogram-driven glue | 1 | Exact output bits across flush boundaries, clone/reset trials, degenerate and depth-limited trees; 11,852 native comparisons and portable round trips pass |
| 3 | `PixOrCopy`, reference storage/cursors; histogram initialization, accumulation and cost estimation `[D/E foundations]` | 1 | Hand-built literal/copy/cache streams yield matching histograms and costs; cursor/storage boundaries exercised |
| 4 | Hash chains; RLE, standard and box LZ77; cache-size selection; traceback cost model/interval machinery `[D]` | 3 | Matching tokens, match lengths/distances, cache choices, and chosen paths; replay reconstructs the source pixels |
| 5 | `GetHuffBitLengthsAndCodes`, `StoreImageToBitMask`, `EncodeImageNoHuffman` `[F]`; RIFF/image header helpers | 2, 4 | Exact auxiliary streams; a test-only untransformed/no-cache image wrapped in RIFF decodes correctly |
| 6 | Histogram clustering/remap `[E remainder]`; `VP8LOptimizeSampling`; `EncodeImageInternal` `[K]` | 5 | Matching cluster maps, Huffman groups, and main-image streams, including multi-group images and trial resets |
| 7 | Palette sorting/mapping `[G]`; predictor search/residuals `[I]`; cross-color search `[J]`; transform writers | 5, 6's sampling helper | Matching palette orders and packed indices, residuals, selected transforms and auxiliary images on forced branch cases |
| 8 | `AnalyzeEntropy`, sizing heuristics, `EncoderAnalyze` `[H]`; encoder storage and `EncodeStreamHook`; `VP8LEncodeStream` | 6, 7 | Matching analysis decisions, tried configurations, and selected stream; full palette trial selection included |
| 9 | Picture/config/import/writer glue; `WebPEncode`, `VP8LEncodeImage`, and public `WebPEncodeLosslessRGBA` | 8, header helpers | All public corpus cases match the pinned reference and independently decode; invalid inputs fail without panic |

Milestone 5 is an internal integration checkpoint, not a temporary implementation
of the public default API. It provides early valid images while the remaining
upstream heuristics are still being ported. Picture/import/header leaf functions
can be completed earlier whenever useful; their final integration is milestone 9.

Within a large work package, repeat the same method. For example, in traceback
start with population-to-cost conversion and literal/length/distance costs,
then interval updates, then path construction, then the top-level refinement.
For Huffman coding, preserve comparator ties and canonical bit reversal before
testing complete code storage. For palettes, implement distance/co-occurrence
helpers before their ordering algorithms.

### First implementation batch

After the setup/oracle decision, start with these small, reusable leaves:

1. `PutLE32`, `BitsLog2Floor`, `BitsCtz`, `VP8LSubSampleSize`, and geometry checks.
2. `VP8LAddPixels` / `VP8LSubPixels`, including channel carry/borrow isolation.
3. `VP8LPrefixEncodeBitsNoLUT` / `VP8LPrefixEncodeNoLUT`, then table-backed
   variants; test values at every prefix boundary.
4. `VP8LHashPix` and color cache operations; exercise collisions and wrapping.
5. `Average2/3/4`, `Select`, clamped predictors, then predictor modes 0–13.
6. Subtract-green and cross-color kernels, including negative signed-byte
   multipliers and byte wraparound.
7. Log tables, fast-log slow paths, entropy/streak primitives; validate the
   pinned revision's exact rounding behavior before higher-level cost decisions.

## Corpus and completion criteria

Use synthetic inputs that force behavior, supplemented with a small committed
real-image corpus. Include:

- `1×1`, `1×N`, `N×1`, odd dimensions, and dimensions around tile boundaries;
  test the dimension limit with skinny images rather than huge allocations.
- Solid colors, ramps, checkerboards, long runs, repeated rows, and distant
  repeats; copies crossing rows and overlapping copies.
- Palette sizes `1, 2, 3, 4, 5, 16, 17, 224, 225, 256, 257` to cross packing,
  small-palette, spatial-sort, and palette-eligibility thresholds.
- Opaque, partially transparent, fully transparent, and mixed-alpha images,
  with nonzero hidden RGB in the original input.
- Correlated color channels, noisy images, and images with distinct regions
  that exercise cross-color transforms and multiple Huffman groups.
- Padded row strides, input slices beginning at an unaligned byte offset,
  exact minimum input length, short buffers, zero/out-of-range dimensions,
  short strides, and arithmetic-overflow attempts.

Instrument the reference harness to show which configurations and paths these
cases actually reach. Input labels alone do not prove coverage. Use direct
subgraph cases to force branches that public-API heuristic selection rarely
chooses. On a mismatch, report the first differing logical intermediate and
retain a minimized reproducer.

The target is complete when all default-path functions above are implemented,
the public Rust API passes pinned-reference byte comparisons for the agreed
corpus and seeded differential cases, output independently decodes with the
required pixel equality, and checked input handling passes in debug and release.
Run `cargo fmt --check`, the relevant unit/integration tests, and a normal
library build proving there is no production C/Wasm dependency. Record encoded
sizes and basic encode-time measurements for representative inputs so future
optimization has a baseline; performance parity is a subsequent target.

Deferred: lossy VP8, YUV/SharpYUV, separate lossy `ALPH` encoding, near-lossless
quantization, other qualities/methods/presets, worker threads, SIMD, public
advanced configuration/callback APIs, decoding, animation, metadata muxing,
image file loaders, and C ABI compatibility. Shared scalar helpers from files
named for decoding remain in scope where the encoder uses them.
