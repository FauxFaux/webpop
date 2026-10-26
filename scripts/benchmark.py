#!/usr/bin/env python3
"""Compare WebP CLI latency; Python stdlib only. See docs/BENCHMAKRS.md."""
import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def capture(command):
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    return {"command": command, "status": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def quantile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    return ordered[low] + (ordered[high] - ordered[low]) * (position - low)


def paired_interval(reference, candidate, seed):
    # Resample complete blocks, retaining the reference/candidate pairing.
    logs = [math.log(c / r) for c, r in zip(reference, candidate)]
    rng = random.Random(seed)
    boot = [math.exp(statistics.mean(rng.choices(logs, k=len(logs))))
            for _ in range(4000)]
    return {"paired_geomean_speedup": math.exp(statistics.mean(logs)),
            "bootstrap_95_percent": [quantile(boot, .025), quantile(boot, .975)]
            if len(logs) >= 4 else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", action="append", metavar="LABEL=PATH",
                        help="repeat for saved build variants; default current release binary")
    parser.add_argument("--c", type=Path, default=ROOT / "reference-cwebp/artifacts/cwebp")
    parser.add_argument("--compare-c-noasm", action="store_true",
                        help="also time the same C executable with -noasm")
    parser.add_argument("--case", action="append", metavar="QUALITY:METHOD",
                        help="repeat; QUALITY:METHOD (lossless), lossless:QUALITY:METHOD, "
                             "near-lossless:QUALITY:METHOD:STRENGTH or lossy:QUALITY:METHOD; "
                             "defaults 75:4, 75:0, 100:6 (integer quality only)")
    parser.add_argument("--exact", action="store_true")
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--seed", type=int, default=20261023)
    parser.add_argument("--cpu", type=int, help="pin runner and children to one logical CPU (Linux)")
    parser.add_argument("--timeout", type=float, default=600, help="seconds per encoding")
    parser.add_argument("--output", type=Path, required=True, help="new result directory, preferably under target/")
    parser.add_argument("--helper", type=Path,
                        default=ROOT / "target/benchmark-helper/release/examples/benchmark_images")
    parser.add_argument("--build-note", default="", help="actual build commands/flags; cannot recover them from a binary")
    parser.add_argument("images", nargs="*", type=Path, help="default samples/*.webp")
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 1 or args.timeout <= 0:
        parser.error("runs and warmups must be >= 1 and timeout must be positive")
    variants = {"c": args.c.resolve()}
    c_labels = {"c"}
    if args.compare_c_noasm:
        variants["c-noasm"] = args.c.resolve()
        c_labels.add("c-noasm")
    for spec in args.rust or ["rust=target/release/cwebpop"]:
        label, sep, path = spec.partition("=")
        if not sep or not label or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-" for c in label) or label in variants:
            parser.error(f"invalid or duplicate LABEL=PATH: {spec}")
        variants[label] = Path(path).resolve()
    cases = []
    for spec in args.case or ["75:4", "75:0", "100:6"]:
        try:
            fields = spec.split(":")
            if len(fields) == 2:
                fields.insert(0, "lossless")
            mode = fields[0]
            if mode not in {"lossless", "near-lossless", "lossy"}:
                raise ValueError()
            strength = 100
            if mode == "near-lossless":
                mode, q, m, strength = fields
                strength = int(strength)
            else:
                mode, q, m = fields
            q, m = int(q), int(m)
            if not (0 <= q <= 100 and 0 <= m <= 6 and 0 <= strength <= 100):
                raise ValueError()
            if args.exact and mode == "lossy":
                parser.error("--exact cannot be used with lossy cases")
        except ValueError:
            parser.error(f"invalid encoding case: {spec}")
        case = (mode, q, m, strength)
        if case not in cases:
            cases.append(case)
    images = [p.resolve() for p in args.images] or sorted((ROOT / "samples").glob("*.webp"))
    if not images:
        parser.error("no images")
    for path in [*variants.values(), *images]:
        if not path.is_file():
            parser.error(f"missing file: {path}")
    if args.cpu is not None:
        os.sched_setaffinity(0, {args.cpu})
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    for directory in ["bin", "inputs", "outputs", "provenance"]:
        (out / directory).mkdir()
    binary_records = {}
    for label, source in variants.items():
        destination = out / "bin" / label
        shutil.copy2(source, destination)
        binary_records[label] = {"source": str(source), "sha256": digest(destination),
                                 "linkage": capture(["ldd", str(destination)])}
    variants = {label: out / "bin" / label for label in variants}
    c_metadata = args.c.resolve().parent / "build"
    if c_metadata.is_dir():
        shutil.copytree(c_metadata, out / "provenance/c-build")
    shutil.copy2(ROOT / "Cargo.lock", out / "provenance/Cargo.lock")
    shutil.copy2(__file__, out / "provenance/benchmark.py")
    shutil.copy2(ROOT / "cwebpop/examples/benchmark_images.rs", out / "provenance/benchmark_images.rs")
    (out / "provenance/working-tree.patch").write_text(capture(["git", "diff", "HEAD", "--binary"])["stdout"])
    commands = [["git", "rev-parse", "HEAD"], ["git", "status", "--short"],
                ["git", "submodule", "status"], ["git", "-C", "upstream", "status", "--short"],
                ["rustc", "-vV"], ["cargo", "--version"], ["lscpu"], ["uname", "-a"]]
    provenance = {"started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  "argv": sys.argv, "cwd": os.getcwd(), "python": sys.version,
                  "platform": platform.platform(), "binaries": binary_records,
                  "build_note": args.build_note,
                  "environment": {k: v for k, v in os.environ.items()
                                  if k in {"RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTUP_TOOLCHAIN",
                                           "RUSTC", "RUSTC_WRAPPER", "CC", "CXX", "CFLAGS", "CXXFLAGS",
                                           "LDFLAGS", "LD_PRELOAD", "LD_LIBRARY_PATH"}
                                  or k.startswith(("CARGO_PROFILE_", "CARGO_BUILD_", "CARGO_TARGET_"))},
                  "affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
                  "load_average": os.getloadavg(), "commands": [capture(c) for c in commands]}
    for filename in ["/proc/meminfo", "/sys/fs/cgroup/cpu.max", "/sys/fs/cgroup/memory.max"]:
        if Path(filename).exists():
            provenance[filename] = Path(filename).read_text()
    policies = Path("/sys/devices/system/cpu/cpufreq")
    provenance["governors"] = {str(p): p.read_text().strip() for p in policies.glob("policy*/scaling_governor")}
    save(out / "provenance.json", provenance)
    if not args.helper.is_file():
        command = ["cargo", "build", "--locked", "--release", "-p", "cwebpop", "--example", "benchmark_images",
                   "--target-dir", str(ROOT / "target/benchmark-helper")]
        subprocess.run(command, cwd=ROOT, check=True)
    helper = args.helper.resolve()
    shutil.copy2(helper, out / "bin/benchmark_images")
    helper = out / "bin/benchmark_images"
    manifest = []
    for index, source in enumerate(images):
        prepared = out / "inputs" / f"{index}-{source.stem}.pam"
        result = subprocess.run([str(helper), "prepare", str(source), str(prepared)],
                                check=True, capture_output=True, text=True)
        manifest.append({"source": str(source), "source_sha256": digest(source),
                         "prepared": str(prepared), "sha256": digest(prepared),
                         "bytes": prepared.stat().st_size, "layout": "packed RGBA8 PAM",
                         **json.loads(result.stdout)})
    save(out / "inputs.json", manifest)
    rng = random.Random(args.seed)
    summary = []
    print("image / mode:q:m:strength / implementation: median seconds, IQR%, bytes; C/implementation paired speedup [95% bootstrap CI]", flush=True)
    with (out / "samples.jsonl").open("w", buffering=1) as raw:
        for image_index, item in enumerate(manifest):
            for mode, q, m, strength in cases:
                outputs, times, hashes, metrics = {}, {label: [] for label in variants}, {}, {}

                def run(label, block, warmup):
                    output = out / "outputs" / f"{image_index}-{mode}-n{strength}-q{q}-m{m}-{label}.webp"
                    prefix = "-" if label in c_labels else "--"
                    command = [str(variants[label]), prefix + "quiet", "-q", str(q), "-m", str(m)]
                    if mode == "lossless":
                        command.append(prefix + "lossless")
                    elif mode == "near-lossless":
                        command += [prefix + "near_lossless", str(strength)]
                    elif label not in c_labels:
                        command.append("--lossy")
                    if args.compare_c_noasm and label == "c-noasm":
                        command.append("-noasm")
                    if args.exact:
                        command.append(prefix + "exact")
                    command += [item["prepared"], "-o", str(output)]
                    started = time.perf_counter_ns()
                    subprocess.run(command, check=True, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.PIPE, timeout=args.timeout)
                    elapsed = (time.perf_counter_ns() - started) / 1e9
                    # All validation and hashes are outside the timed process boundary.
                    data_hash = digest(output)
                    if label not in hashes:
                        if mode == "lossless":
                            subprocess.run([str(helper), "verify", item["prepared"], str(output),
                                            "exact" if args.exact else "visible"], check=True)
                        else:
                            decoded = subprocess.run([str(helper), "metrics", item["prepared"], str(output)],
                                                     check=True, capture_output=True, text=True)
                            metrics[label] = json.loads(decoded.stdout)
                            if mode == "lossy" and metrics[label]["alpha_sse"] != 0:
                                raise RuntimeError(f"alpha differs at default alpha quality 100 for {label}")
                        hashes[label] = data_hash
                    elif hashes[label] != data_hash:
                        raise RuntimeError(f"nondeterministic output for {label}")
                    outputs[label] = {"bytes": output.stat().st_size, "sha256": data_hash}
                    raw.write(json.dumps({"image": image_index, "mode": mode, "near_lossless": strength,
                                          "quality": q, "method": m,
                                          "exact": args.exact, "label": label, "block": block,
                                          "warmup": warmup, "seconds": elapsed, "command": command,
                                          "binary_sha256": binary_records[label]["sha256"],
                                          **outputs[label]}) + "\n")
                    if not warmup:
                        times[label].append(elapsed)

                for warmup in range(args.warmups):
                    for label in variants:
                        run(label, warmup, True)
                # Random permutation then its reverse: balanced positions every two blocks.
                for block in range(args.runs):
                    if block % 2 == 0:
                        order = list(variants)
                        rng.shuffle(order)
                    else:
                        order.reverse()
                    for label in order:
                        run(label, block, False)
                rows = {}
                for label, values in times.items():
                    median = statistics.median(values)
                    rows[label] = {"median_seconds": median, "min_seconds": min(values), "max_seconds": max(values),
                                   "iqr_percent": 100 * (quantile(values, .75) - quantile(values, .25)) / median,
                                   "megapixels_per_second": item["width"] * item["height"] / median / 1e6,
                                   "matches_c_bytes": hashes[label] == hashes["c"],
                                   "decoded_metrics": metrics.get(label), **outputs[label]}
                    if label != "c":
                        rows[label].update(paired_interval(times["c"], values, args.seed))
                        if label not in c_labels:
                            baseline = next(name for name in variants if name not in c_labels)
                            rows[label]["versus_first_rust"] = paired_interval(times[baseline], values, args.seed)
                    row = rows[label]
                    line = f"{Path(item['source']).name} / {mode}:{q}:{m}:{strength} / {label}: {median:.4f}s, {row['iqr_percent']:.1f}%, {row['bytes']} B"
                    if label != "c":
                        ci = row["bootstrap_95_percent"]
                        interval = f"[{ci[0]:.3f}, {ci[1]:.3f}]" if ci else "[too few samples]"
                        line += f"; {row['paired_geomean_speedup']:.3f}x {interval} byte-match={row['matches_c_bytes']}"
                    print(line, flush=True)
                summary.append({"image": image_index, "mode": mode, "near_lossless": strength,
                                "quality": q, "method": m, "exact": args.exact, "results": rows})
                save(out / "summary.json", summary)
    print(f"Saved raw samples, validated outputs, binaries and provenance to {out}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        if error.stderr:
            print(error.stderr.decode() if isinstance(error.stderr, bytes) else error.stderr, file=sys.stderr)
        sys.exit(f"Benchmark command failed: {error.cmd}")
