//! Staged histogram clustering transcripts and complete sampling-buffer checks.
use super::{Case, SEED, random, words64};
use libwebpop::enc::{backward_references_enc::*, histogram_enc::*, predictor_enc::*};

pub fn inspect_set(set: &VP8LHistogramSet, out: &mut Vec<u32>, raw: bool) {
    out.push(set.size as u32);
    for h in &set.histograms[..set.size] {
        let used = !raw || h.is_used != [0; 5];
        out.push(u32::from(used));
        if !used {
            continue;
        }
        for i in 0..5 {
            let p = GetPopulationInfo(h, i);
            out.push(p.iter().filter(|&&v| v != 0).count() as u32);
            for (j, &v) in p.iter().enumerate() {
                if v != 0 {
                    out.extend([j as u32, v]);
                }
            }
        }
        out.extend(h.trivial_symbol.map(u32::from));
        out.extend(words64(h.bit_cost));
        for cost in h.costs {
            out.extend(words64(cost));
        }
        out.extend(h.is_used.map(u32::from));
        out.push(u32::from(h.bin_id));
    }
}

pub fn refs(a: &[u32]) -> VP8LBackwardRefs {
    let mut refs = VP8LBackwardRefsInit(256);
    for t in a.chunks_exact(3) {
        let token = match t[0] {
            0 => PixOrCopyCreateLiteral(t[1]),
            1 => PixOrCopyCreateCacheIdx(t[1]),
            2 => PixOrCopyCreateCopy(t[1], t[2] as u16),
            _ => unreachable!(),
        };
        VP8LBackwardRefsCursorAdd(&mut refs, token).unwrap();
    }
    refs
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if case.function == "VP8LOptimizeSampling" {
        let mut image = a[4..].to_vec();
        let bits = VP8LOptimizeSampling(&mut image, a[0] as usize, a[1] as usize, a[2], a[3]);
        let mut out = vec![bits];
        out.extend(image);
        return out;
    }
    let (w, h, q, low, bits, cache) = (a[0] as usize, a[1] as usize, a[2], a[3] != 0, a[4], a[5]);
    let size = w.div_ceil(1 << bits) * h.div_ceil(1 << bits);
    let refs = refs(&a[6..]);
    let mut orig = VP8LAllocateHistogramSet(size, cache).unwrap();
    let mut image = VP8LAllocateHistogramSet(size, cache).unwrap();
    let mut tmp = VP8LAllocateHistogram(cache).unwrap();
    let mut out = Vec::new();
    HistogramBuild(w, bits, &refs, &mut orig);
    HistogramCopyAndAnalyze(&mut orig, &mut image);
    inspect_set(&orig, &mut out, true);
    inspect_set(&image, &mut out, false);
    let bins = if low { 4 } else { 64 };
    let combine = image.size > bins * 2 && q < 100;
    out.push(u32::from(combine));
    if combine {
        HistogramAnalyzeEntropyBin(&mut image, low);
    }
    inspect_set(&image, &mut out, false);
    if combine {
        HistogramCombineEntropyBin(
            &mut image,
            &mut tmp,
            bins,
            GetCombineCostFactor(size, q),
            low,
        );
    }
    inspect_set(&image, &mut out, false);
    let mut greedy = false;
    if !low || !combine {
        let threshold =
            1 + libwebpop::dsp::lossless_common::DivRound(i64::from(q).pow(3) * 99, 1000000)
                as usize;
        greedy = HistogramCombineStochastic(&mut image, threshold).unwrap();
    }
    out.push(u32::from(greedy));
    inspect_set(&image, &mut out, false);
    if greedy {
        HistogramCombineGreedy(&mut image).unwrap();
    }
    inspect_set(&image, &mut out, false);
    let mut symbols = vec![0; size];
    HistogramRemap(&orig, &mut image, &mut symbols);
    out.extend_from_slice(&symbols);
    inspect_set(&image, &mut out, false);
    let mut integrated = VP8LAllocateHistogramSet(size, cache).unwrap();
    let mut actual_symbols = vec![u32::MAX; size];
    VP8LGetHistoImageSymbols(
        w,
        h,
        &refs,
        q,
        low,
        bits,
        cache,
        &mut integrated,
        &mut tmp,
        &mut actual_symbols,
    )
    .unwrap();
    assert_eq!(actual_symbols, symbols);
    assert_eq!(integrated.size, image.size);
    assert_eq!(
        &integrated.histograms[..image.size],
        &image.histograms[..image.size]
    );
    inspect_set(&integrated, &mut out, false);
    out.extend(actual_symbols);
    out
}

pub fn clustering_case(
    w: u32,
    h: u32,
    bits: u32,
    cache: u32,
    pattern: u32,
    quality: u32,
    low: bool,
) -> Case {
    let mut a = vec![w, h, quality, u32::from(low), bits, cache];
    let mut seed = SEED;
    let mut pos = 0;
    while pos < w * h {
        let rnd = random(&mut seed);
        let x = pos % w;
        let y = pos / w;
        let tile = ((y >> bits) * w.div_ceil(1 << bits)) + (x >> bits);
        let (mode, payload, len) = match pattern {
            0 => (0, 0xff123456, 1),
            1 => (0, rnd, 1),
            2 => (0, (tile % 4) * 0x55555555, 1),
            3 => (0, ((rnd % (1 + tile % 16)) * 0x10101) | 0xff000000, 1),
            4 if pos > 0 && pos % 7 == 1 => (2, 1 + rnd % 120, (w * h - pos).min(4095)),
            5 if cache > 0 && pos % 3 != 0 => (1, rnd % (1 << cache), 1),
            6 if pos > 0 && rnd % 3 == 0 => (2, 1 + rnd % 1048576, (2 + rnd % 37).min(w * h - pos)),
            // Regional distributions retain several groups during remapping.
            7 => (0, ((tile % 5) * 0x30303030) | (rnd % 4 * 0x10101), 1),
            8 => (0, ((tile % 2) * 0xff0000) | ((rnd % 2) << 8), 1),
            _ => (0, rnd % 4 * 0x55555555, 1),
        };
        a.extend([mode, payload, len]);
        pos += len;
    }
    Case::new("VP8LClustering", a)
}

pub fn sampling_case(w: u32, h: u32, bits: u32, max_bits: u32, pattern: u32) -> Case {
    let mut a = vec![w, h, bits, max_bits];
    let mut seed = SEED;
    let width = w.div_ceil(1 << bits);
    let height = h.div_ceil(1 << bits);
    for y in 0..height {
        for x in 0..width {
            let v = match pattern {
                0 => 0xff123456,
                1 => random(&mut seed),
                2 => ((x / 2) + (y / 2) * width) * 0x10101,
                3 => ((x / 4) + (y / 4) * width) * 0x10101,
                4 => (x / 2 + y / 4 * width) * 0x10101,
                5 => (x / 4 + y / 2 * width) * 0x10101,
                6 => u32::from(x == width - 1 && y == height - 1),
                _ => u32::from(x == width / 2 || y == height / 2),
            };
            a.push(v);
        }
    }
    Case::new("VP8LOptimizeSampling", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for pattern in 0..9 {
        out.push(clustering_case(17, 9, 1, 4, pattern, 70, false));
    }
    for (pattern, low, quality) in [(3, false, 70), (8, false, 100), (7, true, 70)] {
        out.push(clustering_case(65, 9, 1, 0, pattern, quality, low));
    }
    for pattern in 0..8 {
        out.push(sampling_case(17, 11, 1, 5, pattern));
    }
    out
}

pub fn cases() -> Vec<Case> {
    let mut out = fixture_cases();
    for (w, h, bits) in [
        (1, 1, 1),
        (1, 129, 2),
        (129, 1, 2),
        (9, 9, 1),
        (31, 33, 2),
        (33, 17, 1),
        (65, 17, 1),
        (65, 33, 1),
        (129, 33, 1),
    ] {
        for pattern in 0..9 {
            for quality in [0, 25, 50, 70, 89, 90, 99, 100] {
                for low in [false, true] {
                    out.push(clustering_case(
                        w,
                        h,
                        bits,
                        if pattern % 2 == 0 { 0 } else { 4 },
                        pattern,
                        quality,
                        low,
                    ));
                }
            }
        }
    }
    // Every binary 3x3 sampling map; includes row-only/column-only obstacles.
    for mask in 0..512 {
        let mut a = vec![3, 3, 0, 4];
        a.extend((0..9).map(|bit| (mask >> bit) & 1));
        out.push(Case::new("VP8LOptimizeSampling", a));
    }
    for (w, h) in [
        (1, 1),
        (1, 65),
        (65, 1),
        (3, 5),
        (17, 11),
        (32, 32),
        (33, 35),
        (65, 67),
    ] {
        for bits in 0..=4 {
            for max_bits in bits..=9 {
                for pattern in 0..8 {
                    out.push(sampling_case(w, h, bits, max_bits, pattern));
                }
            }
        }
    }
    out
}
