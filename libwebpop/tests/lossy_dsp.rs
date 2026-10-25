#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::{
    dsp::enc::*,
    enc::{quant_enc::*, vp8i_enc::*},
};
use support::vp8_dsp as dsp;

#[test]
fn committed_vp8_dsp_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_dsp.txt"));
    let cases = dsp::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected, "{}", case.function);
    }
}

#[test]
fn quantization_matches_wide_fixed_point_arithmetic_and_zigzag() {
    for case in dsp::quantization_cases() {
        let a = &case.input;
        let actual = case.rust();
        match case.function.as_str() {
            "BIAS" => assert_eq!(actual, [(u64::from(a[0]) * 512 % (1u64 << 32)) as u32]),
            "QUANTDIV" => {
                let numerator =
                    (u64::from(a[0]) * u64::from(a[1]) + u64::from(a[2])) % (1u64 << 32);
                assert_eq!(actual, [(numerator / 131072) as u32]);
            }
            "VP8Quantize" => {
                let m = dsp::matrix(&a[1..81]);
                let count = a.len() - 81;
                let mut reconstructed = vec![0; count];
                let mut levels = vec![0; count];
                let mut mask = 0;
                for block in 0..count / 16 {
                    for j in 0..16 {
                        let original = a[81 + 16 * block + j] as i32;
                        let magnitude = original.unsigned_abs() + u32::from(m.sharpen[j]);
                        let numerator = (u64::from(magnitude) * u64::from(m.iq[j])
                            + u64::from(m.bias[j]))
                            % (1u64 << 32);
                        let level = if magnitude <= m.zthresh[j] {
                            0
                        } else {
                            (numerator / 131072).min(2047) as i32
                        };
                        let level = if original < 0 { -level } else { level };
                        let dequantized = level * i32::from(m.q[j]);
                        // The synthetic corpus keeps C's i16 assignment in range.
                        assert!(i16::try_from(dequantized).is_ok());
                        reconstructed[16 * block + j] = dequantized as u32;
                        let zigzag = kZigzag.iter().position(|&v| v == j).unwrap();
                        levels[16 * block + zigzag] = level as u32;
                        if level != 0 {
                            mask |= 1 << block;
                        }
                    }
                }
                let mut expected = vec![mask];
                expected.extend(reconstructed);
                expected.extend(levels);
                assert_eq!(actual, expected);
            }
            "VP8DspTables" => {}
            _ => unreachable!(),
        }
    }
}

#[test]
fn quantizer_threshold_sharpening_limits_and_paired_masks() {
    let mut matrix = dsp::uniform_matrix(8, BIAS(128), 0);
    for (value, expected) in [(3, 0), (4, 1), (-3, 0), (-4, -1)] {
        let mut input = [value; 16];
        let mut levels = [999; 16];
        assert_eq!(
            QuantizeBlock_C(&mut input, &mut levels, &matrix),
            expected != 0
        );
        assert_eq!(levels, [expected; 16]);
        assert_eq!(input, [expected * 8; 16]);
    }
    matrix.sharpen.fill(1);
    let mut input = [-3; 16];
    let mut levels = [0; 16];
    assert!(QuantizeBlock_C(&mut input, &mut levels, &matrix));
    assert_eq!(levels, [-1; 16]);
    assert_eq!(input, [-8; 16]);
    matrix.iq.fill(u16::MAX);
    matrix.zthresh.fill(0);
    matrix.sharpen.fill(u16::MAX);
    let mut input = [i16::MIN; 16];
    assert!(QuantizeBlock_C(&mut input, &mut levels, &matrix));
    assert_eq!(levels, [-2047; 16]);
    assert_eq!(input, [-16376; 16]);
    matrix.iq.fill(0);
    matrix.bias.fill(0);
    assert!(!QuantizeBlock_C(&mut input, &mut levels, &matrix));
    assert_eq!(levels, [0; 16]);
    assert_eq!(input, [0; 16]);
    let matrix = dsp::uniform_matrix(8, BIAS(128), 0);
    for mask in 0..4 {
        let mut input = [0; 32];
        let mut levels = [999; 32];
        if mask & 1 != 0 {
            input[0] = 8;
        }
        if mask & 2 != 0 {
            input[31] = -8;
        }
        assert_eq!(Quantize2Blocks_C(&mut input, &mut levels, &matrix), mask);
        assert_eq!(levels[0], if mask & 1 != 0 { 1 } else { 0 });
        assert_eq!(levels[31], if mask & 2 != 0 { -1 } else { 0 });
    }
}

fn spectral_energy(pixels: &[u32], stride: usize, x: usize, y: usize, weights: &[u32]) -> i64 {
    const H: [[i64; 4]; 4] = [[1, 1, 1, 1], [1, 1, -1, -1], [1, -1, -1, 1], [1, -1, 1, -1]];
    let mut energy = 0;
    for v in 0..4 {
        for u in 0..4 {
            let mut coefficient = 0;
            for r in 0..4 {
                for c in 0..4 {
                    coefficient += H[v][r] * i64::from(pixels[(y + r) * stride + x + c]) * H[c][u];
                }
            }
            energy += i64::from(weights[v * 4 + u]) * coefficient.abs();
        }
    }
    energy
}

#[test]
fn metrics_match_direct_pixel_sums_and_hadamard_products() {
    for case in dsp::metric_cases() {
        let a = &case.input;
        let op = a[0] as usize;
        let (w, h) = dsp::DIMENSIONS[op];
        let pixels = &a[if op == 4 || op == 5 { 17 } else { 1 }..];
        let actual = case.rust();
        let expected = match op {
            0..=3 => vec![
                pixels[..w * h]
                    .iter()
                    .zip(&pixels[w * h..])
                    .map(|(&p, &q)| (i64::from(p) - i64::from(q)).pow(2) as u32)
                    .sum(),
            ],
            4 | 5 => {
                let mut distortion = 0;
                for y in (0..h).step_by(4) {
                    for x in (0..w).step_by(4) {
                        let src = spectral_energy(pixels, w, x, y, &a[1..17]);
                        let dst = spectral_energy(&pixels[w * h..], w, x, y, &a[1..17]);
                        assert!(src <= i64::from(i32::MAX) && dst <= i64::from(i32::MAX));
                        distortion += ((src - dst).abs() / 32) as u32;
                    }
                }
                vec![distortion]
            }
            6 => (0..4)
                .map(|block| {
                    (0..4)
                        .flat_map(|y| (0..4).map(move |x| pixels[y * w + 4 * block + x]))
                        .sum()
                })
                .collect(),
            7 | 8 => {
                let mut expected = vec![0xa5; h * BPS];
                for y in 0..h {
                    expected[y * BPS..y * BPS + w].copy_from_slice(&pixels[y * w..y * w + w]);
                }
                expected
            }
            _ => unreachable!(),
        };
        assert_eq!(actual, expected, "metric {op}");
    }
    // Each 4×4 impulse has spectral energy 16, so each tile rounds to zero.
    // Rounding after adding two tiles would incorrectly produce one.
    let mut impulses = [0; 15 * BPS + 16];
    impulses[0] = 1;
    impulses[4] = 1;
    assert_eq!(Disto16x16_C(&impulses, &[0; 15 * BPS + 16], &[1; 16]), 0);
}

#[test]
fn histogram_empty_state_constant_residual_bins_and_signed_distributions() {
    let mut histo = VP8Histogram {
        max_value: 999,
        last_non_zero: 999,
    };
    CollectHistogram_C(&[], &[], 24, 24, &mut histo);
    assert_eq!(
        histo,
        VP8Histogram {
            max_value: 0,
            last_non_zero: 1
        }
    );
    for delta in -255i32..=255 {
        let src = vec![delta.max(0) as u8; 16 * BPS];
        let pred = vec![(-delta).max(0) as u8; 16 * BPS];
        for (start, end) in [(0, 16), (16, 24), (0, 0)] {
            CollectHistogram_C(&src, &pred, start, end, &mut histo);
            let blocks = (end - start) as i32;
            let expected = if blocks == 0 {
                VP8Histogram {
                    max_value: 0,
                    last_non_zero: 1,
                }
            } else {
                VP8Histogram {
                    max_value: blocks * if delta == 0 { 16 } else { 15 },
                    last_non_zero: delta.abs().min(31),
                }
            };
            assert_eq!(histo, expected, "delta {delta}, range {start}..{end}");
        }
    }
    for case in dsp::histogram_cases()
        .into_iter()
        .filter(|c| c.function == "VP8SetHistogramData")
    {
        let bins: Vec<_> = case.input.iter().map(|&v| v as i32).collect();
        let max = bins.iter().copied().max().unwrap().max(0);
        let last = bins.iter().rposition(|&v| v > 0).unwrap_or(1);
        assert_eq!(case.rust(), [max as u32, last as u32]);
    }
}

#[test]
fn scans_cover_tiles_and_zigzag_follows_alternating_diagonals() {
    let mut expected = Vec::new();
    for diagonal in 0..7i32 {
        let mut xs: Vec<_> = (0..4)
            .filter(|&x| (0..4).contains(&(diagonal - x)))
            .collect();
        if diagonal % 2 == 1 {
            xs.reverse();
        }
        expected.extend(xs.into_iter().map(|x| (4 * (diagonal - x) + x) as usize));
    }
    assert_eq!(kZigzag.as_slice(), expected);
    assert_eq!(VP8Scan.as_slice(), &VP8DspScan[..16]);
    assert_eq!(VP8ScanUV.as_slice(), &VP8DspScan[16..]);
    for (scans, width, height) in [(&VP8Scan[..], 16, 16), (&VP8ScanUV[..], 16, 8)] {
        let mut coverage = vec![0; height * BPS];
        for &offset in scans {
            for y in 0..4 {
                for x in 0..4 {
                    coverage[offset + y * BPS + x] += 1;
                }
            }
        }
        for y in 0..height {
            for x in 0..BPS {
                assert_eq!(coverage[y * BPS + x], u8::from(x < width));
            }
        }
    }
}

#[test]
fn copy_exact_minimum_slices_preserves_padding_and_neighbors() {
    for (w, h, copy) in [
        (4, 4, Copy4x4_C as fn(&[u8], &mut [u8])),
        (16, 8, Copy16x8_C),
    ] {
        let size = (h - 1) * BPS + w;
        let src: Vec<_> = (0..size + 1).map(|i| i as u8).collect();
        let mut dst = vec![0xa5; size + 7];
        copy(&src[1..], &mut dst[3..3 + size]);
        for (i, &value) in dst.iter().enumerate() {
            let expected = if i >= 3 && i < 3 + size && (i - 3) % BPS < w {
                src[i - 2]
            } else {
                0xa5
            };
            assert_eq!(value, expected);
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_dsp_matches_pinned_scalar_reference() {
    let cases = dsp::cases();
    native::parallel_cases("vp8-dsp", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} exact VP8 DSP/quantizer comparisons", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_dsp_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = dsp::fixture_cases();
    let outputs = native::run("vp8-dsp-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 quantizers, scans, metrics, histograms and copies.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lossy_dsp.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_dsp.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
