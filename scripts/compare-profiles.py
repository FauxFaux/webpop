#!/usr/bin/env python3
"""Aggregate period-weighted encoder stages from profile-map.sh captures."""
import collections
import json
from pathlib import Path
import re
import subprocess
import sys

# First matching stage wins; each sample belongs to exactly one stage.
STAGES = {
    "predictor_transform": r"VP8LResidualImage|GetBestPredictorsAndSubSampling|ComputeResidualsForTile|GetResidual|PredictPixel|UpdateHisto",
    "hash_chain": r"VP8LHashChainFill",
    "lz77_box": r"BackwardReferencesLz77Box",
    "trace_backwards": r"VP8LBackwardReferencesTraceBackwards|BackwardReferencesHashChainDistanceOnly",
    "lz77_other": r"BackwardReferencesLz77",
    "cache_size": r"CalculateBestCacheSize",
    "palette": r"ApplyPalette|MapImageFromPalette|PaletteSort|CoOccurrenceBuild",
}
FUNCTIONS = [
    "UpdateHisto", "GetResidual", "PredictPixel", "ComputeResidualsForTile",
    "GetBestPredictorsAndSubSampling", "VP8LResidualImage", "VP8LHashChainFill",
    "VectorMismatch", "BackwardReferencesLz77Box", "VP8LGetBackwardReferences",
]


def summarize(out, label):
    stacks = out / f"{label}.stacks.txt"
    with stacks.open("w") as stream:
        subprocess.run([
            "perf", "script", "-i", str(out / f"{label}.perf.data"),
            "-F", "comm,period,event,ip,sym,dso", "--inline",
        ], stdout=stream, check=True)
    total = count = unknown = main = 0
    stages = collections.Counter()
    samples = collections.Counter()
    functions = collections.Counter()
    for block in stacks.read_text().split("\n\n"):
        lines = block.splitlines()
        if not lines:
            continue
        match = re.match(r"\S+\s+(\d+)\s+[^\s]+:\s*$", lines[0])
        if not match or len(lines) < 2:
            raise ValueError(f"unrecognized perf script sample: {block}")
        weight = int(match[1])
        total += weight
        count += 1
        stack = "\n".join(lines[1:])
        if "[unknown]" in lines[1]:
            unknown += weight
        if re.search(r"\bmain(?: \(inlined\))? \(", stack):
            main += weight
        stage = next((name for name, pattern in STAGES.items()
                      if re.search(pattern, stack)), "other")
        stages[stage] += weight
        samples[stage] += 1
        for name in FUNCTIONS:
            if name in stack:
                functions[name] += weight
    if not total:
        raise ValueError(f"no samples for {label}")
    return {
        "samples": count, "estimated_cycles": total,
        "unknown_leaf_percent": 100 * unknown / total,
        "main_in_stack_percent": 100 * main / total,
        "stages": {name: {"percent": 100 * weight / total,
                          "estimated_cycles": weight, "samples": samples[name]}
                   for name, weight in stages.items()},
        "inclusive_percent": {name: 100 * weight / total
                              for name, weight in functions.items()},
    }


if __name__ == "__main__":
    directory = Path(sys.argv[1])
    result = {"stage_patterns_in_priority_order": STAGES,
              "profiles": {label: summarize(directory, label)
                           for label in ("c", "rust")}}
    (directory / "comparison.json").write_text(json.dumps(result, indent=2) + "\n")
