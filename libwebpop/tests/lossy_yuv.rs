#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::{
    LosslessEncodeError,
    dsp::yuv::*,
    enc::{picture_csp_enc::*, picture_enc::WebPYUVAPicture, picture_tools_enc::*},
};
use support::vp8_yuv as yuv;

#[test]
fn committed_yuv_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_yuv.txt"));
    let cases = yuv::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn gamma_interpolation_has_fixed_endpoints_monotonicity_and_exact_rounding() {
    let t = WebPInitGammaTables();
    assert_eq!((t.gamma_to_linear[0], t.gamma_to_linear[255]), (0, 4095));
    assert!(t.gamma_to_linear.windows(2).all(|p| p[0] < p[1]));
    assert!(t.linear_to_gamma.windows(2).all(|p| p[0] < p[1]));
    let mut previous = 0;
    for v in 0..=16380u32 {
        let cell = (v / 512) as usize;
        let x = i64::from(v % 512);
        let lo = i64::from(t.linear_to_gamma[cell]);
        let hi = i64::from(t.linear_to_gamma[cell + 1]);
        let expected = (lo * 512 + (hi - lo) * x + 64) / 128;
        let actual = LinearToGamma(v, 0);
        assert_eq!(i64::from(actual), expected);
        assert!(actual >= previous && actual <= 1020);
        previous = actual;
    }
    assert_eq!(previous, 1020);
    for a in 1..=1020 {
        let inv = u64::from(kInvAlpha[a]);
        assert!(inv * a as u64 <= 524288);
        assert!((inv + 1) * a as u64 > 524288);
    }
}

#[test]
fn luma_and_chroma_neutrals_primary_colors_and_clipping() {
    assert_eq!(VP8RGBToY(0, 0, 0, YUV_HALF), 16);
    assert_eq!(VP8RGBToY(255, 255, 255, YUV_HALF), 235);
    for (rgb, expected) in [
        ([255, 0, 0], [82, 90, 240]),
        ([0, 255, 0], [145, 54, 34]),
        ([0, 0, 255], [41, 240, 110]),
    ] {
        let [r, g, b] = rgb;
        assert_eq!(
            [
                VP8RGBToY(r, g, b, YUV_HALF),
                VP8RGBToU(4 * r, 4 * g, 4 * b, 4 * YUV_HALF),
                VP8RGBToV(4 * r, 4 * g, 4 * b, 4 * YUV_HALF)
            ],
            expected
        );
    }
    for v in 0..=1020 {
        assert_eq!(VP8RGBToU(v, v, v, 4 * YUV_HALF), 128);
        assert_eq!(VP8RGBToV(v, v, v, 4 * YUV_HALF), 128);
    }
    for value in [
        -100_000_000,
        -33_685_505,
        -33_685_504,
        -1,
        0,
        33_161_215,
        33_161_216,
        100_000_000,
    ] {
        let wide =
            (i64::from(value) + i64::from(4 * YUV_HALF) + (128i64 << 18)).div_euclid(1 << 18);
        assert_eq!(
            i64::from(VP8ClipUV(value, 4 * YUV_HALF)),
            wide.clamp(0, 255)
        );
    }
}

#[test]
fn conversion_preserves_padding_and_replicates_odd_edges() {
    for w in [1, 3, 7, 8, 9, 16, 17] {
        for h in [1, 2, 3, 8, 9, 17] {
            for pattern in 0..12 {
                let case = yuv::image(w, h, 7, pattern, false);
                let p = yuv::picture(&case);
                let (ew, eh) = ((w + 1) & !1, (h + 1) & !1);
                let mut expanded = Vec::new();
                for y in 0..eh {
                    for x in 0..ew {
                        let i = (y.min(h - 1) * case.input[2] + 4 * x.min(w - 1)) as usize + 4;
                        expanded.extend(case.input[i..i + 4].iter().map(|&v| v as u8));
                    }
                }
                let q = import_rgba_to_yuva(&expanded, ew, eh, ew as usize * 4).unwrap();
                assert_eq!(p.u, q.u);
                assert_eq!(p.v, q.v);
                for row in 0..h as usize {
                    assert_eq!(
                        p.y[row * w as usize..(row + 1) * w as usize],
                        q.y[row * ew as usize..row * ew as usize + w as usize]
                    );
                    if let (Some(a), Some(b)) = (&p.a, &q.a) {
                        assert_eq!(
                            a[row * w as usize..(row + 1) * w as usize],
                            b[row * ew as usize..row * ew as usize + w as usize]
                        );
                    }
                }
                let before = p.clone();
                let mut cleaned = p;
                WebPCleanupTransparentArea(&mut cleaned);
                assert_eq!(cleaned.a, before.a);
                if let Some(alpha) = &before.a {
                    for (i, &a) in alpha.iter().enumerate() {
                        if a != 0 {
                            assert_eq!(cleaned.y[i], before.y[i]);
                        }
                    }
                } else {
                    assert_eq!(cleaned, before);
                }
            }
        }
    }
    let rgb = [5, 23, 251, 91];
    let mut dst = [0xa5; 7];
    ConvertRGBToY_C(&rgb, &mut dst[2..], 1, 4);
    assert_eq!(
        dst,
        [
            0xa5,
            0xa5,
            VP8RGBToY(5, 23, 251, YUV_HALF),
            0xa5,
            0xa5,
            0xa5,
            0xa5
        ]
    );
    let mut u = [0xa5; 5];
    let mut v = u;
    WebPConvertRGBA32ToUV_C(&[0, 0, 0, 0xbeef], &mut u[1..], &mut v[2..], 1);
    assert_eq!(u, [0xa5, 128, 0xa5, 0xa5, 0xa5]);
    assert_eq!(v, [0xa5, 0xa5, 128, 0xa5, 0xa5]);
}

#[test]
fn alpha_weighting_ignores_hidden_rgb_but_zero_alpha_uses_unweighted_fallback() {
    // All RGB bytes are 5, but alpha sum 3 gives reciprocal product 703
    // rather than the exact quotient 704. Gamma interpolation yields 19
    // rather than 20; replacing the reciprocal with division changes C output.
    assert_eq!(
        yuv::reciprocal_boundary_case().rust(),
        [19, 19, 19, 3, 128, 128]
    );
    assert_eq!(LinearToGamma(704, 0), 20);
    let mut a = vec![255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let p = import_rgba_to_yuva(&a, 2, 2, 8).unwrap();
    for i in [4, 8, 12] {
        a[i..i + 3].fill(255);
    }
    let q = import_rgba_to_yuva(&a, 2, 2, 8).unwrap();
    assert_eq!(p.u, q.u);
    assert_eq!(p.v, q.v);
    // Luma still describes the original RGB, independently of alpha.
    assert_ne!(p.y, q.y);
    a[3] = 0;
    let zero = import_rgba_to_yuva(&a, 2, 2, 8).unwrap();
    for i in [3, 7, 11, 15] {
        a[i] = 255;
    }
    let opaque = import_rgba_to_yuva(&a, 2, 2, 8).unwrap();
    assert_eq!(zero.y, opaque.y);
    assert_eq!(zero.u, opaque.u);
    assert_eq!(zero.v, opaque.v);
    assert_ne!(p.u, zero.u);
    assert!(opaque.a.is_none());
    // The scalar RGB primitive must leave the fourth accumulator untouched.
    let mut tmp = [0xbeef; 4];
    WebPAccumulateRGB(&a, &a[1..], &a[2..], 4, 8, &mut tmp, 2);
    assert_eq!(tmp[3], 0xbeef);
}

#[test]
fn cleanup_uses_floor_mean_and_resets_transparent_runs_at_visible_blocks_and_rows() {
    let width = 41usize;
    let height = 17usize;
    let uv_stride = width.div_ceil(2);
    let mut p = WebPYUVAPicture {
        width: width as u32,
        height: height as u32,
        y: (0..width * height).map(|i| (i % 251) as u8).collect(),
        u: (0..uv_stride * height.div_ceil(2))
            .map(|i| i as u8)
            .collect(),
        v: (0..uv_stride * height.div_ceil(2))
            .map(|i| (255 - i % 256) as u8)
            .collect(),
        a: Some(vec![0; width * height]),
    };
    // The third 8x8 block interrupts each run; tiny nonzero alpha counts too.
    let a = p.a.as_mut().unwrap();
    a[16] = 1;
    a[17] = 255;
    p.y[16] = 10;
    p.y[17] = 13;
    let before = p.clone();
    WebPCleanupTransparentArea(&mut p);
    for row in 0..8 {
        for x in 0..16 {
            assert_eq!(p.y[row * width + x], before.y[0]);
        }
    }
    assert_eq!(p.y[16], 10);
    assert_eq!(p.y[17], 13);
    assert_eq!(p.y[18], 11);
    for row in 0..8 {
        for x in 24..40 {
            assert_eq!(p.y[row * width + x], before.y[24]);
        }
    }
    for row in 8..16 {
        for x in 0..40 {
            assert_eq!(p.y[row * width + x], before.y[8 * width]);
        }
    }
    // Fully transparent partial edges remain untouched, including chroma.
    for row in 0..height {
        assert_eq!(p.y[row * width + 40], before.y[row * width + 40]);
    }
    assert_eq!(&p.y[16 * width..], &before.y[16 * width..]);
    for row in 0..height.div_ceil(2) {
        assert_eq!(p.u[row * uv_stride + 20], before.u[row * uv_stride + 20]);
    }
    assert_eq!(&p.u[8 * uv_stride..], &before.u[8 * uv_stride..]);
    assert_eq!(p.a, before.a);
    // Partial blocks with visible pixels replace only zero-alpha luma.
    let mut q = WebPYUVAPicture {
        width: 3,
        height: 1,
        y: vec![1, 6, 100],
        u: vec![77; 2],
        v: vec![99; 2],
        a: Some(vec![1, 200, 0]),
    };
    WebPCleanupTransparentArea(&mut q);
    assert_eq!(q.y, [1, 6, 3]);
    assert_eq!(q.u, [77; 2]);
    assert_eq!(q.v, [99; 2]);
}

#[test]
fn yuva_import_rejects_invalid_geometry_before_reading_or_allocating() {
    for (w, h, stride, bytes, error) in [
        (0, 1, 4, 4, RgbaInputError::InvalidDimensions),
        (1, 0, 4, 4, RgbaInputError::InvalidDimensions),
        (16384, 1, 65536, 0, RgbaInputError::InvalidDimensions),
        (1, 16384, 4, 0, RgbaInputError::InvalidDimensions),
        (2, 1, 7, 8, RgbaInputError::InvalidStride),
        (1, 1, 4, 3, RgbaInputError::InputTooShort),
        (1, 2, usize::MAX, 0, RgbaInputError::SizeOverflow),
    ] {
        assert!(
            matches!(import_rgba_to_yuva(&vec![0;bytes],w,h,stride), Err(LosslessEncodeError::Input(e)) if e == error)
        );
    }
    assert!(import_rgba_to_yuva(&[0; 4], 1, 1, usize::MAX).is_ok());
}

#[cfg(feature = "native-oracle")]
#[test]
fn yuv_and_cleanup_match_pinned_reference() {
    let cases = yuv::cases();
    native::parallel_cases("vp8-yuv", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!("{} exact YUV/import/cleanup requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn yuv_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = yuv::fixture_cases();
    let outputs = native::run("vp8-yuv-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C gamma/YUV/import/cleanup.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lossy_yuv.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_yuv.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
