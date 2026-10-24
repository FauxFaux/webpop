#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::cost::*,
    enc::{cost_enc::*, frame_enc::*, token_enc::*, tree_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};
use support::{vp8_boolean::Decoder, vp8_cost as cost, vp8_tokens as tokens};

#[test]
fn committed_token_and_fitting_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_tokens.txt"));
    let cases = tokens::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

fn fitted(stats: &[u32]) -> (Vec<u32>, u32, bool) {
    let defaults = cost::probability_words(&VP8CoeffsProba0);
    let updates = cost::probability_words(&VP8CoeffsUpdateProba);
    let mut result = Vec::new();
    let mut size = 0;
    let mut changed = false;
    for ((&packed, &old), &update) in stats.iter().zip(&defaults).zip(&updates) {
        let (total, ones) = (u64::from(packed >> 16), u64::from(packed & 65535));
        let candidate = if total == 0 {
            255
        } else {
            (255 * (total - ones)).div_ceil(total) as u32
        };
        let branch = |p: u32| {
            ones * u64::from(cost::bit_cost(true, p as u8))
                + (total - ones) * u64::from(cost::bit_cost(false, p as u8))
        };
        let old_cost = branch(old) + u64::from(cost::bit_cost(false, update as u8));
        let new_cost = branch(candidate) + u64::from(cost::bit_cost(true, update as u8)) + 2048;
        let use_new = new_cost < old_cost;
        let p = if use_new { candidate } else { old };
        result.push(p);
        changed |= p != old;
        size += cost::bit_cost(use_new, update as u8) + if use_new { 2048 } else { 0 };
    }
    (result, size, changed)
}

#[test]
fn probability_fit_and_reset_match_wide_arithmetic() {
    for case in tokens::scalar_cases() {
        let a = &case.input;
        let output = case.rust();
        let (ones, total) = (u64::from(a[0]), u64::from(a[1]));
        let p = if total == 0 {
            255
        } else {
            (255 * (total - ones)).div_ceil(total) as u32
        };
        let branch = ones * u64::from(cost::bit_cost(true, a[2] as u8))
            + (total - ones) * u64::from(cost::bit_cost(false, a[2] as u8));
        assert_eq!(output, [p, branch as u32]);
    }
    for case in tokens::state_cases() {
        let a = &case.input;
        let output = case.rust();
        let mut p = tokens::initialize(&a[2..1058], &a[1058..]);
        let stats = cost::stats_words(&p);
        let original = cost::probability_words(&p.coeffs);
        p.level_cost = [[[[1234; 68]; 3]; 8]; 4];
        p.dirty = a[1] != 0;
        let (expected, size, dirty) = fitted(&stats);
        let mut expected_dirty = a[1];
        let expected_size = if a[0] <= 1 { size } else { 0 };
        let probs = if a[0] <= 1 { expected } else { original };
        let wanted_stats = if a[0] == 2 { vec![0; 1056] } else { stats };
        if a[0] <= 1 {
            expected_dirty = u32::from(dirty);
            p.coeffs = cost::read_probas(&probs).coeffs;
            p.dirty = dirty;
            if a[0] == 1 {
                VP8CalculateLevelCosts(&mut p);
                expected_dirty = 0;
            }
        } else if a[0] == 3 {
            VP8CalculateLevelCosts(&mut p);
            expected_dirty = 0;
        }
        assert_eq!(
            &output[..8],
            &[
                expected_size,
                expected_dirty,
                if a[0] == 3 { 0 } else { 12345 },
                17,
                1,
                3,
                5,
                7
            ]
        );
        assert_eq!(&output[8..1064], probs);
        assert_eq!(&output[1064..2120], wanted_stats);
        assert_eq!(output[2120], tokens::hash(cost::costs_words(&p)));
    }
    // Slot 33: default probability 253, update probability 176. The exact
    // 781-observation tie keeps the default, even from a nondefault prior fit.
    let defaults = cost::probability_words(&VP8CoeffsProba0);
    for (total, want) in [(780, 253), (781, 253), (782, 255)] {
        let mut stats = vec![0; 1056];
        stats[33] = total << 16;
        let (actual, _, _) = fitted(&stats);
        assert_eq!(actual[33], want);
        let mut p = tokens::initialize(&vec![0; 1056], &stats);
        FinalizeTokenProbas(&mut p);
        assert_eq!(cost::probability_words(&p.coeffs), actual);
        if total == 781 {
            let old = total * cost::bit_cost(false, 253) + cost::bit_cost(false, 176);
            let new = total * cost::bit_cost(false, 255) + cost::bit_cost(true, 176) + 2048;
            assert_eq!(old, new);
            assert_eq!(actual[33], defaults[33]);
        }
    }
}

use tokens::expected_tokens;

fn record(stats: &mut u32, bit: bool) {
    let mut count = u64::from(*stats);
    if count >= 0xfffe0000 {
        count = (((count + 1) % (1u64 << 32)) / 2) & 0x7fff7fff;
    }
    *stats = ((count + 65536 + u64::from(bit)) % (1u64 << 32)) as u32;
}
fn read_stream(output: &[u32], at: &mut usize) -> Vec<u32> {
    let end = *at + 5 + output[*at + 4] as usize;
    let stream = output[*at..end].to_vec();
    *at = end;
    stream
}
fn check_stream(stream: &[u32], expected: &[u16], probas: &[u32], prefix: u32) {
    let sum: u64 = expected
        .iter()
        .map(|&t| {
            let p = if t & 0x4000 != 0 {
                u32::from(t as u8)
            } else {
                probas[usize::from(t & 0x3fff)]
            };
            u64::from(cost::bit_cost(t & 0x8000 != 0, p as u8))
        })
        .sum();
    assert_eq!(u64::from(stream[0]) | (u64::from(stream[1]) << 32), sum);
    let bytes: Vec<_> = stream[5..].iter().map(|&v| v as u8).collect();
    let mut d = Decoder::new(&bytes);
    let before = (0..prefix).fold(0, |v, _| 2 * v + u32::from(d.bit(128)));
    assert_eq!(before, 0x5ad & ((1u32 << prefix) - 1));
    for &t in expected {
        let p = if t & 0x4000 != 0 {
            t as u8
        } else {
            probas[usize::from(t & 0x3fff)] as u8
        };
        assert_eq!(d.bit(p), t & 0x8000 != 0);
    }
    assert_eq!((0..11).fold(0, |v, _| 2 * v + u32::from(d.bit(128))), 0x5ad);
}

#[test]
fn token_order_statistics_fitting_and_emission_match_tree_walk() {
    let mut blocks_checked = 0;
    let mut token_bits = 0;
    let mut changed_streams = 0;
    let mut max_pages = 0;
    for case in tokens::token_cases() {
        let a = &case.input;
        let count = a[4] as usize;
        let period = a[1] as usize;
        let output = case.rust();
        let mut stats = a[1061..tokens::HEADER].to_vec();
        let mut expected = Vec::new();
        let mut traces = Vec::new();
        let mut probs = a[5..1061].to_vec();
        let mut dirty = false;
        for (i, block) in a[tokens::HEADER..].chunks_exact(19).enumerate() {
            if period != 0 && i != 0 && i % period == 0 {
                let (p, size, changed) = fitted(&stats);
                probs = p;
                traces.extend([
                    i as u32,
                    size,
                    u32::from(changed),
                    tokens::hash(probs.iter().copied()),
                ]);
                dirty = false;
            }
            let bits = expected_tokens(block);
            assert_eq!(output[1 + i], u32::from(block[3..].iter().any(|&v| v != 0)));
            for &t in &bits {
                if t & 0x4000 == 0 {
                    record(&mut stats[usize::from(t & 0x3fff)], t & 0x8000 != 0);
                }
            }
            expected.extend(bits);
            blocks_checked += 1;
        }
        if a[2] != 0 {
            let (p, size, changed) = fitted(&stats);
            probs = p;
            dirty = changed;
            traces.extend([
                count as u32,
                size,
                u32::from(changed),
                tokens::hash(probs.iter().copied()),
            ]);
        }
        assert_eq!(output[count + 1] as usize, traces.len() / 4);
        assert_eq!(&output[count + 2..count + 2 + traces.len()], traces);
        let mut at = count + 2 + traces.len();
        let page = a[0].max(8192) as usize;
        let len = expected.len();
        let pages = len.div_ceil(page);
        let left = if len == 0 { 0 } else { pages * page - len };
        assert_eq!(
            &output[at..at + 4],
            &[page as u32, pages as u32, left as u32, len as u32]
        );
        at += 4;
        assert_eq!(
            &output[at..at + len],
            expected.iter().map(|&t| u32::from(t)).collect::<Vec<_>>()
        );
        at += len;
        assert_eq!(&output[at..at + 1056], stats);
        at += 1056;
        assert_eq!(&output[at..at + 1056], probs);
        at += 1056;
        assert_eq!(output[at], u32::from(dirty));
        at += 1;
        let trial = read_stream(&output, &mut at);
        let fitted_trial = read_stream(&output, &mut at);
        let final_stream = read_stream(&output, &mut at);
        assert_eq!(fitted_trial, final_stream);
        changed_streams += usize::from(trial[5..] != fitted_trial[5..]);
        check_stream(&trial, &expected, &a[5..1061], a[3]);
        check_stream(&fitted_trial, &expected, &probs, a[3]);
        assert_eq!(&output[at..at + 2], &[1, 0]);
        at += 2;
        assert_eq!(
            &output[at..at + 5],
            &[page as u32, 1, (page - 1) as u32, 1, 814]
        );
        at += 5;
        record(&mut stats[814], false);
        assert_eq!(&output[at..at + 1056], stats);
        at += 1056;
        let reused = read_stream(&output, &mut at);
        check_stream(&reused, &[814], &a[5..1061], 0);
        assert_eq!(at, output.len());
        token_bits += len;
        max_pages = max_pages.max(pages);
    }
    assert!(changed_streams > 100 && max_pages >= 3);
    eprintln!(
        "independently checked {blocks_checked} blocks, {token_bits} token bits; {changed_streams} late-bound streams changed, maximum {max_pages} pages"
    );
}

#[test]
fn final_emission_releases_pages_and_estimates_actual_category_costs() {
    let mut p = VP8EncProba::default();
    VP8DefaultProbas(&mut p);
    VP8CalculateLevelCosts(&mut p);
    let probabilities = tokens::coefficients(&p);
    for (level, difference) in [(9, 211), (10, 312)] {
        let mut coeffs = [0; 16];
        coeffs[15] = level;
        let mut res = VP8InitResidual(0, 3);
        SetResidualCoeffs_C(&coeffs, &mut res);
        let search_cost = GetResidualCost_C(0, &res, &p);
        let mut b = VP8TBufferInit(1);
        VP8RecordCoeffTokens(0, &res, &mut b, &mut p).unwrap();
        assert_eq!(
            VP8EstimateTokenSize(&b, &probabilities),
            (search_cost + difference) as usize
        );
        let retained: Vec<_> = b.tokens().collect();
        let mut first = VP8BitWriterInit(0).unwrap();
        VP8EmitTokens(&mut b, &mut first, &probabilities, false).unwrap();
        assert_eq!(b.tokens().collect::<Vec<_>>(), retained);
        let mut last = VP8BitWriterInit(0).unwrap();
        VP8EmitTokens(&mut b, &mut last, &probabilities, true).unwrap();
        assert_eq!(
            VP8BitWriterFinish(&mut first).unwrap(),
            VP8BitWriterFinish(&mut last).unwrap()
        );
        assert!(b.is_empty());
        assert_eq!((b.page_count(), b.left(), b.page_size()), (0, 0, 8192));
        VP8RecordCoeffTokens(0, &res, &mut b, &mut p).unwrap();
        assert_eq!(b.tokens().collect::<Vec<_>>(), retained);
        VP8TBufferClear(&mut b);
        VP8TBufferClear(&mut b);
        assert!(b.is_empty());
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn tokens_and_fitting_match_pinned_reference() {
    let cases = tokens::cases();
    native::parallel_cases("vp8-tokens", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!("{} exact token/fitting comparisons", cases.len());
}
#[cfg(feature = "native-oracle")]
#[test]
fn token_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = tokens::fixture_cases();
    let outputs = native::run("vp8-token-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 paginated tokens and probability fitting.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_tokens.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_tokens.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
