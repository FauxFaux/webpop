#![allow(dead_code)]

use crate::support::{Case, SEED, random};
use libwebpop::internal::enc::near_lossless_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8LNearLosslessBits" => case.rust(),
        "FindClosestDiscretized" => vec![FindClosestDiscretized(a[0], a[1])],
        "ClosestDiscretizedArgb" => vec![ClosestDiscretizedArgb(a[0], a[1])],
        "IsNear" => vec![u32::from(IsNear(a[0], a[1], a[2]))],
        "IsSmooth" => vec![u32::from(IsSmooth(
            &[0, a[3], 0],
            &[a[1], a[0], a[2]],
            &[0, a[4], 0],
            1,
            a[5],
        ))],
        "NearLossless" => {
            let (w, h, stride) = (a[0] as usize, a[1] as usize, a[2] as usize);
            let mut dst = packed(&a[5..], w, h, stride);
            NearLossless(w, h, a[3], &mut vec![0; 3 * w], &mut dst);
            dst
        }
        "VP8ApplyNearLossless" => {
            VP8ApplyNearLossless(&a[4..], a[0], a[1], a[2] as usize, a[3]).unwrap()
        }
        name => panic!("unknown near-lossless case: {name}"),
    }
}

pub fn packed(argb: &[u32], width: usize, height: usize, stride: usize) -> Vec<u32> {
    (0..height)
        .flat_map(|y| argb[y * stride..y * stride + width].iter().copied())
        .collect()
}

pub fn image(width: u32, height: u32, stride: u32, pattern: u32) -> Vec<u32> {
    let mut state = SEED ^ width ^ (height << 16) ^ pattern;
    let mut argb = vec![0xdeadc0de; ((height - 1) * stride + width) as usize];
    for y in 0..height {
        for x in 0..width {
            let pixel = match pattern {
                0 => random(&mut state),
                1 => ((x + y) & 255) * 0x01010101,
                2 if x == width / 2 && y == height / 2 => 0x7f3f1fff,
                2 => 0x40201008,
                3 => {
                    let alpha = [0, 255, 3, 127, 253][((x + y) % 5) as usize];
                    (alpha << 24) | 0x001f1f1f
                }
                4 => {
                    let alpha = if (x + y) % 2 == 0 { 0 } else { 255 };
                    (alpha << 24) | (random(&mut state) & 0x00ffffff)
                }
                5 => 0x7f3f1fff,
                _ => unreachable!(),
            };
            argb[(y * stride + x) as usize] = pixel;
        }
    }
    argb
}

pub fn preprocessing_case(
    width: u32,
    height: u32,
    padding: u32,
    quality: u32,
    pattern: u32,
) -> Case {
    let stride = width + padding;
    let mut input = vec![width, height, stride, quality];
    input.extend(image(width, height, stride, pattern));
    Case::new("VP8ApplyNearLossless", input)
}

pub fn pass_case(
    width: u32,
    height: u32,
    padding: u32,
    bits: u32,
    pattern: u32,
    inplace: bool,
) -> Case {
    assert!(!inplace || padding == 0);
    let stride = width + padding;
    let mut input = vec![width, height, stride, bits, u32::from(inplace)];
    input.extend(image(width, height, stride, pattern));
    Case::new("NearLossless", input)
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for quality in 0..=100 {
        cases.push(Case::new("VP8LNearLosslessBits", vec![quality]));
    }
    let mut state = SEED;
    for bits in 1..=5 {
        let limit = 1 << bits;
        for a in 0..=255 {
            cases.push(Case::new("FindClosestDiscretized", vec![a, bits]));
            cases.push(Case::new(
                "ClosestDiscretizedArgb",
                vec![random(&mut state), bits],
            ));
            // Both signs, every possible byte difference, and every lane.
            for shift in [0, 8, 16, 24] {
                for (x, y) in [(0, a), (a, 0), (255 - a, 255), (255, 255 - a)] {
                    cases.push(Case::new("IsNear", vec![x << shift, y << shift, limit]));
                }
            }
        }
        for neighbor in 1..=4 {
            for delta in [limit - 1, limit, limit + 1] {
                for shift in [0, 8, 16, 24] {
                    for sign in [-1, 1] {
                        let mut a = vec![0x80808080; 5];
                        a.push(limit);
                        let value = (128 + sign * delta as i32) as u32;
                        a[neighbor] = (a[neighbor] & !(255 << shift)) | (value << shift);
                        cases.push(Case::new("IsSmooth", a));
                    }
                }
            }
        }
        // Same source, first with a separate padded input, then aliased C input
        // and output. Rust packs before its in-place pass in both cases.
        for (w, h) in [(1, 3), (2, 5), (3, 3), (7, 9), (64, 3), (3, 64), (65, 65)] {
            for pattern in 0..6 {
                cases.push(pass_case(w, h, 3, bits, pattern, false));
                cases.push(pass_case(w, h, 0, bits, pattern, true));
            }
        }
    }
    for (w, h) in [
        (1, 1),
        (1, 2),
        (1, 3),
        (1, 65),
        (2, 65),
        (3, 1),
        (65, 1),
        (65, 2),
        (3, 3),
        (5, 5),
        (63, 63),
        (63, 64),
        (64, 3),
        (3, 64),
        (64, 63),
        (64, 64),
        (65, 65),
        (67, 7),
    ] {
        for quality in [0, 19, 20, 39, 40, 59, 60, 79, 80, 99] {
            for pattern in 0..6 {
                for padding in [0, 3] {
                    cases.push(preprocessing_case(w, h, padding, quality, pattern));
                }
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for bits in 1..=5 {
        for a in [0, (1 << bits) / 2, 3 * (1 << bits) / 2, 255] {
            cases.push(Case::new("FindClosestDiscretized", vec![a, bits]));
        }
        cases.push(Case::new("ClosestDiscretizedArgb", vec![0x7f3f1fff, bits]));
        cases.push(Case::new(
            "IsNear",
            vec![0x80808080, 0x80808080 + (1 << bits), 1 << bits],
        ));
        cases.push(Case::new(
            "IsSmooth",
            vec![
                0x80808080,
                0x80808080,
                0x80808080,
                0x80808080,
                0x80808080 + (1 << bits) - 1,
                1 << bits,
            ],
        ));
        cases.push(pass_case(5, 5, 2, bits, 0, false));
        cases.push(pass_case(5, 5, 0, bits, 0, true));
    }
    cases.push(preprocessing_case(3, 3, 2, 0, 0));
    cases.push(preprocessing_case(64, 2, 3, 0, 0));
    cases.push(preprocessing_case(64, 3, 3, 0, 3));
    cases.push(preprocessing_case(3, 64, 2, 20, 0));
    cases
}

pub fn fixtures(text: &str) -> Vec<(Case, Vec<u32>)> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (input, output) = line.split_once('|').unwrap();
            let mut words = input.split_whitespace();
            let function = words.next().unwrap();
            let parse = |word| u32::from_str_radix(word, 16).unwrap();
            (
                Case::new(function, words.map(parse).collect::<Vec<_>>()),
                output.split_whitespace().map(parse).collect(),
            )
        })
        .collect()
}
