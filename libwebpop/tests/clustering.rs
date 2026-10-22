mod support;

use libwebpop::enc::{backward_references_enc::*, histogram_enc::*, predictor_enc::*};

#[test]
fn committed_clustering_and_sampling_match_scalar_c() {
    let lines: Vec<_> = include_str!("fixtures/clustering.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .collect();
    let cases = support::clustering::fixture_cases();
    assert_eq!(lines.len(), cases.len());
    for (line, case) in lines.into_iter().zip(cases) {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        assert_eq!(words.next().unwrap(), case.function);
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        assert_eq!(input, case.input);
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(case.rust(), expected, "{}", case.function);
    }
}

#[test]
fn sampling_matches_exhaustive_uniform_square_search() {
    for case in support::clustering::cases()
        .into_iter()
        .filter(|c| c.function == "VP8LOptimizeSampling")
    {
        let a = case.input;
        let (w, h, bits, max_bits) = (a[0] as usize, a[1] as usize, a[2], a[3]);
        let width = w.div_ceil(1 << bits);
        let source = &a[4..];
        // Independently group every original pixel into squares at each
        // candidate resolution, rather than testing adjacent rows first.
        let expected_bits = (bits..=max_bits)
            .rev()
            .find(|&candidate| {
                let square = 1 << (candidate - bits);
                source.iter().enumerate().all(|(i, &v)| {
                    let x = i % width;
                    let y = i / width;
                    v == source[(y / square * square) * width + x / square * square]
                })
            })
            .unwrap();
        let mut actual = source.to_vec();
        assert_eq!(
            VP8LOptimizeSampling(&mut actual, w, h, bits, max_bits),
            expected_bits
        );
        let new_width = w.div_ceil(1 << expected_bits);
        let new_height = h.div_ceil(1 << expected_bits);
        let square = 1 << (expected_bits - bits);
        for (i, &v) in actual[..new_width * new_height].iter().enumerate() {
            assert_eq!(
                v,
                source[(i / new_width * square) * width + i % new_width * square]
            );
        }
        assert_eq!(
            &actual[new_width * new_height..],
            &source[new_width * new_height..]
        );
    }
}

#[test]
fn histogram_build_assigns_cross_row_copies_to_their_start() {
    let refs = support::clustering::refs(&[0, 0xff123456, 1, 2, 1, 21, 0, 0xffabcdef, 1, 2, 2, 2]);
    let mut orig = VP8LAllocateHistogramSet(9, 0).unwrap();
    HistogramBuild(5, 1, &refs, &mut orig);
    assert_eq!(orig.histograms[0].literal[0x34], 1);
    assert_eq!(orig.histograms[0].distance[0], 1);
    assert_eq!(orig.histograms[7].literal[0xcd], 1);
    assert_eq!(orig.histograms[7].distance[1], 1);
    for i in [1, 2, 3, 4, 5, 6, 8] {
        assert!((0..5).all(|p| {
            GetPopulationInfo(&orig.histograms[i], p)
                .iter()
                .all(|&v| v == 0)
        }));
    }
    let mut out = VP8LAllocateHistogramSet(9, 0).unwrap();
    HistogramCopyAndAnalyze(&mut orig, &mut out);
    assert_eq!(out.size, 2);
    assert_eq!(orig.histograms[1].is_used, [0; 5]);
}

#[test]
fn remap_preserves_empty_tiles_and_rebuilds_populations() {
    let mut orig = VP8LAllocateHistogramSet(3, 0).unwrap();
    for (index, color) in [(0, 0xff123456), (2, 0xffabcdef)] {
        for _ in 0..1000 {
            HistogramAddSinglePixOrCopy(
                &mut orig.histograms[index],
                &PixOrCopyCreateLiteral(color),
                None,
                0,
            );
        }
    }
    let mut out = VP8LAllocateHistogramSet(3, 0).unwrap();
    HistogramCopyAndAnalyze(&mut orig, &mut out);
    let mut symbols = [u32::MAX; 3];
    HistogramRemap(&orig, &mut out, &mut symbols);
    assert_eq!(symbols, [0, 0, 1]);
    assert_eq!(out.size, 2);
    assert_eq!(out.histograms[0].literal[0x34], 1000);
    assert_eq!(out.histograms[1].literal[0xcd], 1000);
    for h in &out.histograms[..2] {
        assert_eq!(h.literal.iter().sum::<u32>(), 1000);
        assert_eq!(h.bit_cost, 0); // Remap clears cached costs, as upstream does.
        assert_eq!(h.costs, [0; 5]);
    }
    // Ties choose the first cluster, even when both match equally well.
    HistogramCopy(&orig.histograms[0], &mut out.histograms[0]);
    HistogramCopy(&orig.histograms[0], &mut out.histograms[1]);
    HistogramRemap(&orig, &mut out, &mut symbols);
    assert_eq!(symbols, [0, 0, 0]);
}

#[test]
fn clustered_populations_equal_a_direct_token_partition() {
    for pattern in 0..9 {
        for low in [false, true] {
            let case = support::clustering::clustering_case(33, 17, 1, 4, pattern, 70, low);
            let refs = support::clustering::refs(&case.input[6..]);
            let size = 17 * 9;
            let mut out = VP8LAllocateHistogramSet(size, 4).unwrap();
            let mut tmp = VP8LAllocateHistogram(4).unwrap();
            let mut symbols = vec![u32::MAX; size];
            VP8LGetHistoImageSymbols(
                33,
                17,
                &refs,
                70,
                low,
                1,
                4,
                &mut out,
                &mut tmp,
                &mut symbols,
            )
            .unwrap();
            let mut expected = VP8LAllocateHistogramSet(out.size, 4).unwrap();
            let mut position = 0;
            for token in refs.iter() {
                let tile = (position / 33 / 2) * 17 + position % 33 / 2;
                let group = symbols[tile] as usize;
                assert!(group < out.size);
                HistogramAddSinglePixOrCopy(&mut expected.histograms[group], token, None, 0);
                position += PixOrCopyLength(token) as usize;
            }
            assert_eq!(position, 33 * 17);
            for (a, b) in out.histograms[..out.size].iter().zip(&expected.histograms) {
                for i in 0..5 {
                    assert_eq!(GetPopulationInfo(a, i), GetPopulationInfo(b, i));
                }
            }
        }
    }
}

#[test]
fn combination_thresholds_are_strict_and_saturate() {
    let mut a = VP8LAllocateHistogram(0).unwrap();
    let mut b = VP8LAllocateHistogram(0).unwrap();
    let mut out = VP8LAllocateHistogram(0).unwrap();
    a.literal[1] = 100;
    b.literal[2] = 100;
    ComputeHistogramCost(&mut a);
    ComputeHistogramCost(&mut b);
    let (cost, _) = GetCombinedHistogramEntropy(&a, &b, i64::MAX).unwrap();
    assert!(GetCombinedHistogramEntropy(&a, &b, cost as i64).is_none());
    assert!(GetCombinedHistogramEntropy(&a, &b, cost as i64 + 1).is_some());
    assert!(GetCombinedHistogramEntropy(&a, &b, 0).is_none());
    let before = VP8LAllocateHistogram(0).unwrap();
    assert!(!HistogramAddEval(&a, &b, &mut out, i64::MIN));
    assert_eq!(out, before); // Failed evaluation must leave scratch untouched.
    assert!(HistogramAddEval(&a, &b, &mut out, i64::MAX));
    assert_eq!(out.literal[1..3], [100, 100]);
    assert_eq!(
        HistogramAddThresh(&a, &b, i64::MAX),
        Some(cost as i64 - a.bit_cost as i64)
    );
}
