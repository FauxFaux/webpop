## webp(sl)op

A "live port" (by agents) of libwebp to rust.

A pinned commit of libwebp exists in [upstream/](upstream/).

Initially focused on simple encoding, then complex encoding, then performance.


### License

... same as upstream (BSD-3), I think? I think that's how it works.


### Cost

As of lossless and near-lossless' completion:

 * Equivalent API price of $22.50 of GPT-6.1-Sol, mostly on "High".
 * Zero cost to me, covered by two "5h" sessions on the $20/mo Codex subscription.
 * ~7h of babysitting, with ~4h45m of waiting for model, so 2h15m of models waiting for me.
 * Typically one concurrent agent, maybe up to 3.
 * Costs include research, debugging, profiling, test tools, computing costs, etc.


### Accuracy

100% byte identical on all tested assets, for supported configurations.
Any non-identical output is a bug, and will be fixed.


### Performance

2-30% slower than my build of the C reference on my i9-13. ~5 percentage points of that are compiler flags.
There's no explicit SIMD here (yet). There's some code which is written to target C optimisations which doesn't
port well: we've kept the original C (for now).


### Examples

 * `cargo run --release -- --lossless t.png -o t.webp`
 * `cargo run --release -- --near_lossless 60 -q 90 -m 6 input.png -o output.webp`
 * `cargo run --release -- --lossy -q 98.5 --alpha_q 70 input.png -o output.webp`

The Rust library exposes `WebPEncodeRGBA(..., quality: f32)` for lossy color
with exact alpha and `WebPEncodeRGBAWithAlphaQuality(..., quality, alpha_quality)`
for independent alpha fidelity. Both use the pinned default preset and return
complete WebP files. Lossless and near-lossless APIs remain available; the CLI
defaults to lossless.


### Docs

 * [Original lossless encoder porting plan](docs/LOSSLESS_PORT_PLAN.md)
 * [Lossy encoder porting plan](docs/LOSSY_PORT_PLAN.md)
 * [CLI documentation](cwebpop/README.md)
 * [reference Docker build](reference-cwebp/README.md)
 * [benchmark harness](docs/BENCHMARKS.md)
