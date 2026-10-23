#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::dsp::{dec::TransformWHT_C, enc::*};
use support::{SEED, random, vp8_transforms as transforms};

#[test]
fn committed_vp8_transform_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_transforms.txt"));
    let cases = transforms::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected, "{}", case.function);
    }
}

#[test]
fn inverse_dc_rounding_clipping_and_padding() {
    for do_two in [false, true] {
        let width = if do_two { 8 } else { 4 };
        let count = 4 * width;
        let reference: Vec<_> = (0..count)
            .map(|i| [0, 1, 127, 128, 254, 255][i % 6])
            .collect();
        for dc in -4095i16..=4095 {
            let mut coefficients = vec![0; count];
            coefficients[0] = dc;
            if do_two {
                coefficients[16] = -dc;
            }
            let actual = transforms::inverse_case(&reference, &coefficients, do_two).rust();
            for y in 0..4 {
                for x in 0..BPS {
                    let expected = if x < width {
                        let delta = i32::from(if x < 4 { dc } else { -dc });
                        // DC reconstruction rounds delta/8 to nearest, ties
                        // toward positive infinity, then saturates the pixel.
                        let change = (delta + 4).div_euclid(8);
                        (i32::from(reference[y * width + x]) + change).clamp(0, 255) as u32
                    } else {
                        0xa5
                    };
                    assert_eq!(
                        actual[y * BPS + x],
                        expected,
                        "DC {dc}, two {do_two}, ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn minimum_slices_and_offset_destinations_preserve_surrounding_storage() {
    for do_two in [false, true] {
        let width = if do_two { 8 } else { 4 };
        let size = 3 * BPS + width;
        let reference = vec![128; size + 1];
        let mut dst = vec![0xa5; size + 6];
        let mut coefficients = vec![0; if do_two { 32 } else { 16 }];
        coefficients[0] = -4;
        if do_two {
            coefficients[16] = 4;
        }
        ITransform_C(
            &reference[1..],
            &coefficients,
            &mut dst[3..3 + size],
            do_two,
        );
        for (i, &v) in dst.iter().enumerate() {
            let expected = if i >= 3 && i < 3 + size && (i - 3) % BPS < width {
                if (i - 3) % BPS < 4 { 128 } else { 129 }
            } else {
                0xa5
            };
            assert_eq!(v, expected);
        }
    }
    let mut input = [0i16; 241];
    for i in 0..16 {
        input[16 * i] = 16;
    }
    let mut wht = [0; 16];
    FTransformWHT_C(&input, &mut wht);
    assert_eq!(wht[0], 128);
    assert!(wht[1..].iter().all(|&v| v == 0));
    let mut dst = [1234; 247];
    TransformWHT_C(&wht, &mut dst[3..244]);
    for (i, &v) in dst.iter().enumerate() {
        assert_eq!(
            v,
            if (3..244).contains(&i) && (i - 3) % 16 == 0 {
                16
            } else {
                1234
            }
        );
    }
}

#[test]
fn paired_forward_transform_matches_two_independent_blocks() {
    let mut seed = SEED;
    for _ in 0..512 {
        let src: [u8; 3 * BPS + 8] = std::array::from_fn(|_| random(&mut seed) as u8);
        let reference: [u8; 3 * BPS + 8] = std::array::from_fn(|_| random(&mut seed) as u8);
        let mut actual = [0; 32];
        FTransform2_C(&src, &reference, &mut actual);
        for block in 0..2 {
            let mut expected = [0; 16];
            FTransform_C(&src[4 * block..], &reference[4 * block..], &mut expected);
            assert_eq!(actual[16 * block..16 * block + 16], expected);
        }
    }
}

#[test]
fn walsh_transforms_match_independent_matrix_products() {
    // Symmetric Hadamard matrix, in VP8's frequency order. Direct matrix
    // multiplication independently checks row/column order and normalization.
    const H: [[i32; 4]; 4] = [[1, 1, 1, 1], [1, 1, -1, -1], [1, -1, -1, 1], [1, -1, 1, -1]];
    for case in transforms::cases() {
        let inverse = case.function == "VP8TransformWHT";
        if !inverse && case.function != "VP8FTransformWHT" {
            continue;
        }
        let input: [i32; 16] =
            std::array::from_fn(|i| case.input[if inverse { i } else { 16 * i }] as i32);
        let mut expected = if inverse {
            case.input[16..].to_vec()
        } else {
            vec![0; 16]
        };
        for y in 0..4 {
            for x in 0..4 {
                let mut sum = 0;
                for row in 0..4 {
                    for col in 0..4 {
                        sum += H[y][row] * input[4 * row + col] * H[col][x];
                    }
                }
                let value = if inverse {
                    (sum + 3).div_euclid(8)
                } else {
                    sum.div_euclid(2)
                };
                expected[(4 * y + x) * if inverse { 16 } else { 1 }] = value as u32;
            }
        }
        assert_eq!(case.rust(), expected, "{}", case.function);
    }
}

#[test]
fn walsh_even_dc_round_trips_and_ignores_ac() {
    let mut seed = SEED;
    for _ in 0..512 {
        let mut blocks = [0i16; 256];
        for (i, v) in blocks.iter_mut().enumerate() {
            *v = if i % 16 == 0 {
                ((random(&mut seed) % 2048) as i16 - 1024) * 2
            } else {
                random(&mut seed) as i16
            };
        }
        let mut wht = [0; 16];
        FTransformWHT_C(&blocks, &mut wht);
        let mut dc_only = blocks;
        for i in 0..256 {
            if i % 16 != 0 {
                dc_only[i] = 0;
            }
        }
        let mut without_ac = [0; 16];
        FTransformWHT_C(&dc_only, &mut without_ac);
        assert_eq!(wht, without_ac);
        let mut reconstructed = blocks;
        for i in 0..16 {
            reconstructed[16 * i] = 0;
        }
        TransformWHT_C(&wht, &mut reconstructed);
        assert_eq!(reconstructed, blocks);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_transform_leaves_match_pinned_scalar_reference() {
    let cases = transforms::cases();
    let outputs = native::run("vp8-transforms", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    for function in [
        "VP8FTransform2",
        "VP8ITransform",
        "VP8FTransformWHT",
        "VP8TransformWHT",
    ] {
        eprintln!(
            "{function}: {} exact C comparisons",
            cases
                .iter()
                .filter(|case| case.function == function)
                .count()
        );
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_transform_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = transforms::fixture_cases();
    let outputs = native::run("vp8-transform-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C paired/inverse VP8 and forward/inverse WHT transforms.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            SEED
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
                "/tests/fixtures/lossy_transforms.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_transforms.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
