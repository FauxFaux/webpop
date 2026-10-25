#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::sharpyuv::{sharpyuv_dsp::*, sharpyuv_gamma::*, *};

#[test]
fn sharpyuv_committed_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/sharpyuv.txt"));
    let cases = support::sharpyuv::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn sharpyuv_gray_and_gamma_independent_checks() {
    for v in 0..=71501 {
        assert_eq!(SharpYuvRGBToGray(v, v, v), v as i32);
    }
    let t = SharpYuvInitGammaTables();
    assert_eq!(
        (
            t.gamma_to_linear[0],
            t.gamma_to_linear[1024],
            t.gamma_to_linear[1025]
        ),
        (0, 65536, 65536)
    );
    assert_eq!(
        (
            t.linear_to_gamma[0],
            t.linear_to_gamma[512],
            t.linear_to_gamma[513]
        ),
        (0, 65536, 65536)
    );
    assert!(t.gamma_to_linear.windows(2).all(|p| p[0] <= p[1]));
    assert!(t.linear_to_gamma.windows(2).all(|p| p[0] <= p[1]));
    let tf = SharpYuvTransferFunctionType::kSharpYuvTransferFunctionSrgb;
    for depth in [8, 10, 12, 14] {
        let mut last = 0;
        for v in 0..=65536 {
            let lo = t.linear_to_gamma[(v / 128) as usize] >> (16 - depth);
            let hi = t.linear_to_gamma[(v / 128) as usize + 1] >> (16 - depth);
            let expected = (lo * (128 - v % 128) + hi * (v % 128) + 64) / 128;
            let actual = SharpYuvLinearToGamma(v, depth, tf) as u32;
            assert_eq!(actual, expected);
            assert!(actual >= last);
            last = actual;
        }
    }
    assert_eq!(Roundf(-2.5), -3.0);
    assert_eq!(Roundf(2.5), 3.0);
    assert!(SharpYuvTransferFunctionType::try_from(2).is_err());
    assert!(
        SharpYuvComputeConversionMatrix(&SharpYuvColorSpace {
            kr: f32::NAN,
            kb: 0.1,
            bit_depth: 8,
            range: SharpYuvRange::kSharpYuvRangeFull,
        })
        .is_err()
    );
}

fn view8<'a>(
    data: &'a [u8],
    width: u32,
    height: u32,
    step: usize,
    stride: usize,
) -> SharpYuvRgb<'a> {
    SharpYuvRgb::U8(SharpYuvRgbView {
        r: data,
        g: data,
        b: data,
        width,
        height,
        step,
        stride,
    })
}

#[test]
fn sharpyuv_validates_before_reading_or_allocating() {
    use SharpYuvError::*;
    let convert =
        |input, rgb, yuv, m: &SharpYuvConversionMatrix| SharpYuvConvert(input, rgb, yuv, m);
    let matrix = &MATRICES[0];
    for (input, rgb, yuv, error) in [
        (view8(&[], 0, 1, 1, 1), 8, 8, InvalidDimensions),
        (view8(&[], 1, 0, 1, 1), 8, 8, InvalidDimensions),
        (
            view8(&[], i32::MAX as u32, 1, 1, i32::MAX as usize),
            8,
            8,
            InvalidDimensions,
        ),
        (view8(&[], 1, u32::MAX, 1, 1), 8, 8, InvalidDimensions),
        (view8(&[], 1, 1, 1, 1), 9, 8, InvalidRgbBitDepth),
        (view8(&[], 1, 1, 1, 1), 8, 16, InvalidYuvBitDepth),
        (view8(&[], 1, 1, 1, 1), 10, 8, InvalidStorage),
        (view8(&[], 1, 1, 0, 1), 8, 8, InvalidStride),
        (view8(&[], 2, 1, 2, 3), 8, 8, InvalidStride),
        (view8(&[], 1, 1, 1, 1), 8, 8, InputTooShort),
        (view8(&[0; 3], 2, 2, 1, 2), 8, 8, InputTooShort),
        (view8(&[], 2, 1, usize::MAX, usize::MAX), 8, 8, SizeOverflow),
        (view8(&[], 1, 3, 1, usize::MAX), 8, 8, SizeOverflow),
    ] {
        assert_eq!(convert(input, rgb, yuv, matrix), Err(error));
    }
    let storage16 = SharpYuvRgb::U16(SharpYuvRgbView {
        r: &[0],
        g: &[0],
        b: &[0],
        width: 1,
        height: 1,
        step: 1,
        stride: 1,
    });
    assert_eq!(convert(storage16, 8, 8, matrix), Err(InvalidStorage));
    assert!(convert(view8(&[0], 1, 1, 1, usize::MAX), 8, 8, matrix).is_ok());
    let mut invalid = *matrix;
    invalid.rgb_to_y[0] = i32::MAX;
    assert_eq!(
        convert(view8(&[0], 1, 1, 1, 1), 8, 12, &invalid),
        Err(InvalidMatrix)
    );
    invalid = *matrix;
    invalid.rgb_to_y[3] = i32::MAX;
    assert_eq!(
        convert(view8(&[0], 1, 1, 1, 1), 8, 8, &invalid),
        Err(InvalidMatrix)
    );
    for (kr, kb, depth) in [
        (f32::NAN, 0.0, 8),
        (0.1, f32::INFINITY, 8),
        (-0.1, 0.2, 8),
        (0.5, 0.5, 8),
        (0.2, 0.1, 16),
    ] {
        assert_eq!(
            SharpYuvComputeConversionMatrix(&SharpYuvColorSpace {
                kr,
                kb,
                bit_depth: depth,
                range: SharpYuvRange::kSharpYuvRangeFull
            }),
            Err(InvalidColorSpace)
        );
    }
    for tf in [0, 2, 3, 19, u32::MAX] {
        assert_eq!(
            SharpYuvTransferFunctionType::try_from(tf),
            Err(InvalidTransferFunction)
        );
    }
}

#[test]
fn sharpyuv_layouts_edges_defaults_and_depth_clamping() {
    for rgb in [8, 10, 12, 16] {
        for yuv in [8, 10, 12] {
            let mut case = support::sharpyuv::image(5, 7, rgb, yuv, 13, 0, 4, 1, 0);
            let expected = case.rust();
            // Same active pixels, arbitrary padding and interleaving step.
            let a = case.input.clone();
            let len = (a.len() - 20) / 3;
            let (w, h) = (5usize, 7usize);
            let mut layout = case.clone();
            layout.input[2] = 4;
            layout.input[3] = 4 * w as u32 + 3;
            layout.input.truncate(20);
            let stride = 4 * w + 3;
            for k in 0..3 {
                let mut data = vec![0x53; (h - 1) * stride + (w - 1) * 4 + 1];
                for row in 0..h {
                    for x in 0..w {
                        data[row * stride + x * 4] = a[20 + k * len + row * w + x];
                    }
                }
                layout.input.extend(data);
            }
            assert_eq!(layout.rust(), expected);
            case.input[7] = 1;
            assert_eq!(case.rust(), expected);
            // Explicit right/bottom replication gives the same cropped output.
            let mut expanded = case.clone();
            expanded.input[0] = 6;
            expanded.input[1] = 8;
            expanded.input[3] = 6;
            expanded.input.truncate(20);
            for k in 0..3 {
                for row in 0..8 {
                    for x in 0..6 {
                        expanded
                            .input
                            .push(a[20 + k * len + row.min(h - 1) * w + x.min(w - 1)]);
                    }
                }
            }
            let (_, p) = support::sharpyuv::convert_case(&case, |_| {});
            let (_, q) = support::sharpyuv::convert_case(&expanded, |_| {});
            match (p, q) {
                (SharpYuvOutput::U8(p), SharpYuvOutput::U8(q)) => {
                    assert_eq!(p.u, q.u);
                    assert_eq!(p.v, q.v);
                    for j in 0..h {
                        assert_eq!(p.y[j * w..j * w + w], q.y[j * 6..j * 6 + w]);
                    }
                }
                (SharpYuvOutput::U16(p), SharpYuvOutput::U16(q)) => {
                    assert_eq!(p.u, q.u);
                    assert_eq!(p.v, q.v);
                    for j in 0..h {
                        assert_eq!(p.y[j * w..j * w + w], q.y[j * 6..j * 6 + w]);
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    for depth in [10, 12] {
        let high = support::sharpyuv::image(3, 5, depth, 8, 13, 0, 5, 1, 0);
        let mut clipped = high.clone();
        for v in &mut clipped.input[20..] {
            *v = (*v).min((1 << depth) - 1);
        }
        assert_eq!(high.rust(), clipped.rust());
    }
    let a = support::sharpyuv::image(3, 5, 16, 8, 13, 0, 4, 1, 0);
    let mut truncated = a.clone();
    for v in &mut truncated.input[20..] {
        *v &= !3;
    }
    assert_eq!(a.rust(), truncated.rust());
}

#[test]
fn sharpyuv_rgba_alpha_is_ignored_and_calls_are_repeatable() {
    let mut pixels = vec![0u8; 4 * 9 * 5 + 1];
    for (i, v) in pixels[1..].iter_mut().enumerate() {
        *v = (i * 103) as u8;
    }
    let convert = |data: &[u8]| {
        SharpYuvConvert(
            SharpYuvRgb::U8(SharpYuvRgbView {
                r: &data[1..],
                g: &data[2..],
                b: &data[3..],
                width: 9,
                height: 5,
                step: 4,
                stride: 36,
            }),
            8,
            8,
            &MATRICES[0],
        )
        .unwrap()
    };
    let expected = convert(&pixels);
    for pixel in pixels[1..].chunks_exact_mut(4) {
        pixel[3] ^= 255;
    }
    assert_eq!(convert(&pixels), expected);
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(convert(&pixels), expected));
        }
    });
}

#[test]
fn sharpyuv_filters_and_srgb_paths_have_independent_checks() {
    use libwebpop::internal::sharpyuv::sharpyuv::{Filter2, ScaleDown};
    let tf = SharpYuvTransferFunctionType::kSharpYuvTransferFunctionSrgb;
    for a in [-32768, -16384, -1, 0, 1, 16383, 32767] {
        for b in [-32768, -1, 0, 1, 32767] {
            for w in [0, 1, 1023, 16383] {
                let expected = ((3i64 * i64::from(a) + i64::from(b) + 2).div_euclid(4)
                    + i64::from(w))
                .clamp(0, 16383);
                assert_eq!(i64::from(Filter2(a, b, w, 14)), expected);
            }
        }
    }
    for depth in [10, 12, 14] {
        let max = (1 << depth) - 1;
        for values in [
            [0, 0, 0, 0],
            [max, max, max, max],
            [0, max, max, 0],
            [1, 2, 3, 4],
        ] {
            let [a, b, c, d] = values;
            assert_eq!(
                ScaleDownSrgb_C(a, b, c, d, depth),
                ScaleDown(a, b, c, d, depth, tf)
            );
            let linear_sum: u32 = values
                .iter()
                .map(|&v| SharpYuvGammaToLinear(v, depth, tf))
                .sum();
            let expected = SharpYuvLinearToGamma((linear_sum + 2) / 4, depth, tf);
            assert_eq!(ScaleDownSrgb_C(a, b, c, d, depth), u32::from(expected));
        }
    }
    // Express the bilinear kernel as two rational weighted averages, and
    // round only once at the end. Include signed chroma and clipped luma.
    for (a, b) in [
        ([-32768, 32767], [32767, -32768]),
        ([-1, 0], [0, 1]),
        ([100, -50], [-77, 99]),
    ] {
        for y in [[0, 16383], [1, 2], [7000, 12000]] {
            let mut out = [0xa55a; 4];
            SharpYuvFilterRow(&a, &b, 1, &y, &mut out[1..], 14);
            let av = a.map(i64::from);
            let bv = b.map(i64::from);
            let horizontal_a = [3 * av[0] + av[1], av[0] + 3 * av[1]];
            let horizontal_b = [3 * bv[0] + bv[1], bv[0] + 3 * bv[1]];
            for i in 0..2 {
                let chroma = (3 * horizontal_a[i] + horizontal_b[i] + 8).div_euclid(16);
                assert_eq!(
                    i64::from(out[i + 1]),
                    (i64::from(y[i]) + chroma).clamp(0, 16383)
                );
            }
            assert_eq!((out[0], out[3]), (0xa55a, 0xa55a));
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharpyuv_leaves_match_pinned_reference() {
    let cases = support::sharpyuv::leaf_cases();
    native::parallel_cases("sharpyuv-leaves", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} SharpYUV leaf requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharpyuv_dsp_matches_pinned_reference() {
    let cases = support::sharpyuv::dsp_cases();
    native::parallel_cases("sharpyuv-dsp", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} exact SharpYUV DSP requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharpyuv_public_matches_pinned_reference() {
    let cases = support::sharpyuv::public_cases();
    native::parallel_cases("sharpyuv-public", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} exact SharpYUV public requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharpyuv_rows_and_iterations_match_pinned_reference() {
    let cases = support::sharpyuv::trace_cases();
    let reasons = native::parallel_cases("sharpyuv-trace", &cases, |case, output| {
        native::compare(case, output, &case.rust());
        if case.function != "SharpYuvTrace" {
            return 0;
        }
        let a = &case.input;
        let (w, h) = (((a[0] + 1) & !1) as usize, ((a[1] + 1) & !1) as usize);
        let stage = 3 + 2 * w * h + 6 * (w / 2) * (h / 2);
        let pixels = a[0] as usize * a[1] as usize
            + 2 * a[0].div_ceil(2) as usize * a[1].div_ceil(2) as usize;
        let snapshots = (output.len() - 1 - pixels) / stage;
        assert!((2..=5).contains(&snapshots));
        assert_eq!(output[1], u32::MAX);
        let mut prev = u64::MAX;
        for iter in 0..snapshots - 1 {
            let at = 1 + (iter + 1) * stage;
            assert_eq!(output[at], iter as u32);
            let diff = u64::from(output[at + 1]) + (u64::from(output[at + 2]) << 32);
            let reason = if diff < 3 * (w * h) as u64 {
                1
            } else if iter > 0 && diff > prev {
                2
            } else if iter == 3 {
                3
            } else {
                0
            };
            if iter + 2 < snapshots {
                assert_eq!(reason, 0);
            } else {
                assert_ne!(reason, 0);
                return reason;
            }
            prev = diff;
        }
        unreachable!()
    });
    for reason in 1..=3 {
        assert!(reasons.contains(&reason), "missing iteration exit {reason}");
    }
    eprintln!(
        "{} exact SharpYUV import/trace requests; all three iteration exits",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
#[ignore = "manual scalar SharpYUV timing and buffer baseline"]
fn sharpyuv_scalar_baseline() {
    use std::time::{Duration, Instant};
    for depth in [8, 12] {
        let c = support::sharpyuv::real_image(depth);
        let mut timed = c.clone();
        timed.function = "SharpYuvTimedConvert".into();
        let outputs = native::run(&format!("sharpyuv-baseline-{depth}"), &vec![timed; 21]);
        let expected = c.rust();
        for out in &outputs {
            assert_eq!(out[0], 1);
            assert_eq!(&out[2..], &expected[1..]);
        }
        let c_us: f64 = outputs[1..]
            .iter()
            .map(|out| f64::from(out[1]))
            .sum::<f64>()
            / 20.0;
        let mut rust_time = Duration::ZERO;
        let a = &c.input;
        let options = SharpYuvOptionsInit(&SharpYuvConversionMatrix {
            rgb_to_y: std::array::from_fn(|i| a[8 + i] as i32),
            rgb_to_u: std::array::from_fn(|i| a[12 + i] as i32),
            rgb_to_v: std::array::from_fn(|i| a[16 + i] as i32),
        });
        support::sharpyuv::with_input(&c, |input| {
            SharpYuvConvertWithOptions(input, depth, depth, &options).unwrap();
            for _ in 0..20 {
                let started = Instant::now();
                std::hint::black_box(
                    SharpYuvConvertWithOptions(input, depth, depth, &options).unwrap(),
                );
                rust_time += started.elapsed();
            }
        });
        // Plane/scratch backing storage only; excludes inputs, tables and allocator overhead.
        let working = 7 * 128 * 128 + 19 * 128;
        let output_samples = 128 * 128 + 2 * 64 * 64;
        eprintln!(
            "SharpYUV {depth}-bit sRGB 128x128: C {c_us:.1} us, Rust {:.1} us; peak plane/scratch bytes C {}, Rust {}",
            rust_time.as_secs_f64() * 1e6 / 20.0,
            working + output_samples * if depth == 8 { 1 } else { 2 },
            working + 2 * output_samples
        );
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharpyuv_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = support::sharpyuv::fixture_cases();
    let outputs = native::run("sharpyuv-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar SharpYUV C.\n# upstream={} seed={:08x}\n# See ../reference/SHARPYUV.md.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sharpyuv.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/sharpyuv.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
}
