#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;
use support::vp8_direct as direct;

#[cfg(feature = "native-oracle")]
#[test]
fn direct_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = direct::fixture_cases();
    let outputs = native::run("vp8-direct-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
        check(case, output);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C direct residual/statistics and skip helpers.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for v in &case.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in output {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lossy_direct.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_direct.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}

#[test]
fn committed_direct_fixtures_match_pinned_c() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_direct.txt"));
    let cases = direct::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        check(&case, expected);
        assert_eq!(
            &case.rust(),
            expected,
            "{} {:?}",
            case.function,
            &case.input[..5]
        );
    }
}
#[cfg(feature = "native-oracle")]
#[test]
fn direct_leaves_match_pinned_c() {
    let mut cases = direct::coeff_cases();
    cases.extend(direct::skip_cases());
    cases.extend(direct::residual_cases());
    let observations = native::parallel_cases("vp8-direct", &cases, |case, output| {
        native::compare(case, output, &case.rust());
        check(case, output);
        let mut counts = [0usize; 8];
        if case.function == "VP8DirectCoeffs" {
            counts[0] = case.input[1] as usize;
        }
        if case.function == "VP8DirectResiduals" {
            let w = case.input[0] as usize;
            let mut previous_left_dc = 1;
            for (index, mb) in case.input[1061 + w..]
                .chunks_exact(direct::MB_WORDS)
                .enumerate()
            {
                let state = &output[9 + index * (59 + w)..9 + (index + 1) * (59 + w)];
                counts[match mb[0] {
                    0 => 1,
                    1 => 2,
                    _ => {
                        if mb[1] == 1 {
                            3
                        } else {
                            4
                        }
                    }
                }] += 1;
                if mb[0] == 2 && mb[1] == 0 && state[56 + index % w] & (1 << 24) != 0 {
                    counts[5] += 1;
                }
                if mb[0] == 2 && mb[1] == 1 && previous_left_dc == 1 {
                    counts[6] += 1;
                }
                if index % w == w - 1 {
                    counts[7] += 1;
                }
                previous_left_dc = state[state.len() - 1];
            }
        }
        counts
    });
    let mut totals = [0; 8];
    for counts in observations {
        for (sum, n) in totals.iter_mut().zip(counts) {
            *sum += n;
        }
    }
    assert!(totals.iter().all(|&n| n > 0));
    eprintln!(
        "coefficient blocks, recorded/coded MBs, skipped I16/I4, preserved I4 DC, cleared I16 left DC, row boundaries: {totals:?}"
    );
    eprintln!(
        "{} exact direct coefficient, residual/statistics and skip requests",
        cases.len()
    );
}

use libwebpop::{dsp::cost::VP8EncBands, enc::frame_enc::*, utils::bit_writer_utils::*};
use support::{
    Case,
    vp8_boolean::{self, Decoder},
    vp8_cost as cost, vp8_tokens as tokens,
};
fn wide(v: u64, out: &mut Vec<u32>) {
    out.extend([v as u32, (v >> 32) as u32]);
}
fn read_wide(a: &[u32]) -> u64 {
    u64::from(a[0]) | (u64::from(a[1]) << 32)
}
fn prefix(d: &mut Decoder<'_>, n: u32) {
    for bit in (0..n).rev() {
        assert_eq!(d.bit(128), (0x5adu32 >> bit) & 1 != 0);
    }
}

// Parse the VP8 coefficient tree from the terminated arithmetic stream. This
// walks decoded branches; no expected token list or encoder levels guide it.
fn decode_block(d: &mut Decoder<'_>, block: &[u32], probs: &[u32]) -> [i16; 16] {
    let (first, ty, mut ctx) = (block[0] as usize, block[1] as usize, block[2] as usize);
    let mut out = [0; 16];
    let mut n = first;
    let bit = |d: &mut Decoder<'_>, n: usize, ctx: usize, node: usize| {
        d.bit(probs[((ty * 8 + VP8EncBands[n]) * 3 + ctx) * 11 + node] as u8)
    };
    if !bit(d, n, ctx, 0) {
        return out;
    }
    while n < 16 {
        if !bit(d, n, ctx, 1) {
            ctx = 0;
            n += 1;
            continue;
        }
        let v = if !bit(d, n, ctx, 2) {
            1
        } else if !bit(d, n, ctx, 3) {
            if !bit(d, n, ctx, 4) {
                2
            } else if bit(d, n, ctx, 5) {
                4
            } else {
                3
            }
        } else if !bit(d, n, ctx, 6) {
            if !bit(d, n, ctx, 7) {
                5 + u32::from(d.bit(159))
            } else {
                7 + 2 * u32::from(d.bit(165)) + u32::from(d.bit(145))
            }
        } else {
            let big = bit(d, n, ctx, 8);
            let next = bit(d, n, ctx, if big { 10 } else { 9 });
            let (base, ps): (u32, &[u8]) = match (big, next) {
                (false, false) => (11, &[173, 148, 140]),
                (false, true) => (19, &[176, 155, 140, 135]),
                (true, false) => (35, &[180, 157, 141, 134, 130]),
                (true, true) => (67, &[254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129]),
            };
            base + ps.iter().fold(0, |v, &p| 2 * v + u32::from(d.bit(p)))
        };
        out[n] = if d.bit(128) { -(v as i16) } else { v as i16 };
        ctx = v.min(2) as usize;
        n += 1;
        if n == 16 || !bit(d, n, ctx, 0) {
            break;
        }
    }
    out
}
fn decode_blocks(bytes: &[u32], prefix_bits: u32, probs: &[u32], blocks: &[[u32; 19]]) {
    let bytes: Vec<_> = bytes.iter().map(|&b| b as u8).collect();
    let mut d = Decoder::new(&bytes);
    prefix(&mut d, prefix_bits);
    for b in blocks {
        assert_eq!(
            decode_block(&mut d, b, probs),
            std::array::from_fn(|i| b[3 + i] as i32 as i16)
        );
    }
}
fn event(t: u16, probs: &[u32]) -> (bool, u8) {
    (
        t & 0x8000 != 0,
        if t & 0x4000 != 0 {
            t as u8
        } else {
            probs[usize::from(t & 0x3fff)] as u8
        },
    )
}
fn check_coeffs(case: &Case, out: &[u32]) {
    let a = &case.input;
    let probs = &a[2..direct::COEFF_HEADER];
    let blocks: Vec<[u32; 19]> = a[direct::COEFF_HEADER..]
        .chunks_exact(19)
        .map(|b| b.try_into().unwrap())
        .collect();
    let mut bw = VP8BitWriterInit(0).unwrap();
    if a[0] != 0 {
        VP8PutBits(&mut bw, 0x5ad, a[0]).unwrap();
    }
    let mut state = Vec::new();
    vp8_boolean::inspect(&bw, 0, &mut state);
    assert_eq!(&out[..9], state);
    for (i, b) in blocks.iter().enumerate() {
        for t in tokens::expected_tokens(b) {
            let (bit, p) = event(t, probs);
            VP8PutBit(&mut bw, i32::from(bit), p).unwrap();
        }
        let last = b[3..].iter().rposition(|&v| v != 0);
        assert_eq!(out[9 + 10 * i], last.map_or(u32::MAX, |n| n as u32));
        state.clear();
        vp8_boolean::inspect(&bw, u32::from(last.is_some()), &mut state);
        assert_eq!(&out[10 + 10 * i..19 + 10 * i], state);
    }
    let end = 9 + 10 * blocks.len();
    VP8BitWriterFinish(&mut bw).unwrap();
    state.clear();
    vp8_boolean::inspect(&bw, 0, &mut state);
    assert_eq!(&out[end..end + 9], state);
    assert_eq!(out[end + 9] as usize, out.len() - end - 10);
    assert_eq!(
        out[end + 10..],
        VP8BitWriterBuf(&bw)
            .iter()
            .map(|&v| u32::from(v))
            .collect::<Vec<_>>()
    );
    decode_blocks(&out[end + 10..], a[0], probs, &blocks);
}
fn record(stats: &mut u32, bit: bool) {
    let mut count = u64::from(*stats);
    if count >= 0xfffe0000 {
        count = (((count + 1) % (1u64 << 32)) / 2) & 0x7fff7fff;
    }
    *stats = ((count + 65536 + u64::from(bit)) % (1u64 << 32)) as u32;
}
fn check_residuals(case: &Case, out: &[u32]) {
    let a = &case.input;
    let w = a[0] as usize;
    let probs = &a[4..1060];
    let mut packed = a[1060..1061 + w].to_vec();
    let mut stats = vec![a[3]; 1056];
    let mut bw = VP8BitWriterInit(0).unwrap();
    if a[2] != 0 {
        VP8PutBits(&mut bw, 0x5ad, a[2]).unwrap();
    }
    let mut state = Vec::new();
    vp8_boolean::inspect(&bw, 0, &mut state);
    assert_eq!(&out[..9], state);
    let mut top: Vec<u32> = (0..9).map(|i| i % 2).collect();
    let mut left: Vec<u32> = (0..9).map(|i| (i + 1) % 2).collect();
    let mut bits: [u64; 12] = std::array::from_fn(|i| (1u64 << 40) + i as u64);
    let (mut luma, mut uv) = ((1u64 << 34) + 17, (1u64 << 35) + 19);
    let mut coded = Vec::new();
    for (index, mb) in a[1061 + w..].chunks_exact(direct::MB_WORDS).enumerate() {
        let (x, y) = (index % w, index / w);
        let is16 = mb[1] == 1;
        let op = mb[0];
        let pos1 = VP8BitWriterPos(&bw);
        if op == 2 {
            packed[x + 1] &= if is16 { 0 } else { 1 << 24 };
            if is16 {
                left[8] = 0;
            }
        } else {
            // Build each context from the geometric north/west block, using
            // input levels for interior neighbors and packed borders outside.
            let north = packed[x + 1];
            let west = packed[x];
            let nz: Vec<u32> = (0..24)
                .map(|n| u32::from(mb[19 + 16 * n..35 + 16 * n].iter().any(|&v| v != 0)))
                .collect();
            let dc = if is16 {
                u32::from(mb[3..19].iter().any(|&v| v != 0))
            } else {
                (north >> 24) & 1
            };
            let mut blocks = Vec::new();
            if is16 {
                blocks.push(tokens::block(
                    0,
                    1,
                    ((north >> 24) & 1) as usize + left[8] as usize,
                    std::array::from_fn(|i| mb[3 + i] as i32 as i16),
                ));
                left[8] = dc;
            }
            for n in 0..24 {
                let (base, dim) = if n < 16 {
                    (0, 4)
                } else if n < 20 {
                    (16, 2)
                } else {
                    (20, 2)
                };
                let (bx, by) = ((n - base) % dim, (n - base) / dim);
                let above = if by == 0 {
                    (north >> (base + (dim - 1) * dim + bx)) & 1
                } else {
                    nz[n - dim]
                };
                let before = if bx == 0 {
                    (west >> (base + by * dim + dim - 1)) & 1
                } else {
                    nz[n - 1]
                };
                blocks.push(tokens::block(
                    usize::from(n < 16 && is16),
                    if n >= 16 {
                        2
                    } else if is16 {
                        0
                    } else {
                        3
                    },
                    (above + before) as usize,
                    std::array::from_fn(|i| mb[19 + 16 * n + i] as i32 as i16),
                ));
            }
            let mut pos2 = pos1;
            for (bindex, b) in blocks.iter().enumerate() {
                for t in tokens::expected_tokens(b) {
                    let (bit, p) = event(t, probs);
                    if op == 0 && t & 0x4000 == 0 {
                        record(&mut stats[usize::from(t & 0x3fff)], bit);
                    }
                    if op == 1 {
                        VP8PutBit(&mut bw, i32::from(bit), p).unwrap();
                    }
                }
                if bindex + 1 == 16 + usize::from(is16) {
                    pos2 = VP8BitWriterPos(&bw);
                }
            }
            if op == 1 {
                luma = pos2 - pos1;
                uv = VP8BitWriterPos(&bw) - pos2;
                bits[3 * mb[2] as usize + usize::from(is16)] += luma;
                bits[3 * mb[2] as usize + 2] += uv;
                coded.extend(blocks);
            }
            let mask = nz.iter().enumerate().fold(0, |m, (n, &v)| m | (v << n));
            let boundary = [3, 7, 11, 12, 13, 14, 15, 17, 18, 19, 21, 22, 23]
                .into_iter()
                .fold(0, |m, n| m | (1 << n));
            packed[x + 1] = (mask & boundary) | (dc << 24);
            top = [12, 13, 14, 15, 18, 19, 22, 23]
                .map(|n| (mask >> n) & 1)
                .to_vec();
            top.push(dc);
            let left_dc = left[8];
            left = [3, 7, 11, 15, 17, 19, 21, 23]
                .map(|n| (mask >> n) & 1)
                .to_vec();
            left.push(left_dc);
        }
        state.clear();
        vp8_boolean::inspect(&bw, 0, &mut state);
        wide(luma, &mut state);
        wide(uv, &mut state);
        for v in bits {
            wide(v, &mut state);
        }
        state.extend(&top);
        state.extend(&left);
        state.extend(&packed);
        let nx = (x + 1) % w;
        let ny = y + usize::from(nx == 0);
        if nx == 0 {
            left[8] = 0;
        }
        state.extend([nx as u32, ny as u32, left[8]]);
        assert_eq!(
            &out[9 + index * (59 + w)..9 + (index + 1) * (59 + w)],
            state,
            "MB {index}, op={op}, family={}",
            mb[1]
        );
    }
    let at = 9 + (59 + w) * (a[0] * a[1]) as usize;
    assert_eq!(&out[at..at + 1056], stats);
    let at = at + 1056;
    VP8BitWriterFinish(&mut bw).unwrap();
    state.clear();
    vp8_boolean::inspect(&bw, 0, &mut state);
    assert_eq!(&out[at..at + 9], state);
    assert_eq!(out[at + 9] as usize, out.len() - at - 10);
    assert_eq!(
        out[at + 10..],
        VP8BitWriterBuf(&bw)
            .iter()
            .map(|&v| u32::from(v))
            .collect::<Vec<_>>()
    );
    decode_blocks(&out[at + 10..], a[2], probs, &coded);
}
fn check(case: &Case, out: &[u32]) {
    match case.function.as_str() {
        "VP8DirectCoeffs" => check_coeffs(case, out),
        "VP8DirectResiduals" => check_residuals(case, out),
        "VP8SkipProba" => {
            let a = &case.input;
            let (nb, total) = (
                u128::from(read_wide(&a[1..3])),
                u128::from(read_wide(&a[3..5])),
            );
            let prob = if total == 0 {
                255
            } else {
                ((total - nb) * 255 / total) as u32
            };
            if a[0] == 0 {
                assert_eq!(out, [prob]);
            } else {
                let enabled = prob < 250;
                let size = 256
                    + if enabled {
                        2048 + nb as u32 * cost::bit_cost(true, prob as u8)
                            + (total - nb) as u32 * cost::bit_cost(false, prob as u8)
                    } else {
                        0
                    };
                assert_eq!(out, [size, prob, u32::from(enabled), nb as u32]);
            }
        }
        _ => unreachable!(),
    }
}
#[test]
fn direct_streams_decode_and_raster_statistics_follow_geometric_contexts() {
    for case in direct::coeff_cases()
        .into_iter()
        .chain(direct::residual_cases())
    {
        check(&case, &case.rust());
    }
}
#[test]
fn skip_fitting_uses_floor_and_the_strict_250_cutoff() {
    for total in 0..=512 {
        for nb in 0..=total {
            let case = direct::skip_case(true, nb, total);
            check(&case, &case.rust());
        }
    }
    assert_eq!(direct::skip_case(true, 5, 255).rust(), [256, 250, 0, 5]);
    let below = direct::skip_case(true, 6, 255).rust();
    assert_eq!(&below[1..], &[249, 1, 6]);
    for case in direct::skip_cases() {
        check(&case, &case.rust());
    }
}
#[test]
fn fitting_changes_only_skip_probability_fields() {
    let mut proba = cost::read_probas(&cost::probability_sets()[11]);
    proba.dirty = true;
    proba.stats = [[[[0xfffefffd; 11]; 3]; 8]; 4];
    proba.level_cost = [[[[1234; 68]; 3]; 8]; 4];
    proba.segments = [3, 5, 7];
    for nb in [0, 5, 6, 255] {
        proba.nb_skip = nb;
        let original = proba.clone();
        FinalizeSkipProba(&mut proba, 255);
        let mut expected = original;
        expected.skip_proba = proba.skip_proba;
        expected.use_skip_proba = proba.use_skip_proba;
        assert_eq!(proba, expected);
    }
}

#[test]
fn direct_coefficient_coding_propagates_allocation_failure() {
    use libwebpop::enc::{cost_enc::VP8InitResidual, vp8i_enc::VP8EncProba};
    let mut bw = VP8BitWriterInit(0).unwrap();
    // Force a flush whose pending run cannot fit a Vec allocation on any host.
    // The caller abandons the writer after the error, as for other VP8 writes.
    bw.range = 126;
    bw.nb_bits = 0;
    bw.run = usize::MAX - 1;
    let proba = VP8EncProba::default();
    assert!(PutCoeffs(&mut bw, 0, &VP8InitResidual(0, 0), &proba).is_err());
}
