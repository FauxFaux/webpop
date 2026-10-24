use super::{Case, SEED, random, vp8_cost as cost, vp8_tokens as tokens};

pub const COEFF_HEADER: usize = 1058;
pub const MB_WORDS: usize = 403;
pub fn coeff_case(prefix: u32, probs: &[u32], blocks: &[[u32; 19]]) -> Case {
    let mut a = vec![prefix, blocks.len() as u32];
    a.extend(probs);
    a.extend(blocks.iter().flatten());
    Case::new("VP8DirectCoeffs", a)
}
pub fn skip_case(finalize: bool, nb: u64, total: u64) -> Case {
    Case::new(
        "VP8SkipProba",
        vec![
            u32::from(finalize),
            nb as u32,
            (nb >> 32) as u32,
            total as u32,
            (total >> 32) as u32,
        ],
    )
}
pub fn coeff_cases() -> Vec<Case> {
    cost::residual_cases()
        .into_iter()
        .enumerate()
        .map(|(i, old)| {
            let a = &old.input;
            let blocks: Vec<_> = a[cost::RESIDUAL_HEADER..]
                .chunks_exact(16)
                .map(|b| {
                    tokens::block(
                        a[0] as usize,
                        a[1] as usize,
                        a[2] as usize,
                        std::array::from_fn(|k| b[k] as i32 as i16),
                    )
                })
                .collect();
            coeff_case([0, 5, 11, 31][i % 4], &a[4..1060], &blocks)
        })
        .collect()
}
pub fn skip_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for total in [
        0u64, 1, 2, 17, 49, 50, 51, 199, 200, 201, 204, 255, 256, 257, 1000, 1024, 1048576,
    ] {
        let values: Vec<_> = if total <= 257 {
            (0..=total).collect()
        } else {
            vec![
                0,
                1,
                total / 100,
                total / 50,
                total / 40,
                total / 2,
                total - 1,
                total,
            ]
        };
        for nb in values {
            for finalize in [false, true] {
                cases.push(skip_case(finalize, nb, total));
            }
        }
    }
    for total in [1u64 << 32, 1u64 << 48, u64::MAX / 255] {
        for nb in [0, 1, total / 2, total - 1, total] {
            cases.push(skip_case(false, nb, total));
        }
    }
    cases
}
pub fn residual_case(
    w: u32,
    h: u32,
    actions: u32,
    family: u32,
    pattern: u32,
    probs: &[u32],
) -> Case {
    let mut seed = SEED ^ pattern ^ actions;
    let mut a = vec![
        w,
        h,
        [0, 5, 11, 31][pattern as usize % 4],
        [0, 0xfffdfffc, 0xfffefffd, 0xffff0000, u32::MAX][pattern as usize % 5],
    ];
    a.extend(probs);
    for col in 0..=w {
        a.push(match (pattern + col) % 6 {
            0 => 0,
            1 => 1 << 24,
            2 => 0xffffff,
            3 => 0x1ffffff,
            4 => u32::MAX,
            _ => random(&mut seed),
        });
    }
    for n in 0..w * h {
        let ty = if family < 2 { family } else { n % 2 };
        let action = match actions {
            0..=2 => actions,
            3 => {
                if n % 3 == 1 {
                    2
                } else {
                    1
                }
            }
            _ => n % 3,
        };
        a.extend([action, ty, n % 4]);
        for block in 0..25 {
            for k in 0..16 {
                let value: i32 = match pattern {
                    0 => 0,
                    1 => {
                        if k == 0 {
                            1
                        } else {
                            0
                        }
                    }
                    2 => {
                        if k == 15 {
                            [
                                -2047, -67, -35, -19, -11, -7, -5, -3, -2, -1, 1, 2, 3, 4, 5, 6, 7,
                                8, 9, 10, 11, 18, 19, 34, 35,
                            ][block as usize]
                        } else {
                            0
                        }
                    }
                    _ => {
                        let v = random(&mut seed);
                        if v & 3 == 0 {
                            0
                        } else {
                            ((v >> 8) % 4095) as i32 - 2047
                        }
                    }
                };
                let v = if ty == 1 && (1..17).contains(&block) && k == 0 {
                    0
                } else {
                    value
                };
                a.push(v as u32);
            }
        }
    }
    Case::new("VP8DirectResiduals", a)
}
pub fn residual_cases() -> Vec<Case> {
    let sets = cost::probability_sets();
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (2, 1), (1, 2), (2, 2), (3, 3)] {
        for actions in 0..5 {
            for family in 0..3 {
                for pattern in 0..5 {
                    cases.push(residual_case(
                        w,
                        h,
                        actions,
                        family,
                        pattern,
                        &sets[(actions + family + pattern) as usize % sets.len()],
                    ));
                }
            }
        }
    }
    for probs in &sets {
        cases.push(residual_case(2, 2, 4, 2, 4, probs));
    }
    // Long cumulative bit counts and repeated statistics rescaling.
    cases.push(residual_case(16, 8, 0, 2, 4, &sets[0]));
    cases.push(residual_case(16, 8, 1, 2, 4, &sets[0]));
    cases
}
pub fn fixture_cases() -> Vec<Case> {
    let sets = cost::probability_sets();
    let mut cases = Vec::new();
    for first in 0..2 {
        for ty in 0..4 {
            for set in [0, 11] {
                let mut blocks = vec![tokens::block(first, ty, ty % 3, [0; 16])];
                for v in [
                    1, -1, 2, -2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 18, 19, 34, 35, 66, 67, 2047, -2047,
                ] {
                    let mut coeffs = [0; 16];
                    coeffs[first] = v;
                    blocks.push(tokens::block(first, ty, ty % 3, coeffs));
                    let mut coeffs = [0; 16];
                    coeffs[15] = v;
                    blocks.push(tokens::block(first, ty, ty % 3, coeffs));
                }
                cases.push(coeff_case(11, &sets[set], &blocks));
            }
        }
    }
    for nb in [0, 1, 4, 5, 6, 7, 254, 255] {
        cases.push(skip_case(true, nb, 255));
    }
    for (nb, total) in [
        (0, 0),
        (0, 1),
        (1, 1),
        (1, 1u64 << 32),
        (1u64 << 47, 1u64 << 48),
    ] {
        cases.push(skip_case(false, nb, total));
    }
    for action in 0..5 {
        cases.push(residual_case(2, 2, action, 2, 4, &sets[0]));
    }
    for family in 0..2 {
        cases.push(residual_case(2, 2, 3, family, 1, &sets[11]));
    }
    cases.push(residual_case(1, 2, 3, 2, 0, &sets[0]));
    cases
}

use libwebpop::{
    dsp::cost::SetResidualCoeffs_C,
    enc::{cost_enc::*, frame_enc::*, iterator_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};
fn wide(v: u64, out: &mut Vec<u32>) {
    out.extend([v as u32, (v >> 32) as u32]);
}
fn finish(bw: &mut VP8BitWriter, out: &mut Vec<u32>) {
    VP8BitWriterFinish(bw).unwrap();
    super::vp8_boolean::inspect(bw, 0, out);
    out.push(VP8BitWriterSize(bw) as u32);
    out.extend(VP8BitWriterBuf(bw).iter().map(|&v| u32::from(v)));
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8SkipProba" => {
            let nb = u64::from(a[1]) | (u64::from(a[2]) << 32);
            let total = u64::from(a[3]) | (u64::from(a[4]) << 32);
            if a[0] == 0 {
                vec![u32::from(CalcSkipProba(nb, total))]
            } else {
                let mut p = VP8EncProba {
                    nb_skip: nb as i32,
                    skip_proba: 17,
                    use_skip_proba: true,
                    ..Default::default()
                };
                vec![
                    FinalizeSkipProba(&mut p, total as usize) as u32,
                    u32::from(p.skip_proba),
                    u32::from(p.use_skip_proba),
                    p.nb_skip as u32,
                ]
            }
        }
        "VP8DirectCoeffs" => {
            let p = cost::read_probas(&a[2..COEFF_HEADER]);
            let mut bw = VP8BitWriterInit(0).unwrap();
            if a[0] != 0 {
                VP8PutBits(&mut bw, 0x5ad, a[0]).unwrap();
            }
            let mut out = Vec::new();
            super::vp8_boolean::inspect(&bw, 0, &mut out);
            for b in a[COEFF_HEADER..].chunks_exact(19) {
                let coeffs = std::array::from_fn(|i| b[3 + i] as i32 as i16);
                let mut res = VP8InitResidual(b[0] as usize, b[1] as usize);
                SetResidualCoeffs_C(&coeffs, &mut res);
                let result = PutCoeffs(&mut bw, b[2] as usize, &res, &p).unwrap();
                out.push(res.last as u32);
                super::vp8_boolean::inspect(&bw, u32::from(result), &mut out);
            }
            finish(&mut bw, &mut out);
            out
        }
        "VP8DirectResiduals" => {
            let (w, h) = (a[0] as usize, a[1] as usize);
            let mut enc = VP8Encoder::new((16 * w) as u32, (16 * h) as u32, false).unwrap();
            enc.proba = tokens::initialize(&a[4..1060], &vec![a[3]; 1056]);
            let mut it = VP8EncIterator::default();
            VP8IteratorInit(&mut enc, &mut it);
            enc.nz.copy_from_slice(&a[1060..1061 + w]);
            it.top_nz = std::array::from_fn(|i| (i % 2) as u8);
            it.left_nz = std::array::from_fn(|i| ((i + 1) % 2) as u8);
            it.luma_bits = (1u64 << 34) + 17;
            it.uv_bits = (1u64 << 35) + 19;
            it.bit_count =
                std::array::from_fn(|i| std::array::from_fn(|j| (1u64 << 40) + (3 * i + j) as u64));
            let mut bw = VP8BitWriterInit(0).unwrap();
            if a[2] != 0 {
                VP8PutBits(&mut bw, 0x5ad, a[2]).unwrap();
            }
            let mut out = Vec::new();
            super::vp8_boolean::inspect(&bw, 0, &mut out);
            for mb in a[1061 + w..].chunks_exact(MB_WORDS) {
                let current = &mut enc.mb_info[it.y * w + it.x];
                current.r#type = mb[1] as u8;
                current.segment = mb[2] as u8;
                let rd = score(mb);
                match mb[0] {
                    0 => RecordResiduals(&mut it, &mut enc, &rd),
                    1 => CodeResiduals(&mut bw, &mut it, &mut enc, &rd).unwrap(),
                    2 => ResetAfterSkip(&mut it, &mut enc),
                    _ => unreachable!(),
                }
                super::vp8_boolean::inspect(&bw, 0, &mut out);
                wide(it.luma_bits, &mut out);
                wide(it.uv_bits, &mut out);
                for &v in it.bit_count.as_flattened() {
                    wide(v, &mut out);
                }
                out.extend(it.top_nz.map(u32::from));
                out.extend(it.left_nz.map(u32::from));
                out.extend(&enc.nz);
                VP8IteratorNext(&mut it, &enc);
                out.extend([it.x as u32, it.y as u32, u32::from(it.left_nz[8])]);
            }
            out.extend(cost::stats_words(&enc.proba));
            finish(&mut bw, &mut out);
            out
        }
        _ => unreachable!(),
    }
}
pub fn score(mb: &[u32]) -> VP8ModeScore {
    VP8ModeScore {
        y_dc_levels: std::array::from_fn(|i| mb[3 + i] as i32 as i16),
        y_ac_levels: std::array::from_fn(|n| {
            std::array::from_fn(|i| mb[19 + 16 * n + i] as i32 as i16)
        }),
        uv_levels: std::array::from_fn(|n| {
            std::array::from_fn(|i| mb[275 + 16 * n + i] as i32 as i16)
        }),
        ..Default::default()
    }
}
