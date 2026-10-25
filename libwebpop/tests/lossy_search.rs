#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::{
    dsp::enc::*,
    enc::{cost_enc::*, iterator_enc::*, quant_enc::*, vp8i_enc::*},
};
use support::vp8_search as search;

#[test]
fn committed_search_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_search.txt"));
    let cases = search::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn scores_flatness_and_mode_contexts_have_independent_definitions() {
    for case in search::score_cases() {
        let a = &case.input;
        let (mut expected, src) = search::scores(a);
        match a[0] {
            0 => {
                expected.D = 0;
                expected.SD = 0;
                expected.H = 0;
                expected.R = 0;
                expected.nz = 0;
                expected.score = 0x7fffffffffffff;
            }
            1 => {
                expected.D = src.D;
                expected.SD = src.SD;
                expected.H = src.H;
                expected.R = src.R;
                expected.nz = src.nz;
                expected.score = src.score;
            }
            2 => {
                expected.D += src.D;
                expected.SD += src.SD;
                expected.H += src.H;
                expected.R += src.R;
                expected.nz |= src.nz;
                expected.score += src.score;
            }
            3 => {
                expected.score = (((i128::from(expected.R) + i128::from(expected.H))
                    * i128::from(a[1]))
                    + 256 * (i128::from(expected.D) + i128::from(expected.SD)))
                    as i64;
            }
            _ => unreachable!(),
        }
        let mut words = Vec::new();
        support::vp8_reconstruct::score(&expected, &mut words);
        assert_eq!(case.rust(), words);
    }
    for case in search::flat_cases() {
        let a = &case.input;
        let pop = a[514..]
            .chunks_exact(16)
            .flat_map(|b| &b[1..])
            .filter(|&&v| v != 0)
            .count();
        let pixels: Vec<_> = a[2..514]
            .chunks_exact(BPS)
            .flat_map(|row| &row[..16])
            .copied()
            .collect();
        let uniform = pixels.iter().min() == pixels.iter().max();
        assert_eq!(
            case.rust(),
            [u32::from(pop <= a[0] as usize), u32::from(uniform)]
        );
    }
    for case in search::cost_cases() {
        let a = &case.input;
        let (col, row) = (a[0] % 4, a[0] / 4);
        let (mx, my) = (a[1] % 2, a[1] / 2);
        let top = if row == 0 {
            a[(2 + 4 * my * 9 + 1 + 4 * mx + col) as usize]
        } else {
            a[(83 + a[0] - 4) as usize]
        };
        let left = if col == 0 {
            a[(2 + (1 + 4 * my + row) * 9 + 4 * mx) as usize]
        } else {
            a[(83 + a[0] - 1) as usize]
        };
        assert_eq!(
            case.rust(),
            VP8FixedCostsI4[top as usize][left as usize].map(u32::from)
        );
    }
}

fn incoming(it: &VP8EncIterator, enc: &VP8Encoder) -> ([usize; 9], [usize; 9]) {
    let upper = enc.nz[it.x + 1];
    let beside = enc.nz[it.x];
    let top =
        std::array::from_fn(|i| ((upper >> [12, 13, 14, 15, 18, 19, 22, 23, 24][i]) & 1) as usize);
    let left = std::array::from_fn(|i| {
        if i == 8 {
            it.left_nz[8] as usize
        } else {
            ((beside >> [3, 7, 11, 15, 17, 19, 21, 23][i]) & 1) as usize
        }
    });
    (top, left)
}
fn residual(levels: &[i16; 16], first: usize, ty: usize, ctx: usize, enc: &VP8Encoder) -> i64 {
    let words = levels.map(|v| v as i32 as u32);
    i64::from(support::vp8_cost::tree_cost(
        &words,
        first,
        ty,
        ctx,
        &enc.proba.coeffs,
    ))
}
fn rate16(it: &VP8EncIterator, enc: &VP8Encoder, rd: &VP8ModeScore) -> i64 {
    let (mut top, mut left) = incoming(it, enc);
    let mut rate = residual(&rd.y_dc_levels, 0, 1, top[8] + left[8], enc);
    for (n, block) in rd.y_ac_levels.iter().enumerate() {
        let (x, y) = (n % 4, n / 4);
        rate += residual(block, 1, 0, top[x] + left[y], enc);
        let nz = usize::from(block[1..].iter().any(|&v| v != 0));
        top[x] = nz;
        left[y] = nz;
    }
    rate
}
fn rate_uv(it: &VP8EncIterator, enc: &VP8Encoder, rd: &VP8ModeScore) -> i64 {
    let (mut top, mut left) = incoming(it, enc);
    let mut rate = 0;
    for (n, block) in rd.uv_levels.iter().enumerate() {
        let (x, y) = (4 + 2 * (n / 4) + n % 2, 4 + 2 * (n / 4) + (n % 4) / 2);
        rate += residual(block, 0, 2, top[x] + left[y], enc);
        let nz = usize::from(block.iter().any(|&v| v != 0));
        top[x] = nz;
        left[y] = nz;
    }
    rate
}
fn sse(a: &[u8], b: &[u8], w: usize, h: usize) -> i64 {
    (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                let d = i64::from(a[y * BPS + x]) - i64::from(b[y * BPS + x]);
                d * d
            })
        })
        .sum()
}
fn texture(a: &[u8], b: &[u8], size: usize, lambda: i32) -> i64 {
    let h = [
        [1i64, 1, 1, 1],
        [1, 1, -1, -1],
        [1, -1, -1, 1],
        [1, -1, 1, -1],
    ];
    let weights = [38i64, 32, 20, 9, 32, 28, 17, 7, 20, 17, 10, 4, 9, 7, 4, 2];
    let energy = |pixels: &[u8]| {
        let mut sum = 0;
        for y in 0..4 {
            for x in 0..4 {
                let mut coefficient = 0;
                for j in 0..4 {
                    for i in 0..4 {
                        coefficient += h[y][j] * i64::from(pixels[j * BPS + i]) * h[x][i];
                    }
                }
                sum += coefficient.abs() * weights[y * 4 + x];
            }
        }
        sum
    };
    let mut total = 0;
    for y in (0..size).step_by(4) {
        for x in (0..size).step_by(4) {
            let off = y * BPS + x;
            total += (energy(&a[off..]) - energy(&b[off..])).abs() / 32;
        }
    }
    (i64::from(lambda) * total + 128) / 256
}
fn score(rd: &VP8ModeScore, lambda: i32) -> i64 {
    (rd.H + rd.R) * i64::from(lambda) + 256 * (rd.D + rd.SD)
}
fn ac_population(blocks: &[[i16; 16]]) -> usize {
    blocks
        .iter()
        .flat_map(|b| &b[1..])
        .filter(|&&v| v != 0)
        .count()
}

// Enumerate every candidate, including ones skipped by the production lower-
// bound check. Metrics use wider sums/matrix products and rates use syntax-tree
// walks rather than the encoder's cached residual costs.
fn model(
    a: &[u32],
    enc: &VP8Encoder,
    it: &mut VP8EncIterator,
    rd: &mut VP8ModeScore,
    counters: &mut [usize; 5],
) -> (bool, bool) {
    let stage = a[634];
    let m = &enc.dqm[a[2] as usize];
    let mut pixels = it.yuv_out;
    let mut selected_i4 = false;
    let mut aborted = false;
    VP8MakeLuma16Preds(it, enc);
    VP8MakeChroma8Preds(it, enc);
    if stage == 2 {
        InitScore(rd);
    } else {
        rd.score = (u64::from(a[637]) | (u64::from(a[638]) << 32)) as i64;
    }
    if stage <= 2 {
        let mut flat = (0..16).all(|y| {
            it.yuv_in[y * BPS..y * BPS + 16]
                .iter()
                .all(|&v| v == it.yuv_in[0])
        });
        let mut candidates = Vec::new();
        let mut was_flat = flat;
        for mode in 0..4 {
            let mut cur = VP8ModeScore::default();
            let mut dst = [0; 512];
            cur.mode_i16 = mode as i32;
            cur.nz = ReconstructIntra16(it, enc, &mut cur, &mut dst, mode);
            cur.D = sse(&it.yuv_in, &dst, 16, 16);
            cur.SD = texture(&it.yuv_in, &dst, 16, m.tlambda);
            cur.H = i64::from(VP8FixedCostsI16[mode]);
            cur.R = rate16(it, enc, &cur);
            let empty = ac_population(&cur.y_ac_levels) == 0;
            if was_flat && !flat && empty {
                counters[4] += 1;
            }
            if flat {
                flat = empty;
                if flat {
                    cur.D *= 2;
                    cur.SD *= 2;
                } else {
                    was_flat = true;
                }
            }
            cur.score = score(&cur, m.lambda_i16);
            candidates.push((cur, dst));
        }
        let (mut best, dst) = candidates
            .into_iter()
            .min_by_key(|(r, _)| (r.score, r.mode_i16))
            .unwrap();
        best.score = score(&best, m.lambda_mode);
        CopyScore(rd, &best);
        rd.mode_i16 = best.mode_i16;
        rd.y_dc_levels = best.y_dc_levels;
        rd.y_ac_levels = best.y_ac_levels;
        for y in 0..16 {
            pixels[y * BPS..y * BPS + 16].copy_from_slice(&dst[y * BPS..y * BPS + 16]);
        }
        if stage == 1 {
            rd.modes_i4 = std::array::from_fn(|i| (i % 10) as u8);
        }
    }
    if stage == 1 || stage == 2 || stage == 4 {
        if enc.max_i4_header_bits == 0 {
            counters[0] += 1;
            aborted = true;
        } else {
            VP8IteratorStartI4(it, enc);
            let mut total = VP8ModeScore {
                H: 211,
                ..Default::default()
            };
            total.score = 211 * i64::from(m.lambda_mode);
            let mut headers = 0;
            let mut chosen = [0u8; 16];
            let mut levels = [[0i16; 16]; 16];
            let mut blocks = [0; 512];
            for n in 0..16 {
                MakeIntra4Preds(it);
                let off = 4 * (n / 4) * BPS + 4 * (n % 4);
                let base = enc.preds_offset(it.x, it.y);
                let left = if n % 4 == 0 {
                    enc.preds[base + (n / 4) * enc.preds_w - 1]
                } else {
                    chosen[n - 1]
                };
                let top = if n < 4 {
                    enc.preds[base - enc.preds_w + n]
                } else {
                    chosen[n - 4]
                };
                let mut candidates = Vec::new();
                for mode in 0..10 {
                    let mut cur = VP8ModeScore::default();
                    let mut lev = [0; 16];
                    let mut dst = [0; 3 * BPS + 4];
                    cur.nz = u32::from(ReconstructIntra4(
                        it,
                        enc,
                        &mut lev,
                        &it.yuv_in[off..],
                        &mut dst,
                        mode,
                    )) << n;
                    cur.D = sse(&it.yuv_in[off..], &dst, 4, 4);
                    cur.SD = texture(&it.yuv_in[off..], &dst, 4, m.tlambda);
                    cur.H = i64::from(VP8FixedCostsI4[top as usize][left as usize][mode]);
                    cur.R = residual(
                        &lev,
                        0,
                        3,
                        usize::from(it.top_nz[n % 4] + it.left_nz[n / 4]),
                        enc,
                    ) + if mode > 0 && ac_population(&[lev]) <= 3 {
                        140
                    } else {
                        0
                    };
                    cur.score = score(&cur, m.lambda_i4);
                    candidates.push((cur, dst, lev, mode));
                }
                let (mut best, dst, lev, mode) = candidates
                    .into_iter()
                    .min_by_key(|(r, _, _, mode)| (r.score, *mode))
                    .unwrap();
                best.score = score(&best, m.lambda_mode);
                total.D += best.D;
                total.SD += best.SD;
                total.H += best.H;
                total.R += best.R;
                total.score += best.score;
                total.nz |= best.nz;
                if total.score >= rd.score {
                    counters[1] += 1;
                    aborted = true;
                    break;
                }
                headers += best.H as i32;
                if headers > enc.max_i4_header_bits {
                    counters[2] += 1;
                    aborted = true;
                    break;
                }
                chosen[n] = mode as u8;
                rd.modes_i4[n] = mode as u8;
                levels[n] = lev;
                for y in 0..4 {
                    blocks[off + y * BPS..off + y * BPS + 4]
                        .copy_from_slice(&dst[y * BPS..y * BPS + 4]);
                }
                it.top_nz[n % 4] = u8::from(best.nz != 0);
                it.left_nz[n / 4] = u8::from(best.nz != 0);
                VP8IteratorRotateI4(it, &blocks);
            }
            if !aborted {
                counters[3] += 1;
                selected_i4 = true;
                CopyScore(rd, &total);
                rd.y_ac_levels = levels;
                for y in 0..16 {
                    pixels[y * BPS..y * BPS + 16].copy_from_slice(&blocks[y * BPS..y * BPS + 16]);
                }
            }
        }
    }
    if stage == 2 || stage == 3 {
        let mut candidates = Vec::new();
        for mode in 0..4 {
            let mut cur = VP8ModeScore::default();
            let mut dst = [0; 7 * BPS + 16];
            cur.mode_uv = mode as i32;
            cur.nz = ReconstructUV(it, enc, &mut cur, &mut dst, mode);
            cur.D = sse(&it.yuv_in[16..], &dst, 16, 8);
            cur.H = i64::from(VP8FixedCostsUV[mode]);
            cur.R = rate_uv(it, enc, &cur)
                + if mode > 0 && ac_population(&cur.uv_levels) <= 2 {
                    140 * 8
                } else {
                    0
                };
            cur.score = score(&cur, m.lambda_uv);
            candidates.push((cur, dst));
        }
        let (best, dst) = candidates
            .into_iter()
            .min_by_key(|(r, _)| (r.score, r.mode_uv))
            .unwrap();
        rd.D += best.D;
        rd.SD += best.SD;
        rd.H += best.H;
        rd.R += best.R;
        rd.score += best.score;
        rd.nz |= best.nz;
        rd.mode_uv = best.mode_uv;
        rd.uv_levels = best.uv_levels;
        if enc.top_derr.is_some() {
            rd.derr = best.derr;
        }
        for y in 0..8 {
            pixels[16 + y * BPS..32 + y * BPS].copy_from_slice(&dst[y * BPS..y * BPS + 16]);
        }
    }
    it.yuv_out = pixels;
    (selected_i4, aborted)
}

#[test]
fn unpruned_search_matches_independent_scores_rates_and_selected_pixels() {
    let mut counters = [0usize; 5];
    let mut selected = [0usize; 2];
    let mut modes16 = [0usize; 4];
    let mut modesuv = [0usize; 4];
    let mut modes4 = [0usize; 10];
    for case in search::search_cases() {
        let a = &case.input;
        let (mut enc, mut it) = search::initialized(a);
        let mut modeled = it.clone();
        let mut expected = support::vp8_reconstruct::seed_score();
        let mut actual = expected.clone();
        let (choice, aborted) = model(a, &enc, &mut modeled, &mut expected, &mut counters);
        let packed = enc.nz.clone();
        let stats = enc.proba.stats;
        let result = search::run(a, &mut enc, &mut it, &mut actual);
        assert_eq!(
            search::logical_score(a, &enc, &it, &actual),
            search::logical_score(a, &enc, &it, &expected),
            "scores {:?}",
            &a[634..639]
        );
        assert_eq!(enc.nz, packed);
        assert_eq!(enc.proba.stats, stats);
        let stage = a[634];
        if stage == 1 || stage == 4 {
            assert_eq!(result, choice);
        }
        if stage <= 2 || stage == 4 {
            if !aborted || stage != 4 {
                for y in 0..16 {
                    assert_eq!(
                        &it.yuv_out[y * BPS..y * BPS + 16],
                        &modeled.yuv_out[y * BPS..y * BPS + 16]
                    );
                }
            }
        }
        if stage == 2 || stage == 3 {
            for y in 0..8 {
                assert_eq!(
                    &it.yuv_out[16 + y * BPS..32 + y * BPS],
                    &modeled.yuv_out[16 + y * BPS..32 + y * BPS]
                );
            }
            modesuv[actual.mode_uv as usize] += 1;
        }
        if stage == 2 {
            let mb = enc.mb_info[it.y * enc.mb_w + it.x];
            assert_eq!(mb.r#type, u8::from(!choice));
            assert_eq!(mb.skip, actual.nz == 0);
            assert_eq!(result, mb.skip);
            selected[usize::from(choice)] += 1;
            modes16[actual.mode_i16 as usize] += 1;
            if choice {
                for mode in actual.modes_i4 {
                    modes4[mode as usize] += 1;
                }
            }
            let offset = enc.preds_offset(it.x, it.y);
            for y in 0..4 {
                for x in 0..4 {
                    assert_eq!(
                        enc.preds[offset + y * enc.preds_w + x],
                        if choice {
                            actual.modes_i4[y * 4 + x]
                        } else {
                            actual.mode_i16 as u8
                        }
                    );
                }
            }
        }
    }
    assert!(counters.iter().all(|&n| n > 0), "{counters:?}");
    assert!(selected.iter().all(|&n| n > 0));
    assert!(
        modes16
            .iter()
            .chain(&modesuv)
            .chain(&modes4)
            .all(|&n| n > 0)
    );
    eprintln!(
        "disabled/score/header/accepted/sticky-flat {counters:?}; I16/I4 {selected:?}; I16 {modes16:?}; UV {modesuv:?}; I4 {modes4:?}"
    );
}

#[test]
fn exact_header_and_score_boundaries_keep_the_pinned_comparisons() {
    let outputs: Vec<_> = search::boundary_cases()
        .iter()
        .map(|case| case.rust()[0])
        .collect();
    assert_eq!(outputs, [0, 1, 0, 0, 1]);
}

#[test]
fn raster_decisions_propagate_geometric_borders_and_coefficient_contexts() {
    let mut count = 0;
    let mut dc_retained = 0;
    let mut diffused = 0;
    for case in search::frame_cases() {
        let a = &case.input;
        let out = case.rust();
        let (w, h) = ((a[0] as usize).div_ceil(16), (a[1] as usize).div_ceil(16));
        let diffusion = f32::from_bits(a[3]) <= 98.;
        let mut planes = [
            vec![0u8; w * 16 * h * 16],
            vec![0u8; w * 8 * h * 8],
            vec![0u8; w * 8 * h * 8],
        ];
        let mut top_dc = vec![0u32; w];
        let mut left_dc = 0;
        for n in 0..w * h {
            let p = &out[6 + 1047 * n..6 + 1047 * (n + 1)];
            let (x, y) = (n % w, n / w);
            if x == 0 {
                left_dc = 0;
            }
            let is16 = p[948] == 1;
            let mut mask = 0u32;
            for block in 0..16 {
                let c = &p[27 + block * 16..27 + (block + 1) * 16];
                if is16 {
                    assert_eq!(c[0], 0);
                }
                mask |= u32::from(c.iter().any(|&v| v != 0)) << block;
            }
            for block in 0..8 {
                let c = &p[283 + block * 16..283 + (block + 1) * 16];
                mask |= u32::from(c.iter().any(|&v| v != 0)) << (16 + block);
            }
            if is16 {
                let nz = u32::from(p[11..27].iter().any(|&v| v != 0));
                top_dc[x] = nz;
                left_dc = nz;
                mask |= nz << 24;
            } else {
                dc_retained += usize::from(top_dc[x] != 0 || left_dc != 0);
            }
            assert_eq!(p[429], mask);
            assert_eq!(p[0], u32::from(mask == 0));
            assert_eq!(p[950], p[0]);
            let bit = |n: u32| (mask >> n) & 1u32;
            let top = [
                bit(12),
                bit(13),
                bit(14),
                bit(15),
                bit(18),
                bit(19),
                bit(22),
                bit(23),
                top_dc[x],
            ];
            let left = [
                bit(3),
                bit(7),
                bit(11),
                bit(15),
                bit(17),
                bit(19),
                bit(21),
                bit(23),
                left_dc,
            ];
            assert_eq!(&p[953..962], &top);
            assert_eq!(&p[962..971], &left);
            let boundary_bits = [3, 7, 11, 12, 13, 14, 15, 17, 18, 19, 21, 22, 23]
                .into_iter()
                .fold(0u32, |m, b| m | (1 << b));
            assert_eq!(p[971], (mask & boundary_bits) | (top_dc[x] << 24));
            if diffusion {
                for ch in 0..2 {
                    let err = p[430 + 3 * ch..433 + 3 * ch]
                        .iter()
                        .map(|&v| v as i32)
                        .collect::<Vec<_>>();
                    let down = (3 * err[2]).div_euclid(4);
                    assert_eq!(
                        &p[972 + 2 * ch..974 + 2 * ch],
                        &[err[0] as u32, down as u32]
                    );
                    assert_eq!(
                        &p[976 + 2 * ch..978 + 2 * ch],
                        &[err[1] as u32, (err[2] - down) as u32]
                    );
                    diffused += usize::from(err.iter().any(|&v| v != 0));
                }
            } else {
                assert!(p[430..436].iter().chain(&p[972..980]).all(|&v| v == 0));
            }
            for ch in 0..3 {
                let size = if ch == 0 { 16 } else { 8 };
                let off = if ch == 0 { 0 } else { 16 + 8 * (ch - 1) };
                let stride = w * size;
                for row in 0..size {
                    for col in 0..size {
                        planes[ch][(y * size + row) * stride + x * size + col] =
                            p[436 + off + row * BPS + col] as u8;
                    }
                }
                let at = if ch == 0 { 980 } else { 997 + 9 * (ch - 1) };
                let left_x = if x + 1 < w {
                    (x * size + size - 1) as isize
                } else {
                    x as isize * size as isize - 1
                };
                let old_top_y = y as isize * size as isize - 1;
                let corner = if left_x < 0 {
                    if y == 0 { 127 } else { 129 }
                } else if old_top_y < 0 {
                    127
                } else {
                    planes[ch][old_top_y as usize * stride + left_x as usize]
                };
                assert_eq!(p[at], u32::from(corner));
                for row in 0..size {
                    let value = if left_x < 0 {
                        129
                    } else {
                        planes[ch][(y * size + row) * stride + left_x as usize]
                    };
                    assert_eq!(p[at + 1 + row], u32::from(value));
                }
                let top_y = if y + 1 < h {
                    (y * size + size - 1) as isize
                } else {
                    old_top_y
                };
                let top_at = if ch == 0 { 1015 } else { 1031 + 8 * (ch - 1) };
                for col in 0..size {
                    let value = if top_y < 0 {
                        127
                    } else {
                        planes[ch][top_y as usize * stride + x * size + col]
                    };
                    assert_eq!(p[top_at + col], u32::from(value));
                }
            }
            count += 1;
        }
        let expected =
            1067 + 1047 * w * h + (4 * w + 1) * (4 * h + 1) + w + if diffusion { 4 * w } else { 0 };
        assert_eq!(out.len(), expected);
    }
    assert!(dc_retained > 0 && diffused > 0);
    eprintln!(
        "{count} raster macroblocks, {dc_retained} retained DC contexts, {diffused} nonzero diffusion commits"
    );
}

#[test]
fn equal_candidate_scores_keep_the_earliest_mode() {
    for case in search::tie_cases() {
        let a = &case.input;
        let (mut enc, mut it) = search::initialized(a);
        let mut rd = support::vp8_reconstruct::seed_score();
        search::run(a, &mut enc, &mut it, &mut rd);
        match a[634] {
            0 => assert_eq!(rd.mode_i16, 0),
            2 => {
                assert_eq!(rd.mode_i16, 0);
                assert_eq!(rd.mode_uv, 0);
            }
            3 => assert_eq!(rd.mode_uv, 0),
            4 => assert_eq!(rd.modes_i4, [0; 16]),
            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn search_matches_pinned_reference() {
    let cases = search::cases();
    native::parallel_cases("vp8-search", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!("{} exact mode-search/helper requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn search_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = search::fixture_cases();
    let outputs = native::run("vp8-search-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C default mode search and score helpers.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_search.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_search.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
