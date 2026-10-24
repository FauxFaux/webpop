# Scalar SharpYUV reference

Source: `upstream/sharpyuv/` at
`1844042d1ca751e62e61df7617dba19c98588339`, SharpYUV API version 0.4.2.
The Rust implementation is isolated in `libwebpop/src/sharpyuv/` and uses
only its own modules and the standard library. Encoder selection is implemented
at the caller; SIMD remains separate work. See the
[port plan](../../../docs/SHARPYUV_PORT_PLAN.md).

## Backend and provenance

The existing `webpop_oracle` executable links the normally built pinned
`sharpyuv` target through `webp`. Public conversion/version/options requests
call that library. Static transfer and image helpers use included original
translation units with renamed exports; no algorithms are copied into wrappers.
DSP requests call the original scalar dispatch functions. The image wrapper
intercepts filtering and update calls only to observe the original loop.

The recorded 2026-10-24 run used Ubuntu x86_64, GCC
`15.2.0-16ubuntu1`, system libm, CMake Release `-O3 -DNDEBUG`, Rust
`1.101.0-nightly (7189d39d9 2026-10-22)`, seed `6d2b79f5`, SIMD and threading
off. Actual flags, compiler path/version and commands are retained in
`target/native-oracle/provenance.txt`. Rust production builds require no C,
CMake, Wasm or upstream checkout.

The oracle build explicitly puts CMake's generated include root first for
all upstream targets and wrappers. An ignored in-source Autotools
`upstream/src/webp/config.h` can otherwise override the generated scalar
configuration, notably in upstream's SharpYUV target. `oracle.c` now rejects
SIMD/threading configuration macros at compile time. No upstream file was
modified. The existing native encoder suites pass with this include ordering.

## Logical protocol

Use the existing hexadecimal-u32 request/response transport. All samples are
logical values; signed values use two's complement, f32 arguments use exact
bit patterns, and u64 values use low/high words. No pointer, struct layout or
host byte order enters the fixtures. Float helper NaNs are canonicalized to
`7fc00000` to compare classification rather than libm payloads.

| Request | Input | Output |
| --- | --- | --- |
| `SharpYuvTables` | Empty | 1,026 gamma-to-linear entries, then 514 linear-to-gamma entries |
| `SharpYuvMath` | R/G/B, clip maximum, clip value, precision shift, four coefficients | Clipped sample, weighted gray, converted component |
| `SharpYuvMatrix` | Predefined index 0–4; or `(5, Kr bits, Kb bits, depth, range)` | Twelve signed Y/U/V coefficients/offsets |
| `SharpYuvGamma` | Direction 0=forward/1=inverse, depth, transfer enum, first value, count | Contiguous converted values |
| `SharpYuvFloat` | Helper index, x bits, y bits | Result bits |
| `SharpYuvDsp` | Operation, depth, length, precision shift, twelve matrix words, samples | Updated samples with guards; Y update also returns u64 error |
| `SharpYuvPublicInfo` | Requested API version | Actual version, initialization success, default transfer on success |
| `SharpYuvConvert` | Image request below | Output guard success, packed Y/U/V samples |
| `SharpYuvImport` | Image request below | First two imported rows, gamma-domain gray, target W, target RGB offsets |
| `SharpYuvTrace` | Image request below; simple flag zero | Guard success, initial/iteration snapshots, packed final Y/U/V |
| `SharpYuvTimedConvert` | Image request below | Guard success, C conversion CPU microseconds, packed Y/U/V |

Float indices 0–21 alternate forward/inverse for 709, 470M, 470Bg,
Smpte240, Log100, Log100Sqrt10, Iec61966, Bt1361, PQ, Smpte428 and HLG;
22/23/24 select `Roundf`, `Powf(x,y)` and `Log10f`.

DSP operations 0/1/2/3/4/5/7 select UpdateY, UpdateRGB, FilterRow,
ConvertRowY, ConvertRowUV, UpdateWSrgb and UpdateChromaSrgb. Payloads are,
respectively: three length-sized arrays; three length-sized arrays; A/B
arrays of `length+1` plus `2*length` luma; luma plus three chroma segments;
three chroma segments; three RGB segments; and two RGB rows of `6*length`
samples each. Output guards are logical u16/i16 values, except the next-byte
guard after 8-bit row conversion. Byte guards use a uniform byte pattern so
the protocol does not assume little-endian storage.

Image requests contain `(width, height, sample_step, sample_stride,
rgb_depth, yuv_depth, transfer, simple_flag)`, twelve explicit matrix words,
then three source channel arrays. Each array has minimum reachable length
`(height-1)*stride + (width-1)*step + 1`; unrelated padding is retained.
The C adapter translates steps/strides to bytes, uses deliberately unaligned
8-bit inputs, and checks output row padding and leading/trailing guards.
High-depth inputs use aligned u16 storage. The simple flag selects the
normal default-sRGB API; otherwise it selects the options API.

Trace snapshots contain `(iteration, error low, error high)`, all best Y,
all target Y, all best RGB offsets and all target RGB offsets. Initial
iteration is `ffffffff` with error zero. Working dimensions are rounded even;
each offset plane has three segments at half width/height. Snapshots occur
before the first iteration and after every full iteration. The observation
adapter relies on the original allocation layout to locate the immutable
target planes; review that adapter whenever changing the source pin.

## Acceptance and fixture regeneration

The debug and release suites compare exactly:

- 3,317 leaf requests, including both full gamma tables, more than 6.6 million
  gamma/inverse values, random fixed-point/matrix cases and float boundaries.
  Smpte428 inverse checks extend above 65,536 to its reachable linear maximum.
- 976 scalar DSP requests covering signed/full-width values, clipping,
  zero lengths where allowed, row lookahead and guard preservation.
- 2,304 import/iteration requests covering every transfer and RGB depth,
  initial/intermediate planes and all three observed iteration exits.
- 6,772 public requests covering all twelve depth pairs, every transfer,
  predefined/computed/custom matrices, odd/skinny/20,000-pixel geometries,
  padding, clamping, version checks, default/options identity and the committed
  128×128 `test_ref.ppm` sample at 8 and 12 bits.

There are 153 committed C fixtures in `fixtures/sharpyuv.txt`, checked in
portable tests and again against live C. Independent tests check gray sums,
gamma interpolation, signed bilinear filters, explicit edge replication,
layout equivalence, alpha independence, clamping/truncation, version/depth/
geometry validation, checked allocation failure and repeatable concurrent calls.
No C initialization state or mutable dispatch table is used in Rust.

Review the pin, wrappers and recorded toolchain before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test sharpyuv sharpyuv_fixtures_match_pinned_reference -- --nocapture
cargo test -p libwebpop --test sharpyuv
cargo test -p libwebpop --features native-oracle --test sharpyuv -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test sharpyuv -- --nocapture
```

The inverse sweep avoids float-to-u16 overflow at the 16-bit endpoint;
actual image conversion's internal depths are 10/12/14. Custom conversion
matrices are checked before C-style i32 scaling could overflow. Computed
color spaces require finite nonnegative Kr/Kb with sum below one, and a
supported depth. Unsupported enum values cannot inhabit Rust's typed enums.

## First scalar baseline

Run explicitly; timing is not a test gate:

```sh
cargo test -p libwebpop --release --features native-oracle --test sharpyuv sharpyuv_scalar_baseline -- --ignored --nocapture
```

On the recorded host, using the 128×128 real sample, discard one warm-up
and average 20 conversions. C measures conversion CPU time with `clock()`;
Rust measures wall time around the public call, excluding test input setup.
C uses caller-owned output planes; Rust allocates and returns owned planes.

| Input/output, transfer | C | Rust | C peak plane/scratch bytes | Rust peak plane/scratch bytes |
| --- | --- | --- | --- | --- |
| 8-bit, sRGB | 403.6 µs | 415.3 µs | 141,696 | 166,272 |
| 12-bit, sRGB | 496.4 µs | 499.2 µs | 166,272 | 166,272 |

The byte counts are calculated backing-buffer peaks, excluding inputs,
gamma tables, stack and allocator overhead; they are not measured RSS.
Both implementations use `7*w*h + 19*w` bytes of iterative plane/scratch
storage. Rust's final row conversion uses logical u16 output planes, then
packs 8-bit output after releasing the iterative scratch. Consequently its
8-bit peak includes the temporary u16 planes. Timing is one small baseline,
not a general performance claim.

## Encoder integration

`sharpyuv_enc.c` adds two requests to the same scalar executable. Both call
the normally linked libwebp public API, with `picture.use_argb=1` before
`WebPPictureImportRGBA` so conversion happens through the encoder's ARGB
route. No conversion, cleanup or encoding formulas are copied into the adapter.

- `YuvaSharpImport`: `(width, height, byte_stride, cleanup)`, then RGBA bytes.
  Calls `WebPPictureSharpARGBToYUVA`, optionally cleans transparent areas and
  returns transparency/alpha-plane flags followed by packed Y/U/V/A samples.
- `WebPEncodeLossyOptions`: `(width, height, byte_stride, quality bits,
  alpha_quality, use_sharp_yuv, preprocessing)`, then RGBA bytes. Initializes
  the default preset, sets the three explicit settings and calls `WebPEncode`
  with a memory writer. Returns complete bytes. Preprocessing is 0 or 4;
  either the sharp flag or bit 4 selects upstream's sharp route. Inputs start
  at an unaligned byte offset, require no final-row padding, and are checked
  unchanged afterward.

`tests/sharpyuv_encoder.rs` compares 432 YUVA imports before/after cleanup
and 2,318 complete files, including independent decode/alpha/container checks.
The corpus covers geometry below/at/above 4, odd edges, alpha zero/endpoints/
intermediate values, fractional quality, independent alpha quality, the two
sharp selectors and their combination, real images and both 16,383-pixel
skinny limits. Sixty portable C fixtures live in
`fixtures/sharpyuv_encoder.txt`. Independent checks establish that small
images retain the ordinary importer, larger-image RGB conversion ignores
alpha, alpha is copied exactly, defaults match the existing convenience API,
and output capacity/content guarantees hold for both conversion choices.

The public Rust surface adds `LossyOptions` and `encode_lossy_rgba`; it keeps
the two existing simple lossy functions on ordinary conversion. The CLI
requires `--lossy` for `--sharp_yuv` and `--pre`, accepts preprocessing 0/4
only, and rejects other values before reading input or touching output.

Regenerate only after reviewing the pin and recorded provenance:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test sharpyuv_encoder sharp_yuv_encoder_fixtures_match_pinned_c -- --nocapture
cargo test -p libwebpop --test sharpyuv_encoder
cargo test -p libwebpop --features native-oracle --test sharpyuv_encoder -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test sharpyuv_encoder -- --nocapture
cargo test -p cwebpop
```
