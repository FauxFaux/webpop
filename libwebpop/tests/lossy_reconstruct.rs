#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::enc::*,
    enc::{iterator_enc::*, quant_enc::*, vp8i_enc::*},
};
use support::vp8_reconstruct as reconstruction;

#[test]
fn committed_reconstruction_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_reconstruct.txt"));
    let cases = reconstruction::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

// Wider rational quantization, independent of QUANTDIV and QuantizeSingle.
fn single_model(value: i16, m: &VP8Matrix) -> (i16, i32) {
    let magnitude = i64::from(value).abs();
    let level = (magnitude * i64::from(m.iq[0]) + i64::from(m.bias[0])) / 131072;
    let signed = level * i64::from(m.q[0]) * if value < 0 { -1 } else { 1 };
    (
        signed as i16,
        (i64::from(value) - signed).div_euclid(2) as i32,
    )
}

#[test]
fn signed_scalar_quantization_and_uncapped_levels() {
    for case in reconstruction::scalar_cases() {
        let a = &case.input;
        let mut value = a[1] as i16;
        let m = reconstruction::uv_matrix(a[0]);
        let (expected, error) = single_model(value, &m);
        assert_eq!(QuantizeSingle(&mut value, &m), error);
        assert_eq!(value, expected);
        assert!(error.abs() <= i32::from(m.q[0]));
    }
    let m = reconstruction::uv_matrix(4);
    let mut value = i16::MAX;
    QuantizeSingle(&mut value, &m);
    assert_eq!(value, i16::MIN); // narrowed 32768, without the block level cap
    let mut coeffs = [i16::MAX; 16];
    let mut levels = [0; 16];
    QuantizeBlock_C(&mut coeffs, &mut levels, &m);
    assert_eq!(levels, [2047; 16]);
}

// A spatial 2x2 grid with wider weighted rational arithmetic. Division of
// negative corrections and residuals rounds down, unlike chroma delta setup.
fn diffuse_model(
    tmp: &mut [[i16; 16]; 8],
    m: &VP8Matrix,
    top: DError,
    left: DError,
) -> [[i8; 3]; 2] {
    std::array::from_fn(|ch| {
        let mut error = [[0i32; 2]; 2];
        for y in 0..2 {
            for x in 0..2 {
                let above = if y == 0 {
                    i32::from(top[ch][x])
                } else {
                    error[y - 1][x]
                };
                let beside = if x == 0 {
                    i32::from(left[ch][y])
                } else {
                    error[y][x - 1]
                };
                let offset = ch * 4 + y * 2 + x;
                let correction = (7i64 * i64::from(above) + 8 * i64::from(beside)).div_euclid(8);
                let value = (i64::from(tmp[offset][0]) + correction) as i16;
                let (quantized, err) = single_model(value, m);
                tmp[offset][0] = quantized;
                error[y][x] = err;
            }
        }
        [error[0][1] as i8, error[1][0] as i8, error[1][1] as i8]
    })
}

#[test]
fn diffusion_feedback_rounding_and_selected_commit() {
    for case in reconstruction::dc_cases() {
        let a = &case.input;
        let (mut enc, mut it, mut actual) = reconstruction::dc_state(a);
        let mut expected = actual;
        let mut rd = reconstruction::seed_score();
        let mut expected_rd = rd.clone();
        let m = reconstruction::uv_matrix(a[0]);
        let before_top = enc.top_derr.clone().unwrap();
        let before_it = it.clone();
        expected_rd.derr = diffuse_model(&mut expected, &m, before_top[it.x], it.left_derr);
        CorrectDCValues(&it, &enc, &m, &mut actual, &mut rd);
        assert_eq!(actual, expected);
        assert_eq!(rd, expected_rd);
        assert_eq!(enc.top_derr.as_ref().unwrap(), &before_top);
        assert_eq!(it, before_it);
        StoreDiffusionErrors(&mut it, &mut enc, &rd);
        assert_eq!(enc.top_derr.as_ref().unwrap()[0], before_top[0]);
        for ch in 0..2 {
            let down = (3 * i32::from(rd.derr[ch][2])).div_euclid(4) as i8;
            assert_eq!(it.left_derr[ch], [rd.derr[ch][0], down]);
            assert_eq!(
                enc.top_derr.as_ref().unwrap()[1][ch],
                [rd.derr[ch][1], rd.derr[ch][2] - down]
            );
        }
    }
    let base = &reconstruction::dc_cases()[0];
    for err in -128i16..=127 {
        let (mut enc, mut it, _) = reconstruction::dc_state(&base.input);
        let mut rd = VP8ModeScore::default();
        rd.derr = [[7, -9, err as i8]; 2];
        StoreDiffusionErrors(&mut it, &mut enc, &rd);
        let left = i16::from(it.left_derr[0][1]);
        let top = i16::from(enc.top_derr.unwrap()[1][0][1]);
        assert_eq!(left + top, err);
        assert_eq!(left, (3 * err).div_euclid(4));
    }
}

fn quantize_model(input: &mut [i16; 16], m: &VP8Matrix) -> [i16; 16] {
    let mut raster = [0i16; 16];
    for i in 0..16 {
        let value = i64::from(input[i]);
        let magnitude = value.abs() + i64::from(m.sharpen[i]);
        let level = ((magnitude * i64::from(m.iq[i]) + i64::from(m.bias[i])) / 131072).min(2047);
        raster[i] = (if value < 0 { -level } else { level }) as i16;
        input[i] = (i64::from(raster[i]) * i64::from(m.q[i])) as i16;
    }
    let mut result = [0; 16];
    let mut n = 0;
    for diagonal in 0usize..7 {
        let mut indices: Vec<_> = (0..4)
            .filter_map(|y| diagonal.checked_sub(y).filter(|&x| x < 4).map(|x| (y, x)))
            .collect();
        if diagonal % 2 == 0 {
            indices.reverse();
        }
        for (y, x) in indices {
            result[n] = raster[4 * y + x];
            n += 1;
        }
    }
    result
}

const HADAMARD: [[i64; 4]; 4] = [[1, 1, 1, 1], [1, 1, -1, -1], [1, -1, -1, 1], [1, -1, 1, -1]];
fn wht_matrix(input: &[i16; 16], inverse: bool) -> [i16; 16] {
    std::array::from_fn(|i| {
        let (y, x) = (i / 4, i % 4);
        let mut sum = 0;
        for a in 0..4 {
            for b in 0..4 {
                sum += HADAMARD[y][a] * i64::from(input[4 * a + b]) * HADAMARD[x][b];
            }
        }
        (if inverse {
            (sum + 3).div_euclid(8)
        } else {
            sum.div_euclid(2)
        }) as i16
    })
}

fn independent_reconstruction(
    a: &[u32],
    enc: &VP8Encoder,
    it: &mut VP8EncIterator,
    rd: &mut VP8ModeScore,
    dst: &mut [u8],
) -> u32 {
    let m = &enc.dqm[a[2] as usize];
    let mut nz = 0;
    match a[0] {
        0 => {
            VP8MakeLuma16Preds(it, enc);
            let reference = &it.yuv_p[VP8I16ModeOffsets[a[4] as usize]..];
            let mut blocks = [[0i16; 16]; 16];
            for (n, block) in blocks.iter_mut().enumerate() {
                let offset = 4 * (n / 4) * BPS + 4 * (n % 4);
                FTransform_C(&it.yuv_in[offset..], &reference[offset..], block);
            }
            let dc = std::array::from_fn(|i| blocks[i][0]);
            let mut dc = wht_matrix(&dc, false);
            rd.y_dc_levels = quantize_model(&mut dc, &m.y2);
            nz |= u32::from(rd.y_dc_levels.iter().any(|&v| v != 0)) << 24;
            let dc = wht_matrix(&dc, true);
            for (n, block) in blocks.iter_mut().enumerate() {
                block[0] = 0;
                rd.y_ac_levels[n] = quantize_model(block, &m.y1);
                nz |= u32::from(rd.y_ac_levels[n].iter().any(|&v| v != 0)) << n;
                block[0] = dc[n];
                let offset = 4 * (n / 4) * BPS + 4 * (n % 4);
                ITransform_C(&reference[offset..], block, &mut dst[offset..], false);
            }
        }
        2 => {
            VP8MakeChroma8Preds(it, enc);
            let reference = &it.yuv_p[VP8UVModeOffsets[a[4] as usize]..];
            let mut blocks = [[0i16; 16]; 8];
            let offset = |n: usize| n / 4 * 8 + (n % 4) / 2 * 4 * BPS + (n % 2) * 4;
            for (n, block) in blocks.iter_mut().enumerate() {
                FTransform_C(&it.yuv_in[16 + offset(n)..], &reference[offset(n)..], block);
            }
            if let Some(top) = &enc.top_derr {
                rd.derr = diffuse_model(&mut blocks, &m.uv, top[it.x], it.left_derr);
            }
            for (n, block) in blocks.iter_mut().enumerate() {
                rd.uv_levels[n] = quantize_model(block, &m.uv);
                nz |= u32::from(rd.uv_levels[n].iter().any(|&v| v != 0)) << (16 + n);
                ITransform_C(
                    &reference[offset(n)..],
                    block,
                    &mut dst[16 + offset(n)..],
                    false,
                );
            }
        }
        1 | 3 => {
            VP8IteratorStartI4(it, enc);
            if a[0] == 1 {
                for _ in 0..a[5] {
                    VP8IteratorRotateI4(it, dst);
                }
            }
            let count = if a[0] == 1 { 1 } else { 16 };
            for n in 0..count {
                let top = *it.i4_window();
                Intra4Preds_C(&mut it.yuv_p, &top);
                let mode = if count == 1 {
                    a[4] as usize
                } else {
                    (a[4] as usize + n) % 10
                };
                let reference = &it.yuv_p[VP8I4ModeOffsets[mode]..];
                let block = it.i4;
                let offset = 4 * (block / 4) * BPS + 4 * (block % 4);
                let mut coeffs = [0; 16];
                FTransform_C(&it.yuv_in[offset..], reference, &mut coeffs);
                rd.y_ac_levels[block] = quantize_model(&mut coeffs, &m.y1);
                let nonzero = u32::from(rd.y_ac_levels[block].iter().any(|&v| v != 0));
                nz |= nonzero << if count == 1 { 0 } else { block };
                ITransform_C(reference, &coeffs, &mut dst[offset..], false);
                if count > 1 {
                    VP8IteratorRotateI4(it, dst);
                }
            }
        }
        _ => unreachable!(),
    }
    nz
}

#[test]
fn candidate_levels_pixels_masks_and_state_match_independent_block_composition() {
    let mut zero = [0; 4];
    let mut nonzero = [0; 4];
    let mut clipped = [0usize; 2];
    for case in reconstruction::reconstruction_cases() {
        let a = &case.input;
        let (mut enc, mut it) = reconstruction::initialized(a);
        let before = it.clone();
        let original_nz = enc.nz.clone();
        let original_top = enc.top_derr.clone();
        let original_mb = enc.mb_info.clone();
        let original_preds = enc.preds.clone();
        let mut model_it = it.clone();
        let mut expected_rd = reconstruction::seed_score();
        let mut rd = expected_rd.clone();
        let mut actual = [0xa5; 512];
        let mut expected = actual;
        let expected_nz =
            independent_reconstruction(a, &enc, &mut model_it, &mut expected_rd, &mut expected);
        let actual_nz = reconstruction::run(a, &mut enc, &mut it, &mut rd, &mut actual);
        assert_eq!(actual_nz, expected_nz, "mask {:?}", &a[..8]);
        assert_eq!(rd, expected_rd, "levels {:?}", &a[..8]);
        assert_eq!(actual, expected, "pixels {:?}", &a[..8]);
        assert_eq!(it, model_it);
        assert_eq!(it.yuv_in, before.yuv_in);
        assert_eq!(enc.nz, original_nz);
        assert_eq!(enc.top_derr, original_top);
        assert_eq!(enc.mb_info, original_mb);
        assert_eq!(enc.preds, original_preds);
        for (i, &v) in actual.iter().enumerate() {
            let x = i % BPS;
            let y = i / BPS;
            let touched = match a[0] {
                0 | 3 => x < 16,
                2 => (16..32).contains(&x) && y < 8,
                1 => {
                    let block = a[5] as usize;
                    (block % 4 * 4..block % 4 * 4 + 4).contains(&x)
                        && (block / 4 * 4..block / 4 * 4 + 4).contains(&y)
                }
                _ => false,
            };
            if !touched {
                assert_eq!(v, 0xa5);
            } else {
                clipped[0] += usize::from(v == 0);
                clipped[1] += usize::from(v == 255);
            }
        }
        if actual_nz == 0 {
            zero[a[0] as usize] += 1;
        } else {
            nonzero[a[0] as usize] += 1;
        }
    }
    assert!(zero.iter().all(|&v| v > 0));
    assert!(nonzero.iter().all(|&v| v > 0));
    assert!(clipped.iter().all(|&v| v > 0));
    eprintln!("zero masks {zero:?}, nonzero masks {nonzero:?}; clipped pixels {clipped:?}");
}

#[test]
fn maximum_edge_uses_only_selected_dc_coefficients_and_never_decreases() {
    for value in -32768i32..=32767 {
        for position in [1, 2, 4] {
            let mut dc = [0; 16];
            dc[position] = value as i16;
            let mut s = VP8SegmentInfo {
                max_edge: 17,
                ..Default::default()
            };
            StoreMaxDelta(&mut s, &dc);
            assert_eq!(s.max_edge, value.abs().max(17));
            StoreMaxDelta(&mut s, &[0; 16]);
            assert_eq!(s.max_edge, value.abs().max(17));
        }
    }
    for position in (0..16).filter(|i| ![1, 2, 4].contains(i)) {
        let mut dc = [0; 16];
        dc[position] = i16::MIN;
        let mut s = VP8SegmentInfo::default();
        StoreMaxDelta(&mut s, &dc);
        assert_eq!(s, VP8SegmentInfo::default());
    }
}

#[test]
fn reconstruction_accepts_minimum_strided_destination_buffers() {
    for kind in 0..3 {
        for mode in 0..if kind == 1 { 10 } else { 4 } {
            let a = reconstruction::request(kind, 127, 7, mode, 6).input;
            let (mut enc, mut it) = reconstruction::initialized(&a);
            let mut rd = reconstruction::seed_score();
            let mut full = [0xa5; 512];
            let expected = reconstruction::run(&a, &mut enc, &mut it, &mut rd, &mut full);
            let mut candidate = reconstruction::seed_score();
            let (enc, mut it) = reconstruction::initialized(&a);
            let mut minimal = [0xa5; 15 * BPS + 16];
            let (mask, len) = match kind {
                0 => {
                    VP8MakeLuma16Preds(&mut it, &enc);
                    (
                        ReconstructIntra16(
                            &mut it,
                            &enc,
                            &mut candidate,
                            &mut minimal,
                            mode as usize,
                        ),
                        minimal.len(),
                    )
                }
                2 => {
                    VP8MakeChroma8Preds(&mut it, &enc);
                    let len = 16 + 7 * BPS + 16;
                    (
                        ReconstructUV(
                            &it,
                            &enc,
                            &mut candidate,
                            &mut minimal[16..len],
                            mode as usize,
                        ),
                        len,
                    )
                }
                1 => {
                    VP8IteratorStartI4(&mut it, &enc);
                    for _ in 0..a[5] {
                        VP8IteratorRotateI4(&mut it, &full);
                    }
                    MakeIntra4Preds(&mut it);
                    let offset = VP8Scan[it.i4];
                    let len = 3 * BPS + 4;
                    let mut block = [0xa5; 3 * BPS + 4];
                    let mask = ReconstructIntra4(
                        &it,
                        &enc,
                        &mut candidate.y_ac_levels[it.i4],
                        &it.yuv_in[offset..offset + len],
                        &mut block,
                        mode as usize,
                    );
                    assert_eq!(block, full[offset..offset + len]);
                    assert_eq!(candidate, rd);
                    assert_eq!(u32::from(mask), expected);
                    continue;
                }
                _ => unreachable!(),
            };
            assert_eq!(mask, expected);
            assert_eq!(candidate, rd);
            assert_eq!(&minimal[..len], &full[..len]);
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn reconstruction_matches_pinned_reference() {
    let cases = reconstruction::cases();
    native::parallel_cases("vp8-reconstruct", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!("{} exact reconstruction/diffusion requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn reconstruction_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = reconstruction::fixture_cases();
    let outputs = native::run("vp8-reconstruct-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C reconstruction and chroma DC diffusion.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_reconstruct.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_reconstruct.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
