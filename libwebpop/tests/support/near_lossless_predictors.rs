#![allow(dead_code)]

use crate::support::{Case, SEED, random};
use libwebpop::internal::dsp::lossless_common::VP8LAddPixels;
use libwebpop::internal::enc::predictor_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "MaxDiffBetweenPixels" => vec![u32::from(MaxDiffBetweenPixels(a[0], a[1]))],
        "MaxDiffAroundPixel" => vec![u32::from(MaxDiffAroundPixel(a[0], a[1], a[2], a[3], a[4]))],
        "AddGreenToBlueAndRed" => vec![AddGreenToBlueAndRed(a[0])],
        "MaxDiffsForRow" => {
            let mut diffs = vec![0xa5; a[0] as usize];
            MaxDiffsForRow(
                a[0] as usize,
                a[1] as usize,
                &a[3..],
                a[1] as usize,
                &mut diffs,
                a[2] != 0,
            );
            diffs.into_iter().map(u32::from).collect()
        }
        "NearLosslessComponent" => {
            let residual = NearLosslessComponent(a[0] as u8, a[1] as u8, a[2] as u8, a[3]);
            vec![
                u32::from(residual),
                u32::from((a[1] as u8).wrapping_add(residual)),
            ]
        }
        "NearLosslessDiff" => vec![u32::from(NearLosslessDiff(a[0] as u8, a[1] as u8))],
        "PredictorNearLossless" => {
            let residual = NearLossless(a[0], a[1], a[2], a[3] as u8, a[4] != 0);
            vec![residual, VP8LAddPixels(a[1], residual)]
        }
        _ => unreachable!("{}", case.function),
    }
}

pub fn row_case(width: usize, padding: usize, subtract_green: bool, pattern: u32) -> Case {
    let stride = width + padding;
    let mut a = vec![width as u32, stride as u32, u32::from(subtract_green)];
    a.extend(crate::support::analysis::pixels(width, 3, stride, pattern));
    Case::new("MaxDiffsForRow", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = vec![
        Case::new("MaxDiffBetweenPixels", vec![0x01234567, 0xff234567]),
        Case::new(
            "MaxDiffAroundPixel",
            vec![0x80808080, 0x8080807f, 0x80807e80, 0x807d8080, 0x7c808080],
        ),
        Case::new("AddGreenToBlueAndRed", vec![0x80fffe01]),
        Case::new("NearLosslessDiff", vec![0, 255]),
    ];
    for q in [1, 2, 4, 8, 16, 32] {
        for (value, predict, boundary) in [
            (0, 255, 255),
            (255, 0, 255),
            (q / 2, 0, 255),
            (0, q / 2, 255),
            (127, 128, 127),
            (129, 128, 128),
        ] {
            cases.push(Case::new(
                "NearLosslessComponent",
                vec![value, predict, boundary, q],
            ));
        }
        for diff in [0, 2, 3, q, q + 1, 255] {
            for sg in [0, 1] {
                cases.push(Case::new(
                    "PredictorNearLossless",
                    vec![0x7f01feff, 0x8000ff01, q, diff, sg],
                ));
            }
        }
    }
    for w in [1, 2, 3, 9] {
        for sg in [false, true] {
            cases.push(row_case(w, 3, sg, 2));
        }
    }
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for q in [1, 2, 4, 8, 16, 32] {
        // Exhaustive value/prediction pairs at the normal component boundary.
        for value in 0..=255 {
            for predict in 0..=255 {
                cases.push(Case::new(
                    "NearLosslessComponent",
                    vec![value, predict, 255, q],
                ));
            }
        }
        // Every subtract-green boundary, including ties and wrapped residuals.
        for boundary in 0..=255 {
            for value in [0, 1, 127, 128, 254, 255, boundary, (boundary + 1) & 255] {
                for predict in [0, 1, 127, 128, 254, 255, boundary, (boundary + 1) & 255] {
                    cases.push(Case::new(
                        "NearLosslessComponent",
                        vec![value, predict, boundary, q],
                    ));
                }
            }
        }
    }
    for a in 0..=255 {
        for b in 0..=255 {
            cases.push(Case::new("NearLosslessDiff", vec![a, b]));
            cases.push(Case::new(
                "AddGreenToBlueAndRed",
                vec![0x80000000 | (a << 16) | (b << 8) | (255 - a)],
            ));
        }
        for shift in [0, 8, 16, 24] {
            for (p1, p2) in [(a << shift, 0), (0, a << shift)] {
                cases.push(Case::new("MaxDiffBetweenPixels", vec![p1, p2]));
            }
        }
    }
    let mut seed = SEED;
    for q in [1, 2, 4, 8, 16, 32] {
        for diff in 0..=255 {
            for alpha in [0, 255, 1, 127, 254, random(&mut seed) & 255] {
                let value = (random(&mut seed) & 0xffffff) | (alpha << 24);
                let predict = random(&mut seed);
                for sg in [0, 1] {
                    cases.push(Case::new(
                        "PredictorNearLossless",
                        vec![value, predict, q, diff, sg],
                    ));
                }
            }
        }
    }
    for _ in 0..2048 {
        cases.push(Case::new(
            "MaxDiffAroundPixel",
            (0..5).map(|_| random(&mut seed)).collect::<Vec<_>>(),
        ));
    }
    for w in [1, 2, 3, 17, 64, 65] {
        for padding in [0, 3] {
            for sg in [false, true] {
                for pattern in [0, 2, 3, 5, 6, 12] {
                    cases.push(row_case(w, padding, sg, pattern));
                }
            }
        }
    }
    cases
}
