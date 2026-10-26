#!/usr/bin/env python3
"""Aggregate period-weighted encoder stages from perf captures."""
import argparse
import collections
import json
from pathlib import Path
import re
import subprocess

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

LOSSY_STAGES = {
    "rgba_to_yuv": r"WebPPictureImport|import_rgba_to_yuva|ImportYUVA|ConvertARGBToYUV|WebPPictureARGBToYUVA",
    "analysis": r"VP8EncAnalyze|DoSegmentsJob|MBAnalyze|CollectHistogram",
    "intra4_search": r"PickBestIntra4",
    "intra16_search": r"PickBestIntra16",
    "chroma_search": r"PickBestUV",
    "other_decimation": r"VP8Decimate|\bDecimate\* volatile",
    "token_recording": r"VP8RecordCoeffTokens|RecordTokens",
    "token_emission": r"VP8EmitTokens",
    "probability_updates": r"VP8CalculateLevelCosts|FinalizeTokenProbas|FinalizeSkipProba|VP8EstimateTokenSize",
    "other_residuals": r"VP8RecordCoeffs|RecordResiduals|CodeResiduals",
    "other_encode_loop": r"VP8EncTokenLoop|VP8EncLoop",
}
LOSSY_FUNCTIONS = [
    "VP8Decimate", "PickBestIntra4", "PickBestIntra16", "PickBestUV",
    "ReconstructIntra4", "ReconstructIntra16", "ReconstructUV",
    "FTransform", "ITransform", "QuantizeBlock", "Quantize2Blocks",
    "GetResidualCost", "VP8GetCostLuma4", "VP8GetCostLuma16", "VP8GetCostUV",
    "VP8RecordCoeffTokens", "VP8EmitTokens", "VP8EncAnalyze", "CollectHistogram",
    "SSE4x4", "SSE16x16", "Disto4x4", "Disto16x16",
    "Intra4Preds", "Intra16Preds", "IntraChromaPreds",
    "import_rgba_to_yuva", "WebPPictureImport",
]
LOSSY_KERNELS = {
    "inverse_transform": r"ITransform",
    "forward_transform": r"FTransform",
    "quantization": r"QuantizeBlock|Quantize2Blocks",
    "spectral_distortion": r"Disto4x4|Disto16x16|TTransform",
    "pixel_distortion": r"SSE16x16|SSE16x8|SSE8x8|SSE4x4|GetSSE",
    "residual_cost": r"GetResidualCost|SetResidualCoeffs|VP8GetCostLuma|VP8GetCostUV",
    "prediction": r"Intra4Preds|Intra16Preds|IntraChromaPreds|TrueMotion",
    "copy_or_initialize": r"memcpy|memmove|memset|copy_from_slice|default\b",
}


def summarize(out, label, stage_patterns=STAGES, function_names=FUNCTIONS, kernel_patterns=None):
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
    kernels = collections.Counter()
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
        stage = next((name for name, pattern in stage_patterns.items()
                      if re.search(pattern, stack)), "other")
        stages[stage] += weight
        samples[stage] += 1
        if kernel_patterns:
            kernel = next((name for name, pattern in kernel_patterns.items()
                           if re.search(pattern, stack)), "other")
            kernels[kernel] += weight
        for name in function_names:
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
        "kernels": {name: {"percent": 100 * weight / total,
                           "estimated_cycles": weight}
                    for name, weight in kernels.items()},
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--mode", choices=["lossless", "lossy"], default="lossless")
    args = parser.parse_args()
    directory = args.directory
    patterns, names = (LOSSY_STAGES, LOSSY_FUNCTIONS) if args.mode == "lossy" else (STAGES, FUNCTIONS)
    result = {"mode": args.mode, "stage_patterns_in_priority_order": patterns,
              "kernel_patterns_in_priority_order": LOSSY_KERNELS if args.mode == "lossy" else {},
              "profiles": {label: summarize(directory, label, patterns, names,
                                           LOSSY_KERNELS if args.mode == "lossy" else None)
                           for label in ("c", "rust")}}
    (directory / "comparison.json").write_text(json.dumps(result, indent=2) + "\n")
