use super::{Case, SEED, random};
use libwebpop::enc::backward_references_cost_enc::*;
use libwebpop::enc::backward_references_enc::*;
use libwebpop::utils::color_cache_utils::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if case.function == "VP8LCostEstimates" {
        let mut out = vec![0; a.len()];
        ConvertPopulationCountTableToBitEstimates(a, &mut out);
        return out;
    }
    if case.function == "VP8LGetBackwardReferences" {
        return get_refs(case);
    }

    let (width, height, quality, low, strategy, bits) =
        (a[0] as usize, a[1] as usize, a[2], a[3] != 0, a[4], a[5]);
    let pixels = &a[6..];
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    VP8LHashChainFill(&mut chain, quality, pixels, width, height, low).unwrap();
    let mut refs = VP8LBackwardRefsInit(256);
    match strategy {
        0 => (),
        1 => BackwardReferencesRle(width, height, pixels, bits, &mut refs).unwrap(),
        2 | 4 => BackwardReferencesLz77(
            width,
            height,
            pixels,
            if strategy == 4 { 0 } else { bits },
            &chain,
            &mut refs,
        )
        .unwrap(),
        3 => {
            let mut box_chain = VP8LHashChainInit(pixels.len()).unwrap();
            BackwardReferencesLz77Box(
                width,
                height,
                pixels,
                bits,
                &chain,
                &mut box_chain,
                &mut refs,
            )
            .unwrap();
            chain = box_chain;
        }
        _ => panic!("strategy"),
    }
    if case.function == "VP8LTrace" {
        let m = CostModelBuild(width as u32, bits, &refs).unwrap();
        let mut out = m.literal.clone();
        out.extend(m.red);
        out.extend(m.blue);
        out.extend(m.alpha);
        out.extend(m.distance);
        let mut dist =
            BackwardReferencesHashChainDistanceOnly(width, height, pixels, bits, &chain, &refs)
                .unwrap();
        out.extend(dist.iter().map(|&v| u32::from(v)));
        let start = TraceBackwards(&mut dist);
        out.push((dist.len() - start) as u32);
        out.extend(dist[start..].iter().map(|&v| u32::from(v)));
        let mut refined = VP8LBackwardRefsInit(256);
        BackwardReferencesHashChainFollowChosenPath(
            pixels,
            bits,
            &dist[start..],
            &chain,
            &mut refined,
        )
        .unwrap();
        replay(&refined, bits, pixels);
        let mut integrated = VP8LBackwardRefsInit(256);
        VP8LBackwardReferencesTraceBackwards(
            width,
            height,
            pixels,
            bits,
            &chain,
            &refs,
            &mut integrated,
        )
        .unwrap();
        assert!(refined.iter().eq(integrated.iter()));
        super::references::inspect_refs(&refined, &mut out);
        return out;
    }
    let mut out = chain.offset_length;
    let mut replay_bits = bits;
    if strategy == 4 {
        CalculateBestCacheSize(pixels, quality, &refs, &mut replay_bits).unwrap();
        out.push(replay_bits);
        if replay_bits > 0 {
            BackwardRefsWithLocalCache(pixels, replay_bits, &mut refs).unwrap();
        }
    }
    if strategy != 0 {
        replay(&refs, replay_bits, pixels);
    }
    super::references::inspect_refs(&refs, &mut out);
    out
}

/// Independent decoder for raw references; overlapping copies read the growing
/// output. Every decoded pixel updates the cache, including cached literals.
fn replay(refs: &VP8LBackwardRefs, bits: u32, expected: &[u32]) {
    let mut cache = if bits == 0 {
        None
    } else {
        Some(VP8LColorCacheInit(bits).unwrap())
    };
    let mut out = Vec::new();
    for p in refs.iter() {
        for _ in 0..PixOrCopyLength(p) {
            let pixel = if PixOrCopyIsLiteral(p) {
                (0..4)
                    .map(|c| PixOrCopyLiteral(p, c) << (8 * c))
                    .fold(0, |a, b| a | b)
            } else if PixOrCopyIsCacheIdx(p) {
                VP8LColorCacheLookup(cache.as_ref().unwrap(), PixOrCopyCacheIdx(p))
            } else {
                let distance = PixOrCopyDistance(p) as usize;
                assert!(distance > 0 && distance <= out.len());
                out[out.len() - distance]
            };
            out.push(pixel);
            if let Some(c) = &mut cache {
                VP8LColorCacheInsert(c, pixel);
            }
        }
    }
    assert_eq!(out, expected, "reference replay");
}

fn image(width: usize, height: usize, kind: usize, seed: &mut u32) -> Vec<u32> {
    let n = width * height;
    let mut pixels = Vec::with_capacity(n);
    for i in 0..n {
        let pixel = match kind {
            0 => 0,
            1 => 0xff123456,
            2 => (i % 2) as u32 * 0x11223344,
            3 => ((i / width) % 3 + i % width % 7) as u32 * 0x01010101,
            4 => random(seed),
            5 => random(seed) % 17 * 0x0e0e0e0e,
            6 => (i / 4096 % 2) as u32 * 0xff020304,
            7 if i >= width => pixels[i % width],
            7 => random(seed),
            8 if i >= 37 && i % 53 != 0 => pixels[i - 37],
            _ => random(seed) % 5,
        };
        pixels.push(pixel);
    }
    pixels
}

fn case(
    width: usize,
    height: usize,
    quality: u32,
    low: bool,
    strategy: u32,
    bits: u32,
    pixels: &[u32],
) -> Case {
    let mut input = vec![
        width as u32,
        height as u32,
        quality,
        low as u32,
        strategy,
        bits,
    ];
    input.extend(pixels);
    Case::new("VP8LMatches", input)
}

pub fn matching_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    // Quality/window boundaries, narrow/row-boundary images, all cache sizes.
    for (width, height) in [
        (1, 1),
        (1, 2),
        (1, 3),
        (2, 2),
        (1, 43),
        (43, 1),
        (3, 19),
        (8, 17),
        (17, 19),
        (65, 11),
    ] {
        for kind in 0..9 {
            let pixels = image(width, height, kind, &mut seed);
            for quality in [0, 25, 26, 50, 51, 70, 75, 76, 100] {
                for strategy in 0..=4 {
                    cases.push(case(
                        width,
                        height,
                        quality,
                        false,
                        strategy,
                        (quality + kind as u32) % 11,
                        &pixels,
                    ));
                }
                cases.push(case(width, height, quality, true, 2, 0, &pixels));
            }
            for bits in 0..=10 {
                for strategy in 1..=4 {
                    cases.push(case(width, height, 70, false, strategy, bits, &pixels));
                }
            }
        }
    }
    // Exhaust binary images: exact tie choices matter even when replay agrees.
    for pattern in 0..256u32 {
        let pixels: Vec<_> = (0..8)
            .map(|i| {
                if pattern & (1 << i) != 0 {
                    0xff000001
                } else {
                    0
                }
            })
            .collect();
        for width in [1, 2, 4, 8] {
            for strategy in 0..=4 {
                cases.push(case(width, 8 / width, 70, false, strategy, 3, &pixels));
            }
        }
    }
    // Max-match saturation, run-hash skipping, long left extension, quality
    // window cutoffs and distant repeats. Keep protocol responses manageable.
    for n in [4094, 4095, 4096, 4097, 8193, 20000] {
        for kind in [0, 1, 6, 8] {
            let pixels = image(n, 1, kind, &mut seed);
            for strategy in 0..=4 {
                // width stays within the codec limit, including the 20000 run.
                let (w, h) = if n > 16383 { (100, n / 100) } else { (n, 1) };
                cases.push(case(w, h, 70, false, strategy, 10, &pixels));
                cases.push(case(w, h, 100, true, strategy, 0, &pixels));
            }
        }
    }
    for _ in 0..200 {
        let width = 1 + random(&mut seed) as usize % 101;
        let height = 1 + random(&mut seed) as usize % 31;
        let kind = random(&mut seed) as usize % 9;
        let pixels = image(width, height, kind, &mut seed);
        let quality = random(&mut seed) % 101;
        let bits = random(&mut seed) % 11;
        for strategy in 0..=4 {
            cases.push(case(width, height, quality, false, strategy, bits, &pixels));
        }
    }
    cases
}

pub fn matching_fixture_cases() -> Vec<Case> {
    let mut seed = SEED;
    let mut cases = Vec::new();
    for (w, h, kind) in [
        (1, 1, 0),
        (1, 2, 1),
        (3, 3, 2),
        (5, 7, 3),
        (7, 5, 5),
        (13, 3, 7),
        (9, 7, 8),
    ] {
        let pixels = image(w, h, kind, &mut seed);
        for strategy in 0..=4 {
            cases.push(case(w, h, 70, false, strategy, 5, &pixels));
        }
        cases.push(case(w, h, 100, true, 2, 0, &pixels));
    }
    cases
}

fn get_refs(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let (w, h) = (a[0] as usize, a[1] as usize);
    let pixels = &a[7..];
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    VP8LHashChainFill(&mut chain, a[2], pixels, w, h, a[3] != 0).unwrap();
    let mut refs = std::array::from_fn::<_, 3, _>(|_| VP8LBackwardRefsInit(256));
    let bits = VP8LGetBackwardReferences(
        w,
        h,
        pixels,
        a[2],
        a[3] != 0,
        a[4],
        a[5],
        a[6] != 0,
        &chain,
        &mut refs,
    )
    .unwrap();
    let mut out = vec![bits];
    for i in 0..if a[6] != 0 { 2 } else { 1 } {
        super::references::inspect_refs(&refs[i], &mut out);
        if a[3] != 0 && i == 1 {
            continue;
        }
        let mut raw = VP8LBackwardRefsInit(256);
        for &p in refs[i].iter() {
            let p = if PixOrCopyIsCopy(&p) {
                let code = PixOrCopyDistance(&p);
                let distance = if code > 120 {
                    code - 120
                } else {
                    (1..=(w as u32 * 8 + 8))
                        .find(|&d| VP8LDistanceToPlaneCode(w as u32, d) == code)
                        .unwrap()
                };
                PixOrCopyCreateCopy(distance, PixOrCopyLength(&p) as u16)
            } else {
                p
            };
            VP8LBackwardRefsCursorAdd(&mut raw, p).unwrap();
        }
        replay(&raw, if i == 0 { bits } else { 0 }, pixels);
    }
    out
}

pub fn traceback_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // Reuse independently defined matching inputs, including long saturated runs.
    for c in matching_cases() {
        if (1..=3).contains(&c.input[4]) {
            cases.push(Case::new("VP8LTrace", c.input));
        }
    }
    for len in [1, 2, 3, 4, 40, 256, 280, 2328] {
        let mut seed = SEED;
        for kind in 0..5 {
            let counts: Vec<_> = (0..len)
                .map(|i| match kind {
                    0 => 0,
                    1 => u32::from(i == 0),
                    2 => 1,
                    3 => random(&mut seed) % 10000,
                    _ => random(&mut seed) % 1000000,
                })
                .collect();
            cases.push(Case::new("VP8LCostEstimates", counts));
        }
    }
    cases
}

pub fn selection_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (w, h) in [
        (1, 1),
        (1, 2),
        (3, 3),
        (1, 101),
        (101, 1),
        (7, 11),
        (17, 31),
        (65, 23),
        (100, 100),
    ] {
        for kind in 0..9 {
            let pixels = image(w, h, kind, &mut seed);
            for quality in [0, 24, 25, 26, 70, 100] {
                for types in 1..=7 {
                    for no_cache in [0, 1] {
                        let mut input = vec![
                            w as u32,
                            h as u32,
                            quality,
                            0,
                            types,
                            (kind as u32 + quality) % 11,
                            no_cache,
                        ];
                        input.extend(&pixels);
                        cases.push(Case::new("VP8LGetBackwardReferences", input));
                    }
                }
            }
            for no_cache in [0, 1] {
                let mut input = vec![w as u32, h as u32, 70, 1, 7, 10, no_cache];
                input.extend(&pixels);
                cases.push(Case::new("VP8LGetBackwardReferences", input));
            }
        }
    }
    cases
}

pub fn traceback_fixture_cases() -> Vec<Case> {
    let mut cases = vec![
        Case::new("VP8LPrefixEncode", vec![0]),
        Case::new("VP8LPrefixEncodeBits", vec![0]),
    ];
    for c in matching_fixture_cases() {
        if c.input[4] == 2 || c.input[4] == 3 {
            // No cache for compact cost-model fixtures (1088 words per case).
            let mut input = c.input;
            input[5] = 0;
            cases.push(Case::new("VP8LTrace", input.clone()));
            input[4] = 7;
            input[5] = 5;
            input.insert(6, 1);
            cases.push(Case::new("VP8LGetBackwardReferences", input));
        }
    }
    cases
}
