#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::{cost::VP8EncBands, enc::BPS},
    enc::{iterator_enc::*, picture_enc::WebPYUVAPicture, vp8i_enc::*},
};
use support::{vp8_cost as cost, vp8_iterator as iter};

#[test]
fn committed_iterator_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_iterator.txt"));
    let cases = iter::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

fn sample(x: usize, y: usize, channel: usize) -> u8 {
    (x * 31 + y * 17 + channel * 73) as u8
}
fn picture(w: usize, h: usize) -> WebPYUVAPicture {
    let plane = |w, h, c| {
        (0..h)
            .flat_map(move |y| (0..w).map(move |x| sample(x, y, c)))
            .collect()
    };
    WebPYUVAPicture {
        width: w as u32,
        height: h as u32,
        y: plane(w, h, 0),
        u: plane(w.div_ceil(2), h.div_ceil(2), 1),
        v: plane(w.div_ceil(2), h.div_ceil(2), 2),
        a: None,
    }
}

#[test]
fn raster_import_and_saved_borders_follow_source_or_reconstructed_coordinates() {
    let mut blocks = 0;
    for w in [1, 3, 7, 8, 15, 16, 17, 31, 32, 33, 49] {
        for h in [1, 3, 15, 16, 17, 31, 33] {
            for source in [false, true] {
                let pic = picture(w, h);
                let mut enc = VP8Encoder::new(w as u32, h as u32, true).unwrap();
                let mut it = VP8EncIterator::default();
                VP8IteratorInit(&mut enc, &mut it);
                let mut visited = Vec::new();
                while !VP8IteratorIsDone(&it) {
                    VP8IteratorImport(&mut it, &enc, &pic, source);
                    visited.push((it.x, it.y));
                    let (x, y) = (it.x, it.y);
                    let top_y = it.y_top(&enc);
                    let top_uv = it.uv_top(&enc);
                    for channel in 0..3 {
                        let (size, off, width, height) = if channel == 0 {
                            (16, 0, w, h)
                        } else {
                            (8, 16 + 8 * (channel - 1), w.div_ceil(2), h.div_ceil(2))
                        };
                        let rec = |x, y| sample(x, y, channel).wrapping_add(31);
                        let neighbor = |x, y| {
                            if source {
                                sample(x, y, channel)
                            } else {
                                rec(x, y)
                            }
                        };
                        let left = if channel == 0 {
                            &it.y_left[..]
                        } else {
                            &it.uv_left[16 * (channel - 1)..16 * (channel - 1) + 9]
                        };
                        for row in 0..size {
                            for col in 0..size {
                                assert_eq!(
                                    it.yuv_in[off + row * BPS + col],
                                    sample(
                                        (size * x + col).min(width - 1),
                                        (size * y + row).min(height - 1),
                                        channel
                                    )
                                );
                            }
                        }
                        for row in 0..size {
                            assert_eq!(
                                left[row + 1],
                                if x == 0 {
                                    129
                                } else {
                                    neighbor(size * x - 1, (size * y + row).min(height - 1))
                                }
                            );
                        }
                        assert_eq!(
                            left[0],
                            if x == 0 {
                                if y == 0 { 127 } else { 129 }
                            } else if y == 0 {
                                127
                            } else {
                                neighbor(size * x - 1, size * y - 1)
                            }
                        );
                        let top = if channel == 0 {
                            top_y
                        } else {
                            &top_uv[8 * (channel - 1)..]
                        };
                        for col in 0..size {
                            assert_eq!(
                                top[col],
                                if y == 0 {
                                    127
                                } else {
                                    neighbor((size * x + col).min(width - 1), size * y - 1)
                                }
                            );
                        }
                    }
                    // Chroma's unused lower half and both spare caches are untouched.
                    for row in 8..16 {
                        assert_eq!(&it.yuv_in[row * BPS + 16..row * BPS + 32], &[0; 16]);
                    }
                    assert!(it.yuv_out2.iter().chain(&it.yuv_p).all(|&v| v == 0));
                    for ch in 0..3 {
                        let (size, off, width, height) = if ch == 0 {
                            (16, 0, w, h)
                        } else {
                            (8, 16 + 8 * (ch - 1), w.div_ceil(2), h.div_ceil(2))
                        };
                        for row in 0..size {
                            for col in 0..size {
                                it.yuv_out[off + row * BPS + col] = sample(
                                    (size * x + col).min(width - 1),
                                    (size * y + row).min(height - 1),
                                    ch,
                                )
                                .wrapping_add(31);
                            }
                        }
                    }
                    if !source {
                        VP8IteratorSaveBoundary(&mut it, &mut enc);
                    }
                    let next = VP8IteratorNext(&mut it, &enc);
                    assert_eq!(next, !VP8IteratorIsDone(&it));
                    blocks += 1;
                }
                assert_eq!(
                    visited,
                    (0..enc.mb_h)
                        .flat_map(|y| (0..enc.mb_w).map(move |x| (x, y)))
                        .collect::<Vec<_>>()
                );
                assert_eq!((it.x, it.y, it.count_down), (0, enc.mb_h, 0));
                assert_eq!(enc.nz[0], 0);
            }
        }
    }
    eprintln!("{blocks} independently checked macroblocks");
}

#[test]
fn intra4_windows_match_geometric_neighbors_and_external_top_right_samples() {
    let mut windows = 0;
    for (w, h) in [(16, 16), (17, 17), (32, 33), (49, 35)] {
        let pic = picture(w, h);
        let mut enc = VP8Encoder::new(w as u32, h as u32, false).unwrap();
        let mut it = VP8EncIterator::default();
        VP8IteratorInit(&mut enc, &mut it);
        loop {
            VP8IteratorImport(&mut it, &enc, &pic, false);
            for row in 0..16 {
                for col in 0..16 {
                    it.yuv_out[row * BPS + col] =
                        sample(it.x * 16 + col, it.y * 16 + row, 0).wrapping_add(47);
                }
            }
            let left = it.y_left;
            let mut above = [0u8; 20];
            above[..16].copy_from_slice(&it.y_top(&enc)[..16]);
            if it.x + 1 < enc.mb_w {
                above[16..].copy_from_slice(&it.y_top(&enc)[16..20]);
            } else {
                let last = above[15];
                above[16..].fill(last);
            }
            VP8IteratorStartI4(&mut it, &enc);
            for block in 0..16 {
                let (x, y) = ((block % 4) * 4, (block / 4) * 4);
                let pixel = |cx, cy| it.yuv_out[cy * BPS + cx];
                let l = |row| {
                    if x == 0 {
                        left[y + row + 1]
                    } else {
                        pixel(x - 1, y + row)
                    }
                };
                let corner = if y == 0 {
                    if x == 0 { left[0] } else { above[x - 1] }
                } else if x == 0 {
                    left[y]
                } else {
                    pixel(x - 1, y - 1)
                };
                let mut expected = [l(3), l(2), l(1), l(0), corner, 0, 0, 0, 0, 0, 0, 0, 0];
                for col in 0..8 {
                    expected[5 + col] = if y == 0 || x + col >= 16 {
                        above[x + col]
                    } else {
                        pixel(x + col, y - 1)
                    };
                }
                assert_eq!(
                    it.i4_window(),
                    &expected,
                    "mb {},{} block {block}",
                    it.x,
                    it.y
                );
                windows += 1;
                let out = it.yuv_out;
                assert_eq!(VP8IteratorRotateI4(&mut it, &out), block < 15);
            }
            assert_eq!(it.i4, 16);
            VP8IteratorSaveBoundary(&mut it, &mut enc);
            if !VP8IteratorNext(&mut it, &enc) {
                break;
            }
        }
    }
    eprintln!("{windows} independently checked Intra4 windows");
}

#[test]
fn nonzero_contexts_exhaust_outgoing_edge_patterns_and_keep_dc_separate() {
    for case in iter::nz_cases() {
        let a = &case.input;
        let output = case.rust();
        let top: Vec<_> = if a[0] == 0 {
            [12, 13, 14, 15, 18, 19, 22, 23, 24]
                .into_iter()
                .map(|bit| (a[1] >> bit) & 1)
                .collect()
        } else {
            a[4..13].to_vec()
        };
        let mut left = if a[0] == 0 {
            [3, 7, 11, 15, 17, 19, 21, 23]
                .into_iter()
                .map(|bit| (a[2] >> bit) & 1)
                .chain([a[3]])
                .collect::<Vec<_>>()
        } else {
            a[13..22].to_vec()
        };
        left[8] = a[3];
        let positions = [3, 7, 11, 12, 13, 14, 15, 17, 18, 19, 21, 22, 23, 24];
        let values = [
            left[0], left[1], left[2], top[0], top[1], top[2], top[3], left[4], top[4], top[5],
            left[6], top[6], top[7], top[8],
        ];
        let packed: u32 = positions
            .into_iter()
            .zip(values)
            .map(|(p, v)| v * 2u32.pow(p))
            .sum();
        assert_eq!(&output[..9], top);
        assert_eq!(&output[9..18], left);
        assert_eq!(&output[18..], &[packed, a[2]]);
    }
}

#[test]
fn row_and_restart_reset_only_the_upstream_state_and_preserve_modes() {
    let mut enc = VP8Encoder::new(33, 35, true).unwrap();
    let mut it = VP8EncIterator {
        yuv_in: [23; YUV_SIZE_ENC],
        yuv_out: [45; YUV_SIZE_ENC],
        yuv_out2: [67; YUV_SIZE_ENC],
        yuv_p: [89; PRED_SIZE_ENC],
        top_nz: [1; 9],
        left_nz: [1; 9],
        luma_bits: 123,
        uv_bits: 456,
        ..Default::default()
    };
    enc.preds.fill(6);
    enc.mb_info[0].alpha = 19;
    enc.nz.fill(0xffffff);
    enc.nz[0] = 0;
    enc.y_top.fill(9);
    enc.uv_top.fill(11);
    enc.top_derr.as_mut().unwrap().fill([[7; 2]; 2]);
    it.left_derr = [[8; 2]; 2];
    it.do_trellis = true;
    it.bit_count = [[9; 3]; 4];
    VP8IteratorInit(&mut enc, &mut it);
    assert_eq!((it.x, it.y, it.count_down, it.count_down0), (0, 0, 9, 9));
    assert_eq!(
        it.y_left,
        [
            127, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129, 129
        ]
    );
    assert_eq!(it.top_nz, [1; 9]);
    assert_eq!(it.left_nz, [1, 1, 1, 1, 1, 1, 1, 1, 0]);
    assert_eq!((it.luma_bits, it.uv_bits), (123, 456));
    assert_eq!(it.bit_count, [[0; 3]; 4]);
    assert!(!it.do_trellis);
    assert_eq!(it.left_derr, [[0; 2]; 2]);
    assert!(
        enc.top_derr
            .as_ref()
            .unwrap()
            .iter()
            .all(|v| *v == [[0; 2]; 2])
    );
    assert!(enc.nz.iter().all(|&v| v == 0));
    assert!(enc.y_top.iter().chain(&enc.uv_top).all(|&v| v == 127));
    assert!(enc.preds.iter().all(|&v| v == 6));
    assert_eq!(enc.mb_info[0].alpha, 19);
    assert!(it.yuv_in.iter().all(|&v| v == 23));
    assert!(it.yuv_out.iter().all(|&v| v == 45));
    assert!(it.yuv_out2.iter().all(|&v| v == 67));
    assert!(it.yuv_p.iter().all(|&v| v == 89));
    enc.y_top[0] = 1;
    enc.nz[1] = 123;
    it.top_nz = [1; 9];
    it.left_nz = [1; 9];
    it.left_derr = [[9; 2]; 2];
    VP8IteratorSetRow(&mut it, &enc, 2);
    assert_eq!((it.x, it.y, it.y_left[0]), (0, 2, 129));
    assert_eq!(enc.y_top[0], 1);
    assert_eq!(enc.nz[1], 123);
    assert_eq!(it.top_nz, [1; 9]);
    assert_eq!(it.left_nz, [1, 1, 1, 1, 1, 1, 1, 1, 0]);
    assert_eq!(it.left_derr, [[0; 2]; 2]);
    for n in [-1, 0, 1, 3] {
        VP8IteratorSetCountDown(&mut it, n);
        assert_eq!(VP8IteratorIsDone(&it), n <= 0);
        assert_eq!(it.count_down0, n);
    }
    enc.top_derr = None;
    it.left_derr = [[9; 2]; 2];
    VP8IteratorSetRow(&mut it, &enc, 0);
    assert_eq!(it.left_derr, [[9; 2]; 2]);
    for (w, h) in [(0, 1), (1, 0), (16384, 1), (1, 16384), (u32::MAX, u32::MAX)] {
        assert!(VP8Encoder::new(w, h, false).is_err());
    }
    for (w, h) in [(1, 16383), (16383, 1)] {
        let e = VP8Encoder::new(w, h, false).unwrap();
        assert_eq!(e.mb_info.len(), 1024);
        assert_eq!(e.preds.len(), e.preds_w * (e.mb_h * 4 + 1));
    }
}

// Sum events from the independent coefficient syntax tree rather than reading
// the encoder's residual cost cache. Keep the pin's level-9/10 estimates.
fn tree_cost(
    block: &[u32],
    first: usize,
    ty: usize,
    mut ctx: usize,
    prob: &[[ProbaArray; 8]; 4],
) -> u32 {
    let mut sum = 0;
    let last = block.iter().rposition(|&v| v != 0);
    let mut event = |n: usize, context: usize, node: usize, bit| {
        sum += cost::bit_cost(bit, prob[ty][VP8EncBands[n]][context][node]);
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
            for (node, bit, fixed) in cost::level_events(v) {
                if fixed.is_none() {
                    event(n, ctx, node, bit);
                }
            }
            ctx = v.min(2);
            if n < 15 {
                event(n + 1, ctx, 0, n < last);
            }
        }
        for &word in &block[first..=last] {
            let v = (word as i32).unsigned_abs() as usize;
            if v != 0 {
                sum += if v == 9 || v == 10 {
                    640
                } else {
                    cost::level_events(v)
                        .into_iter()
                        .filter_map(|(_, bit, fixed)| fixed.map(|p| cost::bit_cost(bit, p)))
                        .sum()
                };
            }
        }
    }
    sum
}

#[test]
fn macroblock_costs_follow_independent_tree_walk_and_context_grid() {
    for case in iter::cost_cases() {
        let a = &case.input;
        let output = case.rust();
        let prob = cost::read_probas(&a[22..1078]).coeffs;
        let mut top = a[4..13].to_vec();
        let mut left = a[13..22].to_vec();
        left[8] = a[2];
        let mut expected = vec![tree_cost(
            &a[1478..1494],
            0,
            3,
            (top[(a[3] & 3) as usize] + left[(a[3] >> 2) as usize]) as usize,
            &prob,
        )];
        expected.extend_from_slice(&top);
        expected.extend_from_slice(&left);
        let unpack = || {
            let t = [12, 13, 14, 15, 18, 19, 22, 23, 24]
                .map(|p| (a[0] >> p) & 1)
                .to_vec();
            let l = [3, 7, 11, 15, 17, 19, 21, 23]
                .map(|p| (a[1] >> p) & 1)
                .into_iter()
                .chain([a[2]])
                .collect::<Vec<_>>();
            (t, l)
        };
        (top, left) = unpack();
        let mut total = tree_cost(&a[1078..1094], 0, 1, (top[8] + left[8]) as usize, &prob);
        for block in 0..16 {
            let (x, y) = (block % 4, block / 4);
            let coeffs = &a[1094 + 16 * block..1110 + 16 * block];
            total += tree_cost(coeffs, 1, 0, (top[x] + left[y]) as usize, &prob);
            top[x] = u32::from(coeffs.iter().any(|&v| v != 0));
            left[y] = top[x];
        }
        expected.push(total);
        expected.extend_from_slice(&top);
        expected.extend_from_slice(&left);
        (top, left) = unpack();
        total = 0;
        for ch in 0..2 {
            for block in 0..4 {
                let (x, y) = (4 + ch * 2 + block % 2, 4 + ch * 2 + block / 2);
                let from = 1350 + 16 * (4 * ch + block);
                let coeffs = &a[from..from + 16];
                total += tree_cost(coeffs, 0, 2, (top[x] + left[y]) as usize, &prob);
                top[x] = u32::from(coeffs.iter().any(|&v| v != 0));
                left[y] = top[x];
            }
        }
        expected.push(total);
        expected.extend(top);
        expected.extend(left);
        expected.extend([a[1], a[0]]);
        assert_eq!(output, expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn iterator_and_macroblock_costs_match_pinned_reference() {
    let cases = iter::cases();
    native::parallel_cases("vp8-iterator", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!(
        "{} exact iterator/context/macroblock-cost requests",
        cases.len()
    );
}
#[cfg(feature = "native-oracle")]
#[test]
fn iterator_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = iter::fixture_cases();
    let outputs = native::run("vp8-iterator-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C iterator/context/macroblock costs.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_iterator.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_iterator.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
