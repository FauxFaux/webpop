# Quality, effort, and “levels”

WebP has several independent controls that are sometimes called “quality” or
“level”. The meaning depends on the encoder mode. In particular, **`-q` controls
image fidelity in lossy mode and compression effort in lossless mode**.
`-near_lossless` controls pixel changes within the lossless-format encoder;
`-alpha_q` controls a separate alpha encoder used with lossy images.

This document describes the pinned libwebp revision
`1844042d1ca751e62e61df7617dba19c98588339` in [`upstream/`](../upstream), then
maps it to the current Rust API and CLI. The [lossless port plan](LOSSLESS_PORT_PLAN.md)
describes the pipeline and implementation milestones.

## Upstream names and ranges

The API column refers to fields of [`WebPConfig`](../upstream/src/webp/encode.h),
passed to `WebPEncode()`. Defaults below are from `WebPConfigInit()` and
[`config_enc.c`](../upstream/src/enc/config_enc.c), also used by `cwebp`.

| Purpose | C API | Upstream `cwebp` | Range / default | Meaning of increasing the value |
| --- | --- | --- | --- | --- |
| Select image encoder | `lossless` | `-lossless` | `0` or `1`; default `0` | Switch from lossy VP8 to VP8L |
| Lossy image quality | `quality` (`float`), with `lossless=0` | `-q Q` | `0..100`; default `75` | Higher fidelity, generally larger files |
| Lossless compression effort | The same `quality`, with `lossless=1` | `-lossless -q Q` | `0..100`; default `75` | More compression search, generally slower and smaller files |
| Compression method / effort | `method` (`int`) | `-m M` | `0..6`; default `4` | More expensive encoding methods in either mode |
| Near-lossless fidelity | `near_lossless` (`int`), with `lossless=1` | `-near_lossless N` | `0..100`; default `100` | Fewer permitted pixel changes; `100` disables quantization |
| Alpha fidelity for lossy images | `alpha_quality` (`int`) | `-alpha_q A` | `0..100`; default `100` | More alpha levels retained; `100` preserves alpha exactly |
| Bundled lossless efficiency preset | `WebPConfigLosslessPreset(&config, level)` | `-z LEVEL` | `0..9`; opt-in | Select a slower combination of `method` and `quality` |

These scales are encoder settings, not percentages of retained information,
bits per channel, or directly comparable measures of distortion. File size and
runtime depend on image content; the directions above are intended trade-offs,
not strict monotonic guarantees for each image.

## `quality` / `-q`: two meanings

In **lossy VP8**, `quality` controls the rate/distortion trade-off for the color
image. `-q 100` still uses a lossy format and is not a promise of exact RGB.
Alpha quality is controlled separately. The simple C API exposes this value as
the `quality_factor` argument of `WebPEncodeRGB`, `WebPEncodeRGBA`, and their
BGR variants.

In **VP8L**, `quality` controls compression search rather than pixel fidelity.
With `near_lossless=100`, both `-lossless -q 0` and `-lossless -q 100` preserve
visible RGB and alpha. Fully transparent pixels' hidden RGB may change unless
`exact=1` / `-exact` is supplied. This exception also applies to the simple
lossless API, as explained in [`doc/api.md`](../upstream/doc/api.md).

For near-lossless VP8L, `quality` retains its compression-effort meaning;
`near_lossless` independently controls pixel quantization. For example,
`-near_lossless 60 -q 90 -m 6` selects near-lossless fidelity 60 and lossless
compression effort 90 with method 6. It does not select lossy quality 90.

The implementation uses `quality` for such work as cross-color search, hash
chains, backward-reference refinement, and histogram clustering. VP8L's main
stream converts the floating-point configuration value to an integer, so
fractional `-q` values do not provide fractional resolution to those kernels.
See [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c),
[`predictor_enc.c`](../upstream/src/enc/predictor_enc.c), and
[`backward_references_enc.c`](../upstream/src/enc/backward_references_enc.c).

## `method` / `-m`: effort in either mode

`method` is a second effort control, independent of `quality`. It selects
algorithmic shortcuts and search strategies; there is no general conversion
from method 4 to some equivalent quality value.

In VP8L, method 0 selects the `low_effort` path. Other method-dependent decisions
include histogram and transform tile sizes and which encoding configurations
to try. Method 6 together with quality 100 enables the broadest configuration
search (the “cruncher”). Raising just one control does not necessarily enable
the same work as raising both. These decisions are in `EncoderAnalyze`,
`GetHistoBits`, and `GetTransformBits` in
[`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c).

In lossy encoding, `method` likewise trades CPU time for compression efficiency.
It also supplies the effort level for compression of the separate alpha plane.

## `near_lossless` / `-near_lossless`: VP8L pixel quantization

Near-lossless encoding uses the VP8L format, with controlled changes to pixels
before or during prediction. The decoder does not need a separate near-lossless
mode. Lower settings permit stronger changes; 100 takes the ordinary lossless
path. In `cwebp`, **even `-near_lossless 100` enables `lossless=1`**. In the C
API, callers must set `lossless=1` themselves; changing only `near_lossless`
does not select VP8L.

[`VP8LNearLosslessBits`](../upstream/src/dsp/lossless_common.h) groups settings
into six strengths:

| `near_lossless` | Maximum bits shaved (`5 - N / 20`, integer division) | Maximum predictor quantization step |
| --- | --- | --- |
| `100` | `0` | `1` (no quantization) |
| `80..99` | `1` | `2` |
| `60..79` | `2` | `4` |
| `40..59` | `3` | `8` |
| `20..39` | `4` | `16` |
| `0..19` | `5` | `32` |

This is an adaptive quantizer control, not a uniform bit-depth reduction or a
per-channel error guarantee. Palette candidates bypass near-lossless changes.
Non-palette candidates use either standalone preprocessing or predictor residual
quantization; the low-effort predictor path bypasses the latter. Consequently,
selecting a value below 100 does not guarantee that pixels change.

**Near-lossless can change alpha as well as RGB.** `alpha_quality=100` does not
prevent this: the separate alpha-plane encoder is not involved. `exact` governs
hidden RGB handling and also affects predictor quantization; it is not an
independent “preserve alpha” switch. See the
[near-lossless port plan](NEAR_LOSSLESS_PORT_PLAN.md),
[`near_lossless_enc.c`](../upstream/src/enc/near_lossless_enc.c), and
[`predictor_enc.c`](../upstream/src/enc/predictor_enc.c) for routing details.

## `alpha_quality` / `-alpha_q`: alpha with lossy VP8

A lossy color image can carry transparency in a separate `ALPH` chunk. Its
quality is independent of color quality: `-q 30 -alpha_q 100` permits color
loss while preserving alpha exactly. In an ordinary VP8L image, alpha is encoded
alongside RGB, so `alpha_quality`, `alpha_compression`, and `alpha_filtering`
do not control it.

In [`alpha_enc.c`](../upstream/src/enc/alpha_enc.c), alpha quality below 100
requests quantization to this number of alpha levels, using integer arithmetic:

```text
A <= 70:  2 + A / 5
A >  70: 16 + (A - 70) * 8
A = 100: bypass quantization entirely
```

For example, 0 requests 2 levels, 70 requests 16, and 99 requests 248. These are
quantizer level counts, not effort settings or guarantees of that many distinct
values in the result.

Two related names are easy to confuse with `method`:

| C API | `cwebp` | Meaning |
| --- | --- | --- |
| `alpha_compression` | `-alpha_method 0` or `1` | Store uncompressed alpha or compress it with VP8L; default `1` |
| `alpha_filtering` | `-alpha_filter none`, `fast`, or `best` | Predictive filter selection (`0`, `1`, `2`); default `fast` |

`-alpha_method` selects an algorithm, not the `0..6` effort scale. Even with
`alpha_compression=0`, `alpha_quality<100` can quantize alpha before storage.
When alpha is compressed, the internal VP8L encoder gets `method` from the outer
configuration and a derived compression quality: `8 * method`, except that
unquantized alpha with method 6 uses quality 100. Neither the outer color `-q`
nor `-alpha_q` is passed straight through as this internal VP8L effort value.

## `-z`: a bundled lossless preset level

`WebPConfigLosslessPreset()` sets exactly three fields: `lossless=1`, `method`,
and `quality`. Initialize the config first. It leaves `near_lossless`, `exact`,
and other fields unchanged. The mapping in
[`config_enc.c`](../upstream/src/enc/config_enc.c) is:

| `level` / `-z` | `method` / `-m` | `quality` / `-q` |
| --- | --- | --- |
| 0 | 0 | 0 |
| 1 | 1 | 20 |
| 2 | 2 | 25 |
| 3 | 3 | 30 |
| 4 | 3 | 50 |
| 5 | 4 | 50 |
| 6 | 4 | 75 |
| 7 | 4 | 90 |
| 8 | 5 | 90 |
| 9 | 6 | 100 |

The API recommends level 6 as a balanced choice. `cwebp -lossless` already uses
the same method/quality pair as `-z 6`; `-lossless` itself only sets the mode.

In the pinned [`cwebp.c`](../upstream/examples/cwebp.c), **any explicit `-q` or
`-m` disables `-z`, regardless of argument order**. The preset is applied after
parsing only if neither was supplied. Thus `-z 9 -q 80` does not select method 6,
and `-z 9 -m 6` does not select quality 100. Because `-z` then does not activate
lossless mode either, supply `-lossless` explicitly when using `-q` / `-m` for
lossless encoding.

`-preset photo` (or `default`, `picture`, `drawing`, `icon`, `text`) is a different
feature: `WebPConfigPreset()` initializes a whole configuration for a content
type, retaining the quality passed to it. It is not a numbered effort level.
Put `-preset` first in `cwebp`, since it resets other settings, including mode.

## Other related controls

Lossy rate control has `target_size` / `-size`, `target_PSNR` / `-psnr`,
`qmin` and `qmax` / `-qrange`, and `pass` / `-pass`. These select a target,
constrain the quality search, or set its analysis pass count (`1..10`, default
`1`); they are not additional VP8L quality levels. `cwebp` raises a pass count
of 1 to 6 when a size or PSNR target is supplied. Size/PSNR targeting is
unsupported for lossless encoding. See
[`frame_enc.c`](../upstream/src/enc/frame_enc.c) and
[`cwebp.c`](../upstream/examples/cwebp.c).

Other numeric settings such as `sns_strength` / `-sns`, `filter_strength` / `-f`,
and `partition_limit` / `-partition_limit` tune specific lossy tools. Their
`0..100` ranges do not make them interchangeable with quality. Likewise,
`thread_level` / `-mt` enables threading rather than selecting a compression level.

## Current Rust API and `cwebpop`

The public Rust configuration is
[`LosslessOptions`](../libwebpop/src/enc/config_enc.rs), used by
`encode_lossless_rgba`:

| Rust field | Meaning | Default |
| --- | --- | --- |
| `quality: u32` | VP8L compression effort, `0..=100` | `70` |
| `method: u32` | VP8L method effort, `0..=6` | `4` |
| `exact: bool` | Preserve hidden RGB as well as visible channels | `false` |

`WebPEncodeLosslessRGBA` exposes no quality argument and uses these defaults.
The upstream simple lossless functions also hard-code quality 70 in
[`picture_enc.c`](../upstream/src/enc/picture_enc.c). This differs from quality
75 in `WebPConfigInit()` and `cwebp -lossless`.

[`cwebpop`](../cwebpop/README.md) defaults to VP8L with lossless pixels.
Its `-q` (also `--quality`) and `-m` (also `--method`) map to those Rust fields, but its
default quality is **75**, matching upstream's CLI. Fractional `-q` values are
truncated to integer effort. Multi-letter upstream options use two dashes here,
for example `--lossless` and `--exact`.

`--near_lossless N` accepts all integer strengths `0..100`; omission means 100.
It preserves the CLI's quality 75/method 4 defaults and honors explicit `-q`,
`-m`, `--exact` and `--noalpha`. For example,
`cwebpop --near_lossless 60 -q 90 -m 6 input.png -o output.webp` selects fidelity
60 with effort 90/method 6. Both `--lossless` and `--near_lossless` select VP8L,
so combining them retains the requested strength. Strength 100 produces the
same bytes as ordinary lossless encoding at the same effort/exact settings.

The public `encode_near_lossless_rgba(..., near_lossless, &options, &mut output)`
entry point shares `LosslessOptions` for compression effort and cleanup, with strength
supplied independently. `encode_lossless_rgba` always selects strength 100.
`WebPEncodeRGBAWithNearLossless(..., near_lossless, &mut output)` is the custom
wrapper API, with fixed quality 70/method 4/exact false. Match it in the CLI
with `--near_lossless N -q 70 -m 4` and no `--exact`.

Below 100, RGB and intermediate alpha may change; `--exact` disables cleanup
and predictor quantization, while standalone preprocessing may still change
pixels. It does not promise exact alpha.

`WebPEncodeRGBA(..., quality: f32, &mut output)` exposes default-preset lossy
color with exact alpha. `WebPEncodeRGBAWithAlphaQuality(..., quality: f32, alpha_quality: i32, &mut output)`
also controls alpha fidelity. Color quality must be finite and in `0..=100`;
fractional values reach VP8 unchanged. Alpha quality must be in `0..=100`.
The standard function equals the custom wrapper at alpha quality 100;
alpha quality does not affect opaque output. Quality 100 still changes RGB.
These controls are separate from `LosslessOptions`.

`cwebpop --lossy -q 98.5 --alpha_q 70 input.png -o output.webp` selects VP8
with color quality 98.5 and alpha quality 70. Its defaults are color quality
75, alpha quality 100 and method 4. Lossy `-m 0..6` selects color and alpha
encoding effort, also exposed as `LossyOptions::method` in `encode_lossy_rgba`.
`--lossy` conflicts with `--lossless`, `--near_lossless` and `--exact`.
`--alpha_q` requires `--lossy`. `-z` and other unimplemented settings remain
rejected even where parsing recognizes their names.
