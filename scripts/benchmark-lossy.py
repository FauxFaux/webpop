#!/usr/bin/env python3
"""Paired encode-only scalar Rust/C lossy baseline; see docs/LOSSY_METHODS_PLAN.md."""
import argparse
import csv
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(argv):
    return subprocess.run([str(a) for a in argv], cwd=ROOT, check=True,
                          capture_output=True, text=True).stdout


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--edge", type=int, default=1024)
    parser.add_argument("--cpu", type=int)
    args = parser.parse_args()
    if not (1 <= args.runs <= 100 and 128 <= args.edge <= 16383):
        parser.error("runs must be 1..100 and edge 128..16383")
    # Build before collecting samples; the normal test oracle remains separate.
    command(["cargo", "build", "--locked", "--release", "-p", "cwebpop",
             "--example", "lossy_benchmark", "--example", "benchmark_images"])
    command(["cmake", "-S", "libwebpop/tests/reference", "-B", "target/native-oracle",
             "-DCMAKE_BUILD_TYPE=Release"])
    command(["cmake", "--build", "target/native-oracle", "--target",
             "webpop_lossy_benchmark", "--parallel", "4"])
    pin = command(["git", "-C", "upstream", "rev-parse", "HEAD"]).strip()
    if pin != "1844042d1ca751e62e61df7617dba19c98588339" or command(
            ["git", "-C", "upstream", "status", "--porcelain", "--untracked-files=all"]):
        raise RuntimeError("baseline requires the clean pinned upstream")
    cpu = args.cpu if args.cpu is not None else min(os.sched_getaffinity(0))
    os.sched_setaffinity(0, {cpu})
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    for name in ["inputs", "outputs", "rss", "provenance"]:
        (out / name).mkdir()
    helper = ROOT / "target/release/examples/benchmark_images"
    binaries = {"rust": ROOT / "target/release/examples/lossy_benchmark",
                "c": ROOT / "target/native-oracle/webpop_lossy_benchmark"}
    provenance = {"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  "parent": command(["git", "rev-parse", "HEAD"]).strip(),
                  "upstream": pin, "runs": args.runs, "warmups": 1, "cpu": cpu,
                  "platform": platform.platform(), "rustc": command(["rustc", "-vV"]),
                  "cpu_info": command(["lscpu"]),
                  "binaries": {k: {"path": str(v), "sha256": digest(v)} for k, v in binaries.items()},
                  "environment": {k: v for k, v in os.environ.items() if k.startswith(
                      ("RUSTFLAGS", "CARGO_PROFILE_", "CARGO_ENCODED_RUSTFLAGS", "CC", "CFLAGS", "LDFLAGS"))}}
    save(out / "provenance.json", provenance)
    for source in [ROOT / "target/native-oracle/CMakeCache.txt",
                   ROOT / "target/native-oracle/compile_commands.json", ROOT / "Cargo.lock"]:
        (out / "provenance" / source.name).write_bytes(source.read_bytes())
    (out / "provenance/working-tree.patch").write_text(command(["git", "diff", "HEAD", "--binary"]))
    for source in [Path(__file__), ROOT / "cwebpop/examples/lossy_benchmark.rs",
                   ROOT / "cwebpop/examples/benchmark_images.rs",
                   ROOT / "libwebpop/tests/reference/lossy_benchmark.c"]:
        (out / "provenance" / source.name).write_bytes(source.read_bytes())
    inputs = []
    for size, source in [("small", ROOT / "libwebpop/tests/fixtures/corpus/test_ref.ppm"),
                         ("large", ROOT / "samples/photo-japan-p50.webp")]:
        prepared = out / "inputs" / f"{size}.pam"
        info = json.loads(command([helper, "prepare", source, prepared, args.edge]))
        content = prepared.read_bytes()
        header, plane = content.split(b"ENDHDR\n", 1)
        w, h = info["width"], info["height"]
        if len(plane) != w * h * 4:
            raise RuntimeError("unexpected prepared geometry")
        for alpha in [False, True]:
            name = f"{size}-{'alpha' if alpha else 'opaque'}"
            data = bytearray(plane)
            data[3::4] = bytes(((17 * x + 13 * y) & 255) for y in range(h) for x in range(w)) if alpha else bytes([255]) * (w * h)
            pam, raw = out / "inputs" / f"{name}.pam", out / "inputs" / f"{name}.rgba"
            pam.write_bytes(header + b"ENDHDR\n" + data)
            raw.write_bytes(data)
            inputs.append({"name": name, "width": w, "height": h, "source": str(source.relative_to(ROOT)),
                           "source_sha256": digest(source), "rgba_sha256": digest(raw),
                           "alpha": alpha, "pam": str(pam), "raw": str(raw)})
    save(out / "inputs.json", inputs)
    rows, samples = [], []
    rng = random.Random(0x6D2B79F5)
    for image in inputs:
        for method in range(7):
            for aq in ([70, 100] if image["alpha"] else [100]):
                stem = f"{image['name']}-q75-a{aq}-m{method}"
                results = {}
                labels = list(binaries)
                rng.shuffle(labels)
                for label in labels:
                    output = out / "outputs" / f"{stem}-{label}.webp"
                    rss = out / "rss" / f"{stem}-{label}.txt"
                    argv = ["/usr/bin/time", "-f", "%M", "-o", rss, binaries[label], image["raw"],
                            image["width"], image["height"], 75, aq, method, args.runs, output]
                    times = json.loads(command(argv))
                    results[label] = {"output": output, "times": times,
                                      "median_ms": statistics.median(times) * 1000,
                                      "rss_kib": int(rss.read_text())}
                    samples.append({"image": image["name"], "quality": 75, "alpha_quality": aq,
                                    "method": method, "label": label, "seconds": times,
                                    "peak_process_rss_kib": results[label]["rss_kib"],
                                    "command": list(map(str, argv))})
                rust, c = results["rust"], results["c"]
                if rust["output"].read_bytes() != c["output"].read_bytes():
                    raise RuntimeError(f"complete byte mismatch: {stem}")
                metrics = json.loads(command([helper, "metrics", image["pam"], rust["output"]]))
                if metrics != json.loads(command([helper, "metrics", image["pam"], c["output"]])):
                    raise RuntimeError(f"decoded metric mismatch: {stem}")
                if aq == 100 and metrics["alpha_sse"] != 0:
                    raise RuntimeError(f"alpha differs at quality 100: {stem}")
                row = {"image": image["name"], "width": image["width"], "height": image["height"],
                       "quality": 75, "alpha_quality": aq, "method": method,
                       "bytes": rust["output"].stat().st_size, "sha256": digest(rust["output"]),
                       **metrics, "rust_median_ms": rust["median_ms"], "c_median_ms": c["median_ms"],
                       "c_over_rust": c["median_ms"] / rust["median_ms"],
                       "rust_peak_process_rss_kib": rust["rss_kib"], "c_peak_process_rss_kib": c["rss_kib"]}
                rows.append(row)
                save(out / "samples.json", samples)
                save(out / "summary.json", rows)
                print(f"{stem}: {row['bytes']} B; Rust {rust['median_ms']:.3f} ms, C {c['median_ms']:.3f} ms", flush=True)
    with (out / "summary.csv").open("w") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    print(f"Saved {len(rows)} exact-byte paired cases to {out}")


if __name__ == "__main__":
    main()
