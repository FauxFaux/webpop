#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::cost::*,
    enc::{cost_enc::*, tree_enc::*, vp8i_enc::*},
};
use support::{vp8_boolean::Decoder, vp8_cost as cost};

#[test]
fn committed_cost_and_probability_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_costs.txt"));
    let cases = cost::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

// Independent, explicit syntax-tree walk. Adaptive events identify a probability
// node; fixed events carry their probability directly. No VP8LevelCodes lookup.
fn level_events(v: usize) -> Vec<(usize, bool, Option<u8>)> {
    let mut events = Vec::new();
    events.push((2, v > 1, None));
    if v > 1 {
        events.push((3, v > 4, None));
        if v <= 4 {
            events.push((4, v != 2, None));
            if v != 2 {
                events.push((5, v == 4, None));
            }
        } else {
            events.push((6, v > 10, None));
            if v <= 10 {
                events.push((7, v > 6, None));
                if v <= 6 {
                    events.push((0, v == 6, Some(159)));
                } else {
                    events.push((0, v >= 9, Some(165)));
                    events.push((0, v % 2 == 0, Some(145)));
                }
            } else {
                let (base, probabilities): (usize, &[u8]) = if v <= 18 {
                    events.extend([(8, false, None), (9, false, None)]);
                    (11, &[173, 148, 140])
                } else if v <= 34 {
                    events.extend([(8, false, None), (9, true, None)]);
                    (19, &[176, 155, 140, 135])
                } else if v <= 66 {
                    events.extend([(8, true, None), (10, false, None)]);
                    (35, &[180, 157, 141, 134, 130])
                } else {
                    events.extend([(8, true, None), (10, true, None)]);
                    (67, &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129])
                };
                for (index, &p) in probabilities.iter().enumerate() {
                    events.push((
                        0,
                        (v - base) & (1 << (probabilities.len() - 1 - index)) != 0,
                        Some(p),
                    ));
                }
            }
        }
    }
    events.push((0, false, Some(128))); // sign cost is independent of sign
    events
}
fn bit_cost(bit: bool, probability: u8) -> u32 {
    u32::from(VP8EntropyCost[if bit { 255 - probability } else { probability } as usize])
}

#[test]
fn fixed_costs_follow_category_bits_and_mode_trees() {
    assert_eq!(VP8LevelFixedCosts[0], 0);
    let mut exceptions = Vec::new();
    for level in 1..=2047 {
        let sum: u32 = level_events(level)
            .into_iter()
            .filter_map(|(_, bit, fixed)| fixed.map(|p| bit_cost(bit, p)))
            .sum();
        if u32::from(VP8LevelFixedCosts[level]) != sum {
            exceptions.push((level, VP8LevelFixedCosts[level], sum));
        }
    }
    // Preserve the pin's two historical estimates; they are not the costs of
    // the category bits actually emitted by PutCoeffs/VP8RecordCoeffTokens.
    assert_eq!(exceptions, [(9, 640, 851), (10, 640, 952)]);
    let mut actual_uv = [0; 4];
    for (mode, uv_cost) in actual_uv.iter_mut().enumerate() {
        let uv = match mode {
            0 => bit_cost(false, 142),
            2 => bit_cost(true, 142) + bit_cost(false, 114),
            3 => bit_cost(true, 142) + bit_cost(true, 114) + bit_cost(false, 183),
            1 => bit_cost(true, 142) + bit_cost(true, 114) + bit_cost(true, 183),
            _ => unreachable!(),
        };
        *uv_cost = uv;
        let group = mode == 1 || mode == 3;
        let i16 = bit_cost(true, 145)
            + bit_cost(group, 156)
            + bit_cost(
                if group { mode == 1 } else { mode == 2 },
                if group { 128 } else { 163 },
            );
        assert_eq!(u32::from(VP8FixedCostsI16[mode]), i16);
    }
    // The pinned UV search estimates also differ from the current coding tree.
    assert_eq!(VP8FixedCostsUV, [302, 984, 439, 642]);
    assert_eq!(actual_uv, [218, 990, 601, 648]);
    let paths: [&[(usize, bool)]; 10] = [
        &[(0, false)],
        &[(0, true), (1, false)],
        &[(0, true), (1, true), (2, false)],
        &[(0, true), (1, true), (2, true), (3, false), (4, false)],
        &[
            (0, true),
            (1, true),
            (2, true),
            (3, false),
            (4, true),
            (5, false),
        ],
        &[
            (0, true),
            (1, true),
            (2, true),
            (3, false),
            (4, true),
            (5, true),
        ],
        &[(0, true), (1, true), (2, true), (3, true), (6, false)],
        &[
            (0, true),
            (1, true),
            (2, true),
            (3, true),
            (6, true),
            (7, false),
        ],
        &[
            (0, true),
            (1, true),
            (2, true),
            (3, true),
            (6, true),
            (7, true),
            (8, false),
        ],
        &[
            (0, true),
            (1, true),
            (2, true),
            (3, true),
            (6, true),
            (7, true),
            (8, true),
        ],
    ];
    for top in 0..10 {
        for left in 0..10 {
            for (mode, path) in paths.iter().enumerate() {
                let sum: u32 = path
                    .iter()
                    .map(|&(node, bit)| bit_cost(bit, kBModesProba[top][left][node]))
                    .sum();
                assert_eq!(u32::from(VP8FixedCostsI4[top][left][mode]), sum);
            }
        }
    }
    assert!(VP8EntropyCost.windows(2).all(|w| w[0] >= w[1]));
    for probability in 0..=255u8 {
        assert_eq!(VP8BitCost(0, probability), VP8BitCost(1, 255 - probability));
    }
}

#[test]
fn level_cache_matches_syntax_paths_and_obeys_dirty_flag() {
    for words in cost::probability_sets() {
        let mut p = cost::read_probas(&words);
        p.dirty = true;
        VP8CalculateLevelCosts(&mut p);
        assert!(!p.dirty);
        for t in 0..4 {
            for b in 0..8 {
                for ctx in 0..3 {
                    let prob = p.coeffs[t][b][ctx];
                    for v in 0..=67 {
                        let mut expected = if ctx > 0 { bit_cost(true, prob[0]) } else { 0 };
                        expected += bit_cost(v != 0, prob[1]);
                        if v != 0 {
                            expected += level_events(v)
                                .into_iter()
                                .filter(|e| e.2.is_none())
                                .map(|(node, bit, _)| bit_cost(bit, prob[node]))
                                .sum::<u32>();
                        }
                        assert_eq!(u32::from(p.level_cost[t][b][ctx][v]), expected);
                    }
                    for n in 0..16 {
                        assert!(std::ptr::eq(
                            p.remapped_costs(t, n, ctx),
                            &p.level_cost[t][VP8EncBands[n]][ctx]
                        ));
                    }
                }
            }
        }
        let old_costs = p.level_cost;
        p.coeffs = [[[[42; 11]; 3]; 8]; 4];
        VP8CalculateLevelCosts(&mut p);
        assert_eq!(p.level_cost, old_costs);
        p.dirty = true;
        VP8CalculateLevelCosts(&mut p);
        assert_ne!(p.level_cost, old_costs);
    }
    let mut p = VP8EncProba::default();
    p.stats[1][2][0][9] = 0xfffefffd;
    p.skip_proba = 17;
    p.nb_skip = 123;
    p.level_cost[2][3][1][4] = 777;
    VP8DefaultProbas(&mut p);
    assert_eq!(
        (
            p.skip_proba,
            p.nb_skip,
            p.stats[1][2][0][9],
            p.level_cost[2][3][1][4]
        ),
        (17, 123, 0xfffefffd, 777)
    );
    assert_eq!(p.segments, [255; 3]);
    assert!(p.dirty && !p.use_skip_proba);
}

fn record_independently(stats: &mut u32, bit: bool) {
    // Split packed counts, with rounded division before the next observation.
    // Full-width malformed scalar states are checked separately below.
    let (mut total, mut ones) = (u64::from(*stats >> 16), u64::from(*stats & 65535));
    if total >= 65534 {
        let rounded_ones = ones + 1;
        total = ((total + rounded_ones / 65536) % 65536) / 2;
        ones = (rounded_ones % 65536) / 2;
    }
    *stats = (((total + 1) * 65536 + ones + u64::from(bit)) % (1u64 << 32)) as u32;
}

#[test]
fn scalar_statistics_rescale_and_wrap_as_unsigned_words() {
    for case in cost::scalar_cases()
        .into_iter()
        .filter(|c| c.function == "VP8RecordStats")
    {
        let mut expected = u64::from(case.input[0]);
        let actual = case.rust();
        for (&bit, record) in case.input[1..].iter().zip(actual.chunks_exact(2)) {
            if expected >= 0xfffe0000 {
                expected = (((expected + 1) % (1u64 << 32)) / 2) & 0x7fff7fff;
            }
            expected = (expected + 65536 + u64::from(bit)) % (1u64 << 32);
            assert_eq!(record, [bit, expected as u32]);
        }
    }
    for bit in [false, true] {
        for total in [0u32, 1, 32767, 65533, 65534, 65535] {
            for ones in [0, total / 2, total] {
                let mut p = (total << 16) | ones;
                let mut expected = p;
                record_independently(&mut expected, bit);
                VP8RecordStats(u32::from(bit), &mut p);
                assert_eq!(p, expected);
            }
        }
    }
}

#[test]
fn residual_costs_and_statistics_match_explicit_tree_walk() {
    let mut blocks_checked = 0;
    for case in cost::residual_cases() {
        let a = &case.input;
        let (first, ty, ctx0) = (a[0] as usize, a[1] as usize, a[2] as usize);
        let probabilities = cost::read_probas(&a[4..1060]).coeffs;
        let mut stats = a[1060..2116].to_vec();
        let actual = case.rust();
        for (index, block) in a[cost::RESIDUAL_HEADER..].chunks_exact(16).enumerate() {
            let last = block.iter().rposition(|&v| v as i32 != 0);
            let mut ctx = ctx0;
            let mut sum = 0u32;
            let mut event = |n: usize, context: usize, node: usize, bit: bool| {
                let band = VP8EncBands[n];
                sum += bit_cost(bit, probabilities[ty][band][context][node]);
                let slot = 11 * (context + 3 * (band + 8 * ty)) + node;
                record_independently(&mut stats[slot], bit);
            };
            event(first, ctx, 0, last.is_some());
            if let Some(last) = last {
                for (n, &word) in block.iter().enumerate().take(last + 1).skip(first) {
                    let v = (word as i32).unsigned_abs() as usize;
                    event(n, ctx, 1, v != 0);
                    if v == 0 {
                        ctx = 0;
                        continue;
                    }
                    let events = level_events(v);
                    for &(node, bit, fixed) in &events {
                        if fixed.is_none() {
                            event(n, ctx, node, bit);
                        }
                    }
                    ctx = v.min(2);
                    if n < 15 {
                        event(n + 1, ctx, 0, n < last);
                    }
                }
            }
            // The mutable event closure owns `sum`; add fixed costs afterward.
            if let Some(last) = last {
                for &word in &block[first..=last] {
                    let v = (word as i32).unsigned_abs() as usize;
                    if v != 0 {
                        sum += if v == 9 || v == 10 {
                            640 // pinned fixed-cost estimates, checked above
                        } else {
                            level_events(v)
                                .iter()
                                .filter_map(|&(_, bit, fixed)| fixed.map(|p| bit_cost(bit, p)))
                                .sum::<u32>()
                        };
                    }
                }
            }
            assert_eq!(
                &actual[3 * index..3 * index + 3],
                &[
                    last.map_or(u32::MAX, |v| v as u32),
                    sum,
                    u32::from(last.is_some())
                ]
            );
            blocks_checked += 1;
        }
        assert_eq!(&actual[3 * a[3] as usize..], stats);
    }
    eprintln!("independently walked {blocks_checked} residual blocks");
}

#[test]
fn probability_headers_decode_updates_skip_and_sentinels() {
    for case in cost::write_cases() {
        let a = &case.input;
        let output = case.rust();
        assert_eq!(output[1] as usize, output.len() - 2);
        let bytes: Vec<_> = output[2..].iter().map(|&v| v as u8).collect();
        let mut d = Decoder::new(&bytes);
        let mut read = |n: u32| (0..n).fold(0, |value, _| 2 * value + u32::from(d.bit(128)));
        assert_eq!(read(a[2]), 0x5ad & ((1u32 << a[2]) - 1));
        let defaults = cost::probability_words(&VP8CoeffsProba0);
        let update_probas = cost::probability_words(&VP8CoeffsUpdateProba);
        for ((&expected, &default), &prob) in a[3..].iter().zip(&defaults).zip(&update_probas) {
            let update = d.bit(prob as u8);
            assert_eq!(update, expected != default);
            if update {
                let value = (0..8).fold(0, |v, _| 2 * v + u32::from(d.bit(128)));
                assert_eq!(value, expected);
            }
        }
        let skip = d.bit(128);
        assert_eq!(skip, a[0] != 0);
        if skip {
            assert_eq!((0..8).fold(0, |v, _| 2 * v + u32::from(d.bit(128))), a[1]);
        }
        assert_eq!((0..11).fold(0, |v, _| 2 * v + u32::from(d.bit(128))), 0x5ad);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn costs_and_probabilities_match_pinned_reference() {
    let cases = cost::cases();
    native::parallel_cases("vp8-costs", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} exact cost/probability comparisons", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn cost_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = cost::fixture_cases();
    let outputs = native::run("vp8-cost-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 costs, coefficient statistics and probability headers.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for word in &case.input {
                write!(text, " {word:08x}").unwrap();
            }
            text.push_str(" |");
            for word in output {
                write!(text, " {word:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lossy_costs.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_costs.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
