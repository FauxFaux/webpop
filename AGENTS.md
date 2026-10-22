# Repository Guidelines

## Project Structure & Module Organization

This Cargo workspace contains `libwebpop`, a Rust 2024 port of pinned libwebp code. Work currently focuses on lossless encoder internals; the public Rust image encoder is not implemented yet.

- `libwebpop/src/`: `dsp/` contains pixel operations, `enc/` encoder algorithms, `utils/` shared helpers, and `webp/` format constants.
- `libwebpop/tests/`: portable regression tests, shared evaluators in `support/`, committed fixtures in `fixtures/`, native C wrappers in `reference/`, and the Wasm regression in `integration/`.
- `upstream/`: pinned libwebp reference submodule. Keep it unmodified; place oracle wrappers outside it.
- `docs/`: algorithm notes and lossless, near-lossless, and lossy port plans.

## Build, Test, and Development Commands

Run commands from the repository root:

- `cargo build -p libwebpop --lib`: build the Rust library without native oracle tooling.
- `cargo test`: run portable fixtures, independent checks, and the Wasmtime regression.
- `cargo test -p libwebpop --test integration`: run only the Wasm round-trip regression.
- `cargo test --features native-oracle -- --nocapture`: compare against pinned scalar C; requires Git, CMake >= 3.16, a C99 compiler, and a clean upstream checkout.
- `cargo fmt --all -- --check`: check standard Rust formatting.

## Coding Style & Naming Conventions

Use rustfmt defaults, including four-space indentation. Follow existing module filenames that mirror upstream C translation units. Preserve upstream function names such as `VP8LSubPixels` for traceability; use descriptive `snake_case` for Rust-specific helpers and tests. The library forbids unsafe Rust. Keep C interoperability confined to test infrastructure and handle allocation failures consistently with existing `Result` APIs.

## Testing Guidelines

Use Rust's built-in `#[test]` framework. Add deterministic edge cases and independent correctness checks alongside C differential comparisons. Extend shared evaluators and native wrappers when porting operations. Review source/toolchain changes before regenerating fixtures; follow `libwebpop/tests/reference/README.md`. No numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

History uses short messages prefixed with `feat:` or `chore:`, for example `feat: lossless bit writer and huffman`. Keep commits focused. In pull requests, describe the affected port milestone, upstream functions, behavior changes, and validation commands/results. Explain fixture or pin changes and update the relevant port plan when progress changes.
