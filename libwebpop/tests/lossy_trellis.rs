#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use support::vp8_trellis as trellis;

#[cfg(feature = "native-oracle")]
#[test]
fn trellis_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = trellis::fixture_cases();
    let outputs = native::run("lossy-trellis-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C trellis quantization leaves (methods milestone 5).\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED,
        );
        for (case, out) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for v in &case.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in out {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lossy_trellis.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_trellis.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
    for (case, out) in cases.iter().zip(&outputs) {
        native::compare(case, out, &case.rust());
    }
    eprintln!("{} trellis fixtures match pinned C", cases.len());
}

#[test]
fn committed_trellis_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_trellis.txt"));
    let cases = trellis::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(
            expected,
            &case.rust(),
            "{} {:?}",
            case.function,
            &case.input[..4]
        );
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn trellis_leaves_match_pinned_c() {
    let cases: Vec<_> = trellis::score_cases()
        .into_iter()
        .chain(trellis::block_cases())
        .collect();
    native::parallel_cases("lossy-trellis", &cases, |case, out| {
        native::compare(case, out, &case.rust());
    });
    eprintln!("{} exact trellis leaf requests", cases.len());
}

use libwebpop::{
    dsp::{
        cost::{VP8EncBands, VP8LevelFixedCosts},
        enc::kZigzag,
    },
    enc::{quant_enc::*, vp8i_enc::*},
};
use support::vp8_cost as cost;

fn last_position(input: &[i16; 16], m: &VP8Matrix, first: usize) -> usize {
    let threshold = i64::from(m.q[1]).pow(2) / 4;
    match (first..16)
        .filter(|&n| i64::from(input[kZigzag[n]]).pow(2) > threshold)
        .max()
    {
        Some(n) => (n + 1).min(15),
        None => first,
    }
}

// Walk syntax events instead of using the encoder's precomputed cost tables.
fn level_rate(level: usize, p: &[u8; 11], ctx: usize) -> i128 {
    let mut rate = if ctx > 0 {
        cost::bit_cost(true, p[0])
    } else {
        0
    };
    rate += cost::bit_cost(level != 0, p[1]);
    if level != 0 {
        for (node, bit, fixed) in cost::level_events(level) {
            if fixed.is_none() {
                rate += cost::bit_cost(bit, p[node]);
            }
        }
        // The pin's fixed estimates for levels 9/10 differ from actual bits.
        rate += u32::from(VP8LevelFixedCosts[level]);
    }
    i128::from(rate)
}

// Enumerate whole paths. Numeric masks order choices from latest to earliest,
// matching the first predecessor on ties; earlier EOB positions win terminal ties.
fn enumerate(a: &[u32], coverage: &mut [usize; 5]) -> Option<[i16; 16]> {
    let (enc, m, input, mut output) = trellis::initialized(a);
    let ty = a[0] as usize;
    let first = usize::from(ty == 0);
    let last = last_position(&input, &m, first);
    if last - first > 5 {
        return None;
    }
    coverage[0] += 1;
    let lambda = i128::from(a[2]);
    let ctx0 = a[1] as usize;
    let probas = &enc.proba.coeffs[ty];
    let first_p = probas[VP8EncBands[first]][ctx0][0];
    let mut best_score = lambda * i128::from(cost::bit_cost(false, first_p));
    output[first..].fill(0);
    let mut best = output;
    let mut alive_upper = false;
    for end in first..=last {
        for mask in 0..(1 << (end - first + 1)) {
            let mut levels = output;
            let mut score = if ctx0 == 0 {
                lambda * i128::from(cost::bit_cost(true, first_p))
            } else {
                0
            };
            let mut ctx = ctx0;
            let mut dead = false;
            for n in first..=end {
                let j = kZigzag[n];
                let magnitude = i128::from(input[j]).abs() + i128::from(m.sharpen[j]);
                let scaled = magnitude * i128::from(m.iq[j]);
                let neutral = (scaled / 131072).min(2047);
                let maximum = ((scaled + 65536) / 131072).min(2047);
                let choice = (mask >> (n - first)) & 1;
                let level = neutral + choice as i128;
                if level > maximum {
                    coverage[2] += 1;
                    dead = true;
                    break;
                }
                alive_upper |= choice != 0;
                levels[n] = (if input[j] < 0 { -level } else { level }) as i16;
                let weight = [
                    30i128, 27, 19, 11, 27, 24, 17, 10, 19, 17, 12, 8, 11, 10, 8, 6,
                ][j];
                let error = magnitude - level * i128::from(m.q[j]);
                score += 256 * weight * (error.pow(2) - magnitude.pow(2));
                score += lambda * level_rate(level as usize, &probas[VP8EncBands[n]][ctx], ctx);
                ctx = (level as usize).min(2);
            }
            if dead || levels[end] == 0 {
                continue;
            }
            coverage[1] += 1;
            if end < 15 {
                score += lambda
                    * i128::from(cost::bit_cost(false, probas[VP8EncBands[end + 1]][ctx][0]));
            }
            if score == best_score {
                coverage[3] += 1;
            }
            if score < best_score {
                best_score = score;
                best = levels;
            }
        }
    }
    coverage[4] += usize::from(alive_upper);
    Some(best)
}

#[test]
fn short_graphs_match_exhaustive_path_enumeration() {
    let mut coverage = [0usize; 5]; // blocks, terminal paths, dead paths, ties, upper choices
    for case in trellis::block_cases() {
        let a = &case.input;
        let Some(expected) = enumerate(a, &mut coverage) else {
            continue;
        };
        let (enc, m, mut input, mut output) = trellis::initialized(a);
        let original = input;
        let first = usize::from(a[0] == 0);
        let nz = TrellisQuantizeBlock(
            &enc,
            &mut input,
            &mut output,
            a[1] as usize,
            a[0] as usize,
            &m,
            a[2] as i32,
        );
        assert_eq!(output, expected, "{:?}", &a[..4]);
        assert_eq!(nz, expected[first..].iter().any(|&v| v != 0));
        for n in first..16 {
            assert_eq!(
                input[kZigzag[n]],
                (i32::from(expected[n]) * i32::from(m.q[kZigzag[n]])) as i16
            );
        }
        if first == 1 {
            assert_eq!(input[0], original[0]);
            assert_eq!(output[0], a[trellis::LEVELS] as i16);
        }
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!(
        "independent short graphs: blocks/terminal paths/dead paths/ties/upper-choice blocks = {coverage:?}"
    );
}

#[test]
fn strict_predecessor_terminal_and_skip_ties_preserve_first_winner() {
    for case in trellis::tie_cases() {
        let a = &case.input;
        let first = usize::from(a[0] == 0);
        let (enc, m, mut input, mut output) = trellis::initialized(a);
        let original = input;
        TrellisQuantizeBlock(
            &enc,
            &mut input,
            &mut output,
            a[1] as usize,
            a[0] as usize,
            &m,
            a[2] as i32,
        );
        let values = [
            original[kZigzag[first]],
            original[kZigzag[first + 1]],
            original[kZigzag[first + 2]],
        ];
        let expected = match values {
            [2, 0, 0] => [0, 0, 0], // Equal to skip: preserve skip.
            [2, 8, 0] => [0, 2, 0], // Equal predecessors: preserve lower node.
            [8, 2, 0] => [2, 0, 0], // Equal EOB scores: preserve earlier EOB.
            [8, 8, 2] => [2, 2, 0],
            _ => unreachable!(),
        };
        assert_eq!(&output[first..first + 3], &expected);
        assert!(output[first + 3..].iter().all(|&v| v == 0));
    }
}

#[test]
fn score_arithmetic_and_full_block_invariants() {
    for case in trellis::score_cases() {
        let a = &case.input;
        let expected = i128::from(a[0]) * i128::from(trellis::read_wide(&a[1..]))
            + 256 * i128::from(trellis::read_wide(&a[3..]));
        assert_eq!(i128::from(trellis::read_wide(&case.rust())), expected);
    }
    let mut coverage = [0usize; 9]; // skip, nonzero, negative, saturated, last/first EOB, sharpened zero input/winner, upper winner
    for case in trellis::block_cases() {
        let a = &case.input;
        let (enc, m, mut input, mut output) = trellis::initialized(a);
        let original = input;
        let proba = enc.proba.clone();
        let first = usize::from(a[0] == 0);
        let nz = TrellisQuantizeBlock(
            &enc,
            &mut input,
            &mut output,
            a[1] as usize,
            a[0] as usize,
            &m,
            a[2] as i32,
        );
        assert_eq!(enc.proba, proba);
        assert_eq!(nz, output[first..].iter().any(|&v| v != 0));
        coverage[usize::from(nz)] += 1;
        if let Some(last) = output[first..]
            .iter()
            .rposition(|&v| v != 0)
            .map(|n| n + first)
        {
            coverage[4] += usize::from(last == 15);
            coverage[5] += usize::from(last == first);
            assert!(last <= last_position(&original, &m, first));
        }
        for n in first..16 {
            let j = kZigzag[n];
            assert!(output[n].abs() <= 2047);
            assert_eq!(input[j], (i32::from(output[n]) * i32::from(m.q[j])) as i16);
            if output[n] != 0 {
                assert_eq!(output[n] < 0, original[j] < 0);
            }
            coverage[2] += usize::from(output[n] < 0);
            coverage[3] += usize::from(output[n].abs() == 2047);
            coverage[6] += usize::from(original[j] == 0 && m.sharpen[j] > 0);
            coverage[7] += usize::from(original[j] == 0 && m.sharpen[j] > 0 && output[n] != 0);
            let scaled =
                (i64::from(original[j]).abs() + i64::from(m.sharpen[j])) * i64::from(m.iq[j]);
            coverage[8] += usize::from(i64::from(output[n]).abs() > (scaled / 131072).min(2047));
        }
        if first == 1 {
            assert_eq!(input[0], original[0]);
            assert_eq!(output[0], a[trellis::LEVELS] as i16);
        }
        // Neutral trellis quantization ignores scalar bias and zero thresholds.
        let mut altered = m.clone();
        altered.bias.fill(u32::MAX);
        altered.zthresh.fill(u32::MAX);
        let mut other_input = original;
        let mut other_output = std::array::from_fn(|i| a[trellis::LEVELS + i] as i16);
        let other_nz = TrellisQuantizeBlock(
            &enc,
            &mut other_input,
            &mut other_output,
            a[1] as usize,
            a[0] as usize,
            &altered,
            a[2] as i32,
        );
        assert_eq!((other_nz, other_input, other_output), (nz, input, output));
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!(
        "trellis invariants: skip/nonzero/negative/saturated/last EOB/first EOB/sharpened zero inputs/winners/upper winners = {coverage:?}"
    );
}

#[test]
fn pruning_uses_strict_raw_threshold_and_one_extra_position() {
    for case in trellis::pruning_cases() {
        let a = &case.input;
        let (enc, m, mut input, mut output) = trellis::initialized(a);
        let first = usize::from(a[0] == 0);
        let original = input;
        let position = (first + 2..16)
            .find(|&n| original[kZigzag[n]] != 0)
            .unwrap();
        let magnitude = original[kZigzag[position]].abs();
        let last = last_position(&input, &m, first);
        assert_eq!(
            last,
            if magnitude <= 2 {
                first + 1
            } else {
                (position + 1).min(15)
            }
        );
        TrellisQuantizeBlock(
            &enc,
            &mut input,
            &mut output,
            a[1] as usize,
            a[0] as usize,
            &m,
            a[2] as i32,
        );
        if magnitude <= 2 {
            assert_eq!(output[position], 0); // Sharpening cannot extend pruning.
        } else {
            assert!(output[position] != 0);
        }
    }
}
