#!/usr/bin/env bash
# Run from any directory. Build all variants before collecting any timings.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/benchmark-bins

build_variant() {
    local label="$1" cpu="$2" lto="$3" units="$4"
    local flags="-C target-cpu=$cpu"
    echo "Building $label: RUSTFLAGS='$flags' LTO=$lto codegen-units=$units"
    local tuning=()
    if [[ "$lto" == thin ]]; then
        tuning=(CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1)
    fi
    env -u CARGO_ENCODED_RUSTFLAGS \
        -u CARGO_PROFILE_RELEASE_LTO -u CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
        RUSTFLAGS="$flags" "${tuning[@]}" \
        cargo build --locked --release -p cwebpop --target-dir target/benchmark-build -vv \
        > "target/benchmark-bins/$label.build.log" 2>&1
    cp target/benchmark-build/release/cwebpop "target/benchmark-bins/rust-$label"
}

rustc -vV > target/benchmark-bins/rustc.txt
cargo --version > target/benchmark-bins/cargo.txt
build_variant v3 x86-64-v3 default default
build_variant v3-thin x86-64-v3 thin 1
build_variant native native default default
# Preparation and verification are untimed; compile this helper before measuring.
env -u CARGO_ENCODED_RUSTFLAGS RUSTFLAGS='-C target-cpu=x86-64-v3' \
    cargo build --locked --release -p cwebpop \
    --example benchmark_images --target-dir target/benchmark-helper
