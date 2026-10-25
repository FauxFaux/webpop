use super::{Case, SEED, random, vp8_cost as cost};
use libwebpop::internal::{
    dsp::cost::{SetResidualCoeffs_C, VP8EncBands},
    enc::{cost_enc::*, frame_enc::*, token_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};

pub const HEADER: usize = 2117;
pub fn hash(words: impl IntoIterator<Item = u32>) -> u32 {
    words
        .into_iter()
        .fold(2166136261, |h, v| (h ^ v).wrapping_mul(16777619))
}
pub fn coefficients(p: &VP8EncProba) -> Vec<u8> {
    p.coeffs
        .iter()
        .flatten()
        .flatten()
        .flatten()
        .copied()
        .collect()
}
pub fn initialize(probs: &[u32], stats: &[u32]) -> VP8EncProba {
    let mut p = cost::read_probas(probs);
    for (dst, &v) in p.stats.iter_mut().flatten().flatten().flatten().zip(stats) {
        *dst = v;
    }
    p
}
pub fn inspect(b: &VP8TBuffer, out: &mut Vec<u32>) {
    out.extend([
        b.page_size() as u32,
        b.page_count() as u32,
        b.left() as u32,
        b.len() as u32,
    ]);
    out.extend(b.tokens().map(u32::from));
}
pub fn emit(b: &mut VP8TBuffer, probs: &[u8], final_pass: bool, prefix: u32, out: &mut Vec<u32>) {
    let size = VP8EstimateTokenSize(b, probs) as u64;
    let mut bw = VP8BitWriterInit(0).unwrap();
    if prefix != 0 {
        VP8PutBits(&mut bw, 0x5ad, prefix).unwrap();
    }
    VP8EmitTokens(b, &mut bw, probs, final_pass).unwrap();
    let pos = VP8BitWriterPos(&bw);
    VP8PutBits(&mut bw, 0x5ad, 11).unwrap();
    let bytes = VP8BitWriterFinish(&mut bw).unwrap();
    out.extend([
        size as u32,
        (size >> 32) as u32,
        pos as u32,
        (pos >> 32) as u32,
        bytes.len() as u32,
    ]);
    out.extend(bytes.iter().copied().map(u32::from));
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8TokenProba" => vec![
            u32::from(CalcTokenProba(a[0], a[1])),
            BranchCost(a[0], a[1], a[2] as u8) as u32,
        ],
        "VP8ProbaState" => {
            let mut p = initialize(&a[2..1058], &a[1058..]);
            p.dirty = a[1] != 0;
            p.nb_skip = 12345;
            p.skip_proba = 17;
            p.use_skip_proba = true;
            p.segments = [3, 5, 7];
            p.level_cost = [[[[1234; 68]; 3]; 8]; 4];
            let mut size = 0;
            match a[0] {
                0 | 1 => {
                    size = FinalizeTokenProbas(&mut p);
                    if a[0] == 1 {
                        VP8CalculateLevelCosts(&mut p);
                    }
                }
                2 => ResetTokenStats(&mut p),
                3 => ResetStats(&mut p),
                _ => unreachable!(),
            }
            let mut out = vec![
                size as u32,
                u32::from(p.dirty),
                p.nb_skip as u32,
                u32::from(p.skip_proba),
                u32::from(p.use_skip_proba),
            ];
            out.extend(p.segments.map(u32::from));
            out.extend(cost::probability_words(&p.coeffs));
            out.extend(cost::stats_words(&p));
            out.push(hash(cost::costs_words(&p)));
            out
        }
        "VP8Tokens" => {
            let count = a[4] as usize;
            let period = a[1] as usize;
            let mut p = initialize(&a[5..1061], &a[1061..HEADER]);
            let initial = coefficients(&p);
            let mut b = VP8TBufferInit(a[0] as usize);
            let mut out = vec![a[4]];
            let mut traces = Vec::new();
            for (i, block) in a[HEADER..].chunks_exact(19).enumerate() {
                if period != 0 && i != 0 && i % period == 0 {
                    traces.extend([
                        i as u32,
                        FinalizeTokenProbas(&mut p) as u32,
                        u32::from(p.dirty),
                        hash(cost::probability_words(&p.coeffs)),
                    ]);
                    VP8CalculateLevelCosts(&mut p);
                }
                let coeffs = std::array::from_fn(|j| block[3 + j] as i32 as i16);
                let mut res = VP8InitResidual(block[0] as usize, block[1] as usize);
                SetResidualCoeffs_C(&coeffs, &mut res);
                out.push(u32::from(
                    VP8RecordCoeffTokens(block[2] as usize, &res, &mut b, &mut p).unwrap(),
                ));
            }
            if a[2] != 0 {
                traces.extend([
                    count as u32,
                    FinalizeTokenProbas(&mut p) as u32,
                    u32::from(p.dirty),
                    hash(cost::probability_words(&p.coeffs)),
                ]);
            }
            out.push((traces.len() / 4) as u32);
            out.extend(traces);
            inspect(&b, &mut out);
            out.extend(cost::stats_words(&p));
            out.extend(cost::probability_words(&p.coeffs));
            out.push(u32::from(p.dirty));
            let fitted = coefficients(&p);
            emit(&mut b, &initial, false, a[3], &mut out);
            emit(&mut b, &fitted, false, a[3], &mut out);
            emit(&mut b, &fitted, true, a[3], &mut out);
            out.extend([
                u32::from(b.is_empty()),
                VP8EstimateTokenSize(&b, &initial) as u32,
            ]);
            VP8TBufferClear(&mut b);
            VP8TBufferClear(&mut b);
            let mut res = VP8InitResidual(0, 3);
            SetResidualCoeffs_C(&[0; 16], &mut res);
            assert!(!VP8RecordCoeffTokens(2, &res, &mut b, &mut p).unwrap());
            inspect(&b, &mut out);
            out.extend(cost::stats_words(&p));
            emit(&mut b, &initial, true, 0, &mut out);
            out
        }
        _ => unreachable!(),
    }
}
pub fn scalar_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for total in [
        0u32, 1, 2, 3, 7, 31, 255, 256, 257, 4095, 32767, 65533, 65534, 65535,
    ] {
        for nb in [
            0,
            1u32.min(total),
            total / 2,
            total.saturating_sub(1),
            total,
        ] {
            for p in [0, 1, 63, 127, 128, 129, 254, 255] {
                cases.push(Case::new("VP8TokenProba", vec![nb, total, p]));
            }
        }
    }
    let mut seed = SEED;
    for _ in 0..2048 {
        let total = random(&mut seed) & 65535;
        let nb = random(&mut seed) % (total + 1);
        let p = random(&mut seed) & 255;
        cases.push(Case::new("VP8TokenProba", vec![nb, total, p]));
    }
    cases
}
pub fn proba_case(action: u32, dirty: u32, probs: &[u32], stats: &[u32]) -> Case {
    let mut a = vec![action, dirty];
    a.extend(probs);
    a.extend(stats);
    Case::new("VP8ProbaState", a)
}
pub fn state_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let sets = cost::probability_sets();
    let mut seed = SEED;
    for (i, probs) in sets.iter().enumerate() {
        for packed in [0, 1 << 16, 0x10001, 0xfffd0000, 0xfffefffd, 0xffffffff] {
            for action in 0..4 {
                cases.push(proba_case(
                    action,
                    (i % 2) as u32,
                    probs,
                    &vec![packed; 1056],
                ));
            }
        }
        for action in 0..4 {
            let stats: Vec<_> = (0..1056)
                .map(|_| {
                    let total = random(&mut seed) & 65535;
                    let nb = random(&mut seed) % (total + 1);
                    (total << 16) | nb
                })
                .collect();
            cases.push(proba_case(action, (i % 2) as u32, probs, &stats));
        }
    }
    // Each update slot independently, exercising fitting and table reversion
    // from nondefault current probabilities, with no observation at other slots.
    for slot in 0..1056 {
        let mut stats = vec![0; 1056];
        stats[slot] = if slot % 2 == 0 { 65535 << 16 } else { u32::MAX };
        cases.push(proba_case(0, (slot % 2) as u32, &sets[7], &stats));
    }
    for total in [780u32, 781, 782] {
        let mut stats = vec![0; 1056];
        stats[33] = total << 16;
        cases.push(proba_case(0, 1, &sets[7], &stats));
    }
    cases
}
pub fn token_case(
    page: usize,
    period: usize,
    final_fit: bool,
    prefix: u32,
    probs: &[u32],
    stats: &[u32],
    blocks: &[[u32; 19]],
) -> Case {
    let mut a = vec![
        page as u32,
        period as u32,
        u32::from(final_fit),
        prefix,
        blocks.len() as u32,
    ];
    a.extend(probs);
    a.extend(stats);
    a.extend(blocks.iter().flatten());
    Case::new("VP8Tokens", a)
}
pub fn block(first: usize, ty: usize, ctx: usize, coeffs: [i16; 16]) -> [u32; 19] {
    let mut b = [0; 19];
    b[..3].copy_from_slice(&[first as u32, ty as u32, ctx as u32]);
    for (dst, v) in b[3..].iter_mut().zip(coeffs) {
        *dst = v as i32 as u32;
    }
    b
}
pub fn token_cases() -> Vec<Case> {
    let sets = cost::probability_sets();
    let mut cases = Vec::new();
    let mut seed = SEED;
    for first in 0..=1 {
        for ty in 0..4 {
            for ctx in 0..3 {
                for (set, probs) in sets.iter().enumerate() {
                    let mut blocks = vec![block(first, ty, ctx, [0; 16])];
                    for v in [
                        1, -1, 2, -2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 18, 19, 34, 35, 66, 67, 68,
                        2047, -2047,
                    ] {
                        for pos in [first, 7, 15] {
                            let mut c = [0; 16];
                            c[pos] = v;
                            blocks.push(block(first, ty, ctx, c));
                        }
                    }
                    for i in 0..32 {
                        let mut c = [0; 16];
                        for v in &mut c[first..] {
                            let r = random(&mut seed);
                            *v = if r & 3 == 0 {
                                0
                            } else {
                                ((r >> 8) % 4095) as i16 - 2047
                            };
                        }
                        blocks.push(block(first, (ty + i) % 4, (ctx + i) % 3, c));
                    }
                    cases.push(token_case(
                        [0, 8192, 8193, 16384][set % 4],
                        [0, 7, 96][set % 3],
                        set % 2 == 0,
                        [0, 5, 11, 31][set % 4],
                        probs,
                        &vec![[0, 0xfffefffd, 0xffff0000][set % 3]; 1056],
                        &blocks,
                    ));
                }
                // Every magnitude/sign, including all 11-bit category-six residues.
                for start in (0..2048).step_by(128) {
                    let blocks: Vec<_> = (start..start + 128)
                        .flat_map(|v| [v, -v])
                        .map(|v| {
                            let mut c = [0; 16];
                            c[15] = v as i16;
                            block(first, ty, ctx, c)
                        })
                        .collect();
                    cases.push(token_case(
                        8192, 96, true, 11, &sets[0], &[0; 1056], &blocks,
                    ));
                }
            }
        }
    }
    for page in [0, 1, 8191, 8192, 8193, 16384] {
        let effective = page.max(8192);
        for count in [
            0,
            1,
            effective - 1,
            effective,
            effective + 1,
            2 * effective + 1,
        ] {
            if count > 32769 {
                continue;
            }
            let blocks: Vec<_> = (0..count)
                .map(|i| block(i % 2, i % 4, i % 3, [0; 16]))
                .collect();
            cases.push(token_case(page, 96, true, 5, &sets[0], &[0; 1056], &blocks));
        }
    }
    for offset in 0..20 {
        let mut blocks = vec![block(0, 0, 0, [0; 16]); 8192 - offset];
        blocks.push(block(0, 3, 2, [-2047; 16]));
        cases.push(token_case(8192, 0, true, 0, &sets[0], &[0; 1056], &blocks));
    }
    cases
}
pub fn cases() -> Vec<Case> {
    scalar_cases()
        .into_iter()
        .chain(state_cases())
        .chain(token_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut out: Vec<_> = scalar_cases().into_iter().step_by(109).collect();
    let states = state_cases();
    out.extend(states.iter().step_by(157).cloned());
    out.extend(states[states.len() - 3..].iter().cloned());
    for mut c in token_cases().into_iter().step_by(47) {
        let count = c.input[4].min(3);
        c.input[4] = count;
        c.input.truncate(HEADER + 19 * count as usize);
        out.push(c);
    }
    // Retain compact empty-buffer fixtures as well.
    out.push(token_case(
        0,
        0,
        false,
        0,
        &cost::probability_sets()[0],
        &[0; 1056],
        &[],
    ));
    out
}

// Construct expected token bits by walking the syntax, independently of the
// recorder's control flow and without reading any packed-token output.
pub fn expected_tokens(block: &[u32]) -> Vec<u16> {
    let (first, ty, mut ctx) = (block[0] as usize, block[1] as usize, block[2] as usize);
    let coeffs = &block[3..];
    let last = coeffs.iter().rposition(|&v| v != 0);
    let adaptive = |n: usize, context: usize, node: usize, bit: bool| {
        ((u16::from(bit)) << 15) | (11 * (context + 3 * (VP8EncBands[n] + 8 * ty)) + node) as u16
    };
    let mut out = vec![adaptive(first, ctx, 0, last.is_some())];
    if let Some(last) = last {
        for (n, &word) in coeffs.iter().enumerate().take(last + 1).skip(first) {
            let v = (word as i32).unsigned_abs() as usize;
            out.push(adaptive(n, ctx, 1, v != 0));
            if v == 0 {
                ctx = 0;
                continue;
            }
            for (node, bit, fixed) in cost::level_events(v) {
                out.push(if let Some(p) = fixed {
                    let bit = if p == 128 { (word as i32) < 0 } else { bit };
                    (u16::from(bit) << 15) | 0x4000 | u16::from(p)
                } else {
                    adaptive(n, ctx, node, bit)
                });
            }
            ctx = v.min(2);
            if n < 15 {
                out.push(adaptive(n + 1, ctx, 0, n < last));
            }
        }
    }
    out
}
