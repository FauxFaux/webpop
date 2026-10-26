#!/usr/bin/env bash
# Compare q80/method-4 CLI cycles with SIMD-enabled C and non-LTO v3 Rust.
# Usage: bash scripts/profile-lossy.sh INPUT.pam NEW_OUTPUT_DIR [CPU [REPEATS]]
# Requires the release binary/helper from docs/BENCHMAKRS.md.
set -euo pipefail
cd "$(dirname "$0")/.."
input="$(realpath "${1:?prepared PAM required}")"
out="$(realpath -m "${2:?new output directory required}")"
cpu="${3:-2}"
repeats="${4:-24}"
event="${PERF_EVENT:-cpu_core/cycles/u}"
baseline="${BASELINE_RUST:-target/benchmark-v3/release/cwebpop}"
helper="${BENCHMARK_HELPER:-target/benchmark-v3/release/examples/benchmark_images}"
[[ -f "$input" && -x "$baseline" && -x "$helper" && "$repeats" =~ ^[1-9][0-9]*$ ]]
mkdir "$out"
exec 3>"$out/commands.log"
export BASH_XTRACEFD=3
set -x
env -u CARGO_ENCODED_RUSTFLAGS \
    RUSTFLAGS='-C target-cpu=x86-64-v3 -C link-arg=-Wl,--no-rosegment' \
    CARGO_PROFILE_PROFILING_LTO=off \
    CARGO_PROFILE_PROFILING_CODEGEN_UNITS=16 \
    CARGO_PROFILE_PROFILING_OPT_LEVEL=3 \
    CARGO_PROFILE_PROFILING_DEBUG=2 \
    CARGO_PROFILE_PROFILING_DEBUG_ASSERTIONS=false \
    CARGO_PROFILE_PROFILING_OVERFLOW_CHECKS=false \
    CARGO_PROFILE_PROFILING_INCREMENTAL=false \
    CARGO_PROFILE_PROFILING_PANIC=unwind \
    cargo build --locked --offline --profile profiling -p cwebpop --bin cwebpop \
    --target-dir target/benchmark-v3 -vv >"$out/rust-build.log" 2>&1
cp target/benchmark-v3/profiling/cwebpop "$out/rust"
cp "$baseline" "$out/baseline"
cp reference-cwebp/artifacts/cwebp "$out/c"
cp "$input" "$out/input.pam"
cp -r reference-cwebp/artifacts/build "$out/c-build"
cp scripts/profile-lossy.sh scripts/compare-profiles.py "$out/"
cp Cargo.toml Cargo.lock .cargo/config.toml "$out/"
rustc -vV >"$out/rustc.txt"
perf version >"$out/perf-version.txt"
uname -a >"$out/host.txt"
lscpu >>"$out/host.txt"
git rev-parse HEAD >"$out/revision.txt"
git diff HEAD >"$out/working-tree.patch"
git status --short >"$out/git-status.txt"
sha256sum "$out/rust" "$out/baseline" "$out/c" "$out/input.pam" >"$out/SHA256SUMS"

# Unprofiled paired validation precedes all perf captures.
python3 scripts/benchmark.py \
    --c "$out/c" --rust "baseline=$out/baseline" --rust "profile=$out/rust" \
    --helper "$helper" --case lossy:80:4 --runs 8 --warmups 1 \
    --cpu "$cpu" --seed 20261026 --output "$out/unprofiled" \
    --build-note "Same release baseline as matrix; profiling build commands in $out/commands.log" \
    "$out/input.pam" >"$out/unprofiled.log" 2>&1
printf '%s\n' "$cpu" >"$out/cpu.txt"
printf '%s\n' "$repeats" >"$out/repeats.txt"
printf '%s\n' "$event" >"$out/event.txt"
for label in c rust; do
    command=("$out/$label")
    if [[ "$label" == c ]]; then
        command+=(-quiet)
    else
        command+=(--quiet --lossy)
    fi
    command+=(-q 80 -m 4 "$out/input.pam" -o "$out/$label.webp")
    taskset -c "$cpu" "${command[@]}"
    perf record -o "$out/$label.perf.data" -e "$event" -F99 \
        --call-graph dwarf,64000 -- taskset -c "$cpu" \
        bash -c 'n=$1; shift; for ((i=0; i<n; i++)); do "$@" || exit; done' \
        _ "$repeats" "${command[@]}" 2>"$out/$label.record.log"
    perf report -i "$out/$label.perf.data" --stdio --no-children \
        --call-graph none --sort symbol --percent-limit 0.1 \
        >"$out/$label.self.txt"
    perf report -i "$out/$label.perf.data" --stdio --children \
        --call-graph none --sort symbol --percent-limit 0.1 \
        >"$out/$label.children.txt"
    if command -v flamegraph >/dev/null; then
        flamegraph --perfdata "$out/$label.perf.data" --deterministic \
            -o "$out/$label.svg" >"$out/$label.flamegraph.log" 2>&1
    fi
done
for label in c rust; do
    command=("$out/$label")
    if [[ "$label" == c ]]; then
        command+=(-quiet)
    else
        command+=(--quiet --lossy)
    fi
    command+=(-q 80 -m 4 "$out/input.pam" -o "$out/$label.stat.webp")
    perf stat -j -o "$out/$label.stat.json" -r 5 \
        -e cpu_core/cycles/u,cpu_core/instructions/u,cpu_core/branches/u,cpu_core/branch-misses/u \
        -- taskset -c "$cpu" "${command[@]}"
    cmp "$out/$label.webp" "$out/$label.stat.webp"
done
cmp "$out/c.webp" "$out/rust.webp"
sha256sum "$out/c.webp" "$out/rust.webp" >>"$out/SHA256SUMS"
python3 scripts/compare-profiles.py "$out" --mode lossy
