use super::{Case, SEED, random};
use libwebpop::{
    dsp::cost::*,
    enc::{cost_enc::*, tree_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};

pub const PROBA_WORDS: usize = 1056;
pub const RESIDUAL_HEADER: usize = 4 + 2 * PROBA_WORDS;

pub fn probability_words(coeffs: &[[ProbaArray; 8]; 4]) -> Vec<u32> {
    coeffs
        .iter()
        .flatten()
        .flatten()
        .flatten()
        .map(|&v| u32::from(v))
        .collect()
}
pub fn read_probas(words: &[u32]) -> VP8EncProba {
    let mut proba = VP8EncProba::default();
    for (dst, &src) in proba
        .coeffs
        .iter_mut()
        .flatten()
        .flatten()
        .flatten()
        .zip(words)
    {
        *dst = src as u8;
    }
    proba
}
pub fn stats_words(proba: &VP8EncProba) -> Vec<u32> {
    proba
        .stats
        .iter()
        .flatten()
        .flatten()
        .flatten()
        .copied()
        .collect()
}
pub fn costs_words(proba: &VP8EncProba) -> Vec<u32> {
    proba
        .level_cost
        .iter()
        .flatten()
        .flatten()
        .flatten()
        .map(|&v| u32::from(v))
        .collect()
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8BitCost" => vec![VP8BitCost(a[0] as i32, a[1] as u8) as u32],
        "VP8RecordStats" => {
            let mut stats = a[0];
            let mut out = Vec::new();
            for &bit in &a[1..] {
                out.push(VP8RecordStats(bit, &mut stats));
                out.push(stats);
            }
            out
        }
        "VP8CostTables" => {
            let mut out: Vec<_> = VP8EntropyCost
                .iter()
                .chain(&VP8LevelFixedCosts)
                .copied()
                .map(u32::from)
                .collect();
            out.extend(VP8EncBands.map(|v| v as u32));
            out.extend(VP8FixedCostsUV.map(u32::from));
            out.extend(VP8FixedCostsI16.map(u32::from));
            out.extend(
                VP8FixedCostsI4
                    .iter()
                    .flatten()
                    .flatten()
                    .copied()
                    .map(u32::from),
            );
            out.extend(probability_words(&VP8CoeffsProba0));
            out.extend(probability_words(&VP8CoeffsUpdateProba));
            out
        }
        "VP8DefaultProbas" => {
            let mut p = VP8EncProba {
                skip_proba: 73,
                nb_skip: 12345,
                use_skip_proba: true,
                ..VP8EncProba::default()
            };
            p.stats = [[[[0xfffefffd; 11]; 3]; 8]; 4];
            p.level_cost = [[[[1234; 68]; 3]; 8]; 4];
            VP8DefaultProbas(&mut p);
            let mut out = vec![
                u32::from(p.dirty),
                u32::from(p.use_skip_proba),
                u32::from(p.skip_proba),
                p.nb_skip as u32,
            ];
            out.extend(p.segments.map(u32::from));
            out.extend(probability_words(&p.coeffs));
            out.extend(stats_words(&p));
            out.extend(costs_words(&p));
            out
        }
        "VP8LevelCosts" => {
            let mut p = read_probas(&a[1..]);
            p.dirty = a[0] != 0;
            p.level_cost = [[[[1234; 68]; 3]; 8]; 4];
            VP8CalculateLevelCosts(&mut p);
            let mut out = vec![u32::from(p.dirty)];
            out.extend(costs_words(&p));
            if a[0] != 0 {
                for t in 0..4 {
                    for n in 0..16 {
                        for c in 0..3 {
                            let table = p.remapped_costs(t, n, c);
                            let band = (0..8)
                                .find(|&b| std::ptr::eq(table, &p.level_cost[t][b][c]))
                                .unwrap();
                            out.push(band as u32);
                        }
                    }
                }
            }
            out
        }
        "VP8Residual" => {
            let mut p = read_probas(&a[4..1060]);
            for (dst, &src) in p
                .stats
                .iter_mut()
                .flatten()
                .flatten()
                .flatten()
                .zip(&a[1060..2116])
            {
                *dst = src;
            }
            p.dirty = true;
            VP8CalculateLevelCosts(&mut p);
            let mut out = Vec::new();
            for block in a[RESIDUAL_HEADER..].chunks_exact(16) {
                let coeffs: [i16; 16] = std::array::from_fn(|i| block[i] as i32 as i16);
                let mut res = VP8InitResidual(a[0] as usize, a[1] as usize);
                SetResidualCoeffs_C(&coeffs, &mut res);
                out.push(res.last as u32);
                out.push(GetResidualCost_C(a[2] as usize, &res, &p) as u32);
                out.push(u32::from(VP8RecordCoeffs(a[2] as usize, &res, &mut p)));
            }
            out.extend(stats_words(&p));
            out
        }
        "VP8WriteProbas" => {
            let mut p = read_probas(&a[3..]);
            p.use_skip_proba = a[0] != 0;
            p.skip_proba = a[1] as u8;
            let mut bw = VP8BitWriterInit(0).unwrap();
            if a[2] != 0 {
                VP8PutBits(&mut bw, 0x5ad, a[2]).unwrap();
            }
            VP8WriteProbas(&mut bw, &p).unwrap();
            let mut out = vec![VP8BitWriterPos(&bw) as u32];
            VP8PutBits(&mut bw, 0x5ad, 11).unwrap();
            let bytes = VP8BitWriterFinish(&mut bw).unwrap();
            out.push(bytes.len() as u32);
            out.extend(bytes.iter().copied().map(u32::from));
            out
        }
        _ => unreachable!(),
    }
}

pub fn probability_sets() -> Vec<Vec<u32>> {
    let mut sets = vec![probability_words(&VP8CoeffsProba0)];
    for prob in [0, 1, 2, 63, 127, 128, 129, 192, 254, 255] {
        sets.push(vec![prob; 1056]);
    }
    let mut seed = SEED;
    for _ in 0..8 {
        sets.push((0..1056).map(|_| random(&mut seed) & 255).collect());
    }
    sets
}
pub fn scalar_cases() -> Vec<Case> {
    let mut out = vec![
        Case::new("VP8CostTables", vec![]),
        Case::new("VP8DefaultProbas", vec![]),
    ];
    for prob in 0..=255 {
        for bit in [0, 1, 2, u32::MAX, i32::MIN as u32] {
            out.push(Case::new("VP8BitCost", vec![bit, prob]));
        }
    }
    let mut seed = SEED;
    for initial in [
        0, 1, 0x10000, 0x10001, 0xfffd0000, 0xfffdfffd, 0xfffdffff, 0xfffe0000, 0xfffefffd,
        0xfffefffe, 0xffff0000, 0xfffffffe, 0xffffffff,
    ] {
        for pattern in 0..4 {
            let mut a = vec![initial];
            a.extend((0..257).map(|i| match pattern {
                0 => 0,
                1 => 1,
                2 => i % 2,
                _ => random(&mut seed) & 1,
            }));
            out.push(Case::new("VP8RecordStats", a));
        }
    }
    let mut a = vec![0];
    a.extend((0..131072).map(|i| i % 2));
    out.push(Case::new("VP8RecordStats", a));
    for words in probability_sets() {
        for dirty in [0, 1] {
            let mut a = vec![dirty];
            a.extend(&words);
            out.push(Case::new("VP8LevelCosts", a));
        }
    }
    out
}
pub fn residual_case(
    first: usize,
    ty: usize,
    ctx: usize,
    probabilities: &[u32],
    stats: &[u32],
    blocks: &[[i16; 16]],
) -> Case {
    let mut a = vec![first as u32, ty as u32, ctx as u32, blocks.len() as u32];
    a.extend(probabilities);
    a.extend(stats);
    a.extend(blocks.iter().flatten().map(|&v| v as i32 as u32));
    Case::new("VP8Residual", a)
}
pub fn residual_cases() -> Vec<Case> {
    let sets = probability_sets();
    let mut cases = Vec::new();
    let mut seed = SEED;
    for first in 0..=1 {
        for ty in 0..4 {
            for ctx in 0..3 {
                // Every magnitude and sign at the final coefficient exercises all fixed
                // categories and the absence of a trailing EOB, batched per request.
                for start in (0..2048).step_by(128) {
                    let blocks: Vec<_> = (start..start + 128)
                        .flat_map(|v| [v, -v])
                        .map(|v| {
                            let mut b = [0; 16];
                            b[15] = v as i16;
                            b
                        })
                        .collect();
                    cases.push(residual_case(first, ty, ctx, &sets[0], &[0; 1056], &blocks));
                }
                for (index, probabilities) in sets.iter().enumerate() {
                    let mut blocks = vec![[0; 16]];
                    for pos in first..16 {
                        for v in [
                            1, -1, 2, -2, 4, 5, 6, 7, 10, 11, 18, 19, 34, 35, 66, 67, 68, 2047,
                            -2047,
                        ] {
                            let mut b = [0; 16];
                            b[pos] = v;
                            blocks.push(b);
                        }
                    }
                    for _ in 0..32 {
                        let mut b = [0; 16];
                        for v in &mut b[first..] {
                            let r = random(&mut seed);
                            *v = if r & 3 == 0 {
                                0
                            } else {
                                ((r >> 8) % 4095) as i16 - 2047
                            };
                        }
                        blocks.push(b);
                    }
                    let stats = [0, 0x10001, 0xfffdfffc, 0xfffefffd, 0xffff0000][index % 5];
                    cases.push(residual_case(
                        first,
                        ty,
                        ctx,
                        probabilities,
                        &vec![stats; 1056],
                        &blocks,
                    ));
                }
            }
        }
    }
    cases
}
pub fn write_cases() -> Vec<Case> {
    let mut sets = probability_sets();
    let defaults = probability_words(&VP8CoeffsProba0);
    // Every individual update slot, plus dense/all-equal and sparse mixtures.
    for index in 0..1056 {
        let mut p = defaults.clone();
        p[index] ^= 255;
        sets.push(p);
    }
    let mut cases = Vec::new();
    for (index, probabilities) in sets.into_iter().enumerate() {
        for skip in [0, 1] {
            let mut a = vec![
                skip,
                [0, 1, 128, 254, 255][index % 5],
                [0, 1, 5, 11, 31][index % 5],
            ];
            a.extend(&probabilities);
            cases.push(Case::new("VP8WriteProbas", a));
        }
    }
    cases
}
pub fn cases() -> Vec<Case> {
    scalar_cases()
        .into_iter()
        .chain(residual_cases())
        .chain(write_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut scalar: Vec<_> = scalar_cases()
        .into_iter()
        .filter(|c| {
            c.function != "VP8BitCost" && !(c.function == "VP8RecordStats" && c.input.len() > 258)
        })
        .collect();
    scalar.retain(|c| c.function != "VP8LevelCosts");
    let mut fixtures: Vec<_> = scalar
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i < 2 || i % 11 == 0)
        .map(|(_, c)| c)
        .collect();
    fixtures.extend(
        scalar_cases()
            .into_iter()
            .filter(|c| c.function == "VP8LevelCosts")
            .step_by(11),
    );
    // Compact residual requests still include first/type/context, signed category
    // boundaries, zero runs and dirty statistics; native sweeps retain all blocks.
    for (index, mut c) in residual_cases().into_iter().step_by(37).enumerate() {
        c.input[3] = 3;
        c.input.truncate(RESIDUAL_HEADER);
        for pos in [c.input[0] as usize, 7, 15] {
            let mut b = [0i16; 16];
            b[pos] = [0, -1, 2, -6, 10, -11, 19, -35, 67, -2047][index % 10];
            c.input.extend(b.map(|v| v as i32 as u32));
        }
        fixtures.push(c);
    }
    fixtures.extend(write_cases().into_iter().step_by(101));
    fixtures
}
