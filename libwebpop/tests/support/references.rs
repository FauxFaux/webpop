use super::{Case, SEED, random, words64};
use libwebpop::internal::enc::{backward_references_enc::*, histogram_enc::*};
use libwebpop::internal::webp::format_constants::LZ77_WINDOW_SIZE;

fn token(op: &[u32]) -> PixOrCopy {
    match op[0] {
        0 => PixOrCopyCreateLiteral(op[1]),
        1 => PixOrCopyCreateCacheIdx(op[1]),
        2 => PixOrCopyCreateCopy(op[1], op[2] as u16),
        _ => panic!("not a token"),
    }
}

pub fn inspect_refs(refs: &VP8LBackwardRefs, out: &mut Vec<u32>) {
    let start = out.len();
    out.push(0);
    let mut c = VP8LRefsCursorInit(refs);
    while VP8LRefsCursorOk(&c) {
        let p = c.cur_pos.unwrap();
        let (mode, value) = if PixOrCopyIsLiteral(p) {
            (
                0,
                (0..4)
                    .map(|i| PixOrCopyLiteral(p, i) << (8 * i))
                    .fold(0, |a, b| a | b),
            )
        } else if PixOrCopyIsCacheIdx(p) {
            (1, PixOrCopyCacheIdx(p))
        } else {
            assert!(PixOrCopyIsCopy(p));
            (2, PixOrCopyDistance(p))
        };
        out.extend([mode, PixOrCopyLength(p), value]);
        out[start] += 1;
        VP8LRefsCursorNext(&mut c);
    }
}

fn inspect_histogram(h: &VP8LHistogram, out: &mut Vec<u32>) {
    out.push(h.palette_code_bits);
    for i in 0..5 {
        out.extend(GetPopulationInfo(h, i));
    }
    out.extend(h.trivial_symbol.map(u32::from));
    out.extend(words64(h.bit_cost));
    for cost in h.costs {
        out.extend(words64(cost));
    }
    out.extend(h.is_used.map(u32::from));
    out.push(u32::from(h.bin_id));
    out.extend(words64(VP8LHistogramEstimateBits(h)));
}

fn dirty_histogram(h: &mut VP8LHistogram) {
    for pop in [
        &mut h.literal[..],
        &mut h.red,
        &mut h.blue,
        &mut h.alpha,
        &mut h.distance,
    ] {
        for (i, value) in pop.iter_mut().enumerate() {
            *value = i as u32 + 1;
        }
    }
    ComputeHistogramCost(h);
    h.bin_id = 77;
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let mut out = Vec::new();
    match case.function.as_str() {
        "VP8LDistanceToPlaneCode" => out.push(VP8LDistanceToPlaneCode(a[0], a[1])),
        "PopulationCost" => {
            out.extend(words64(VP8LBitsEntropy(a)));
            let (cost, trivial, used) = PopulationCost(a);
            out.extend(words64(cost));
            out.extend([u32::from(trivial), u32::from(used)]);
        }
        "VP8LBackwardRefs" => {
            let mut refs = VP8LBackwardRefsInit(a[0] as usize);
            let mut saved = VP8LBackwardRefsInit(a[0] as usize);
            for op in a[1..].chunks_exact(3) {
                match op[0] {
                    0..=2 => VP8LBackwardRefsCursorAdd(&mut refs, token(op)).unwrap(),
                    3 => VP8LClearBackwardRefs(&mut refs),
                    4 => BackwardRefsClone(&refs, &mut saved).unwrap(),
                    5 => BackwardRefsSwap(&mut refs, &mut saved),
                    6 => VP8LBackwardRefsClear(&mut refs),
                    7 => BackwardReferences2DLocality(op[1], &mut refs),
                    8 => inspect_refs(&refs, &mut out),
                    _ => panic!("unknown reference operation"),
                }
            }
            inspect_refs(&refs, &mut out);
            inspect_refs(&saved, &mut out);
        }
        "VP8LHistogram" => {
            let mut h = VP8LAllocateHistogram(a[0]).unwrap();
            let mut saved = VP8LAllocateHistogram(a[0]).unwrap();
            let mut refs = VP8LBackwardRefsInit(256);
            let modifier: Option<DistanceModifier> = (a[1] != 0).then_some(VP8LDistanceToPlaneCode);
            for op in a[2..].chunks_exact(3) {
                match op[0] {
                    0..=2 => VP8LBackwardRefsCursorAdd(&mut refs, token(op)).unwrap(),
                    3 => VP8LHistogramStoreRefs(&refs, modifier, a[1], &mut h),
                    4 => VP8LHistogramCreate(&mut h, &refs, (op[1] != 0).then_some(a[0])).unwrap(),
                    5 => VP8LHistogramInit(&mut h, a[0], op[1] != 0).unwrap(),
                    6 => ComputeHistogramCost(&mut h),
                    7 => inspect_histogram(&h, &mut out),
                    8 => {
                        let mut index = op[1] as usize;
                        let mut found = false;
                        for pop in [
                            &mut h.literal[..],
                            &mut h.red,
                            &mut h.blue,
                            &mut h.alpha,
                            &mut h.distance,
                        ] {
                            if index < pop.len() {
                                pop[index] = op[2];
                                found = true;
                                break;
                            }
                            index -= pop.len();
                        }
                        assert!(found);
                    }
                    9 => h.bin_id = op[1] as u16,
                    10 => HistogramCopy(&saved, &mut h),
                    11 => HistogramCopy(&h, &mut saved),
                    _ => panic!("unknown histogram operation"),
                }
            }
            inspect_histogram(&h, &mut out);
        }
        "VP8LHistogramSet" => {
            let mut set = VP8LAllocateHistogramSet(a[0] as usize, a[1]).unwrap();
            for h in &mut set.histograms {
                dirty_histogram(h);
            }
            set.size = 1;
            VP8LHistogramSetClear(&mut set);
            out.extend([set.size as u32, set.histograms.len() as u32]);
            for h in &set.histograms {
                inspect_histogram(h, &mut out);
            }
        }
        _ => panic!("unknown reference/histogram function"),
    }
    out
}

pub fn foundation_fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for population in [
        vec![0],
        vec![7],
        vec![0, 0, 13, 0],
        vec![1, 99],
        vec![1, 1, 100],
        vec![1, 2, 3, 100],
        vec![1, 2, 3, 4, 100],
        vec![7; 4],
        vec![0; 40],
        vec![0, 0, 0, 0, 5, 5, 5, 5, 0, 0, 0],
    ] {
        cases.push(Case::new("PopulationCost", population));
    }
    for width in [1, 3, 8, 9, 16, 16383] {
        for distance in [1, width, 8 * width + 9] {
            cases.push(Case::new("VP8LDistanceToPlaneCode", vec![width, distance]));
        }
    }
    for input in [
        vec![0],
        vec![
            1, 0, 0xa1234567, 0, 1, 1023, 0, 2, 100, 4095, 4, 0, 0, 7, 16, 0, 8, 0, 0, 3, 0, 0, 5,
            0, 0,
        ],
        vec![
            256, 4, 0, 0, 5, 0, 0, 0, 0xff00ff00, 0, 5, 0, 0, 6, 0, 0, 0, 0x11223344, 0,
        ],
    ] {
        cases.push(Case::new("VP8LBackwardRefs", input));
    }
    for bits in [0, 1, 4, 10, 11] {
        let mut input = vec![bits, 17, 0, 0xa1234567, 0, 2, 17, 4095];
        if bits > 0 {
            input.extend([1, ((1 << bits) - 1).min(1023), 0]);
        }
        input.extend([3, 0, 0, 3, 0, 0, 6, 0, 0]);
        cases.push(Case::new("VP8LHistogram", input));
    }
    cases.push(Case::new(
        "VP8LHistogram",
        vec![
            0, 0, 0, 0xff123456, 0, 3, 0, 0, 6, 0, 0, 9, 77, 0, 11, 0, 0, 5, 0, 0, 7, 0, 0, 10, 0,
            0, 7, 0, 0, 4, 0, 0,
        ],
    ));
    cases.push(Case::new(
        "VP8LHistogram",
        vec![0, 0, 8, 0x34, u32::MAX, 0, 0xff123456, 0, 3, 0, 0],
    ));
    for bits in [0, 2, 11] {
        cases.push(Case::new("VP8LHistogramSet", vec![2, bits]));
    }
    cases
}

pub fn foundation_cases() -> Vec<Case> {
    let mut cases = foundation_fixture_cases();
    // Exhaustive distributions: ties, skew, all 0..=5 nonzero-count branches,
    // and streaks on both sides of the four-entry threshold.
    for len in 1..=7 {
        for mut value in 0..4_u32.pow(len) {
            let mut population = Vec::new();
            for _ in 0..len {
                population.push(value & 3);
                value >>= 2;
            }
            cases.push(Case::new("PopulationCost", population));
        }
    }
    let mut state = SEED;
    for len in [24, 40, 256, 280, 282, 296, 1304, 2328] {
        for pattern in 0..64 {
            let population: Vec<_> = (0..len)
                .map(|i| match pattern % 4 {
                    0 => u32::from(i % (pattern as usize + 1) == 0),
                    1 => random(&mut state) % 1_000_000,
                    2 => {
                        if i < pattern as usize + 1 {
                            random(&mut state) % 1_000_000
                        } else {
                            0
                        }
                    }
                    _ => (i / (pattern as usize + 1)) as u32,
                })
                .collect();
            cases.push(Case::new("PopulationCost", population));
        }
    }
    for width in [1, 2, 3, 7, 8, 9, 15, 16, 17, 31, 64, 256, 16383] {
        for distance in 1..=10 * width.min(64) + 128 {
            cases.push(Case::new("VP8LDistanceToPlaneCode", vec![width, distance]));
        }
        for distance in [
            width * 7 - 1,
            width * 7,
            width * 8 - 1,
            width * 8,
            1 << 20,
            (1 << 20) + 119,
        ] {
            cases.push(Case::new("VP8LDistanceToPlaneCode", vec![width, distance]));
        }
    }
    for block_size in [0, 1, 255, 256, 257, 512] {
        for len in [0, 1, 255, 256, 257, 511, 512, 513, 1025] {
            let mut input = vec![block_size];
            for i in 0..len {
                input.extend(match i % 3 {
                    0 => [0, random(&mut state), 0],
                    1 => [1, i % 1024, 0],
                    _ => [2, i + 1, i % 4095 + 1],
                });
            }
            input.extend([
                8, 0, 0, 4, 0, 0, 3, 0, 0, 8, 0, 0, 0, 0x01020304, 0, 5, 0, 0, 8, 0, 0, 7, 17, 0,
                6, 0, 0, 5, 0, 0,
            ]);
            cases.push(Case::new("VP8LBackwardRefs", input));
        }
    }
    // Random storage trials retain the saved sequence across clear/release.
    for _ in 0..128 {
        let mut input = vec![256];
        for _ in 0..160 {
            let r = random(&mut state);
            input.extend(match r % 12 {
                0..=3 => [0, random(&mut state), 0],
                4 => [1, random(&mut state) % 1024, 0],
                5 => [
                    2,
                    random(&mut state) % LZ77_WINDOW_SIZE + 1,
                    random(&mut state) % 4095 + 1,
                ],
                6 => [3, 0, 0],
                7 => [4, 0, 0],
                8 => [5, 0, 0],
                9 => [6, 0, 0],
                _ => [8, 0, 0],
            });
        }
        cases.push(Case::new("VP8LBackwardRefs", input));
    }
    for bits in 0..=11 {
        for trial in 0..32 {
            let width = if trial % 2 == 0 {
                0
            } else {
                1 + random(&mut state) % 16383
            };
            let mut input = vec![bits, width];
            for _ in 0..1 + trial * 17 {
                let r = random(&mut state);
                input.extend(match r % 3 {
                    1 if bits > 0 => [1, random(&mut state) % (1 << bits.min(10)), 0],
                    2 => [
                        2,
                        random(&mut state) % LZ77_WINDOW_SIZE + 1,
                        random(&mut state) % 4095 + 1,
                    ],
                    _ => [0, random(&mut state), 0],
                });
            }
            input.extend([
                3,
                0,
                0,
                3,
                0,
                0,
                6,
                0,
                0,
                11,
                0,
                0,
                9,
                99,
                0,
                5,
                0,
                0,
                7,
                0,
                0,
                10,
                0,
                0,
                7,
                0,
                0,
                5,
                1,
                0,
                4,
                trial % 2,
                0,
                6,
                0,
                0,
            ]);
            cases.push(Case::new("VP8LHistogram", input));
        }
        // Direct histogram populations reach entropy/streak and extra-bit paths
        // that sparse token streams seldom exercise, without building huge refs.
        for trial in 0..16 {
            let size = VP8LHistogramNumCodes(bits) + 3 * 256 + 40;
            let mut input = vec![bits, 0];
            for i in 0..size {
                let value = match trial % 4 {
                    0 => random(&mut state) % 65537,
                    1 => (i % 7) as u32,
                    2 => u32::from(i % 128 == 0),
                    _ => 27,
                };
                if value != 0 {
                    input.extend([8, i as u32, value]);
                }
            }
            input.extend([6, 0, 0]);
            cases.push(Case::new("VP8LHistogram", input));
        }
        cases.push(Case::new("VP8LHistogramSet", vec![5, bits]));
    }
    cases
}
