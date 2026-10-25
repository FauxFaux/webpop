use super::{Case, SEED, random};
use libwebpop::internal::mux::anim_encode::*;

fn rectangle(a: &[u32]) -> FrameRectangle {
    FrameRectangle {
        x_offset: a[0] as i32,
        y_offset: a[1] as i32,
        width: a[2] as i32,
        height: a[3] as i32,
    }
}

fn words(rect: FrameRectangle) -> [u32; 4] {
    [
        rect.x_offset as u32,
        rect.y_offset as u32,
        rect.width as u32,
        rect.height as u32,
    ]
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match a[0] {
        0 => vec![QualityToMaxDiff(f32::from_bits(a[1])) as u32],
        1 => vec![u32::from(PixelsAreSimilar(a[1], a[2], a[3] as i32))],
        2 => {
            let (length, s, d) = (a[1] as usize, a[2] as usize, a[3] as usize);
            let src = &a[5..5 + length * s + 1];
            let dst = &a[5 + length * s + 1..];
            vec![
                u32::from(ComparePixelsLossless(src, s, dst, d, length, a[4] as i32)),
                u32::from(ComparePixelsLossy(src, s, dst, d, length, a[4] as i32)),
            ]
        }
        3 | 4 => {
            let len = ((a[2] - 1) * a[3] + a[1]) as usize;
            let src =
                ArgbView::new(&a[12..12 + len], a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
            let dst =
                ArgbView::new(&a[12 + len..], a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
            let mut rect = rectangle(&a[5..]);
            let quality = f32::from_bits(a[10]);
            let ok = if a[0] == 4 {
                WebPAnimEncoderRefineRect(src, dst, a[9] != 0, quality, &mut rect).is_ok()
            } else {
                MinimizeChangeRectangle(src, dst, &mut rect, a[9] != 0, quality);
                if a[11] != 0 {
                    SnapToEvenOffsets(&mut rect);
                }
                true
            };
            let mut out = vec![u32::from(ok)];
            out.extend(words(rect));
            if ok {
                out.extend([
                    u32::from(IsEmptyRect(&rect)),
                    u32::from(IsLosslessBlendingPossible(src, dst, &rect)),
                    u32::from(IsLossyBlendingPossible(src, dst, &rect, quality)),
                ]);
            }
            out
        }
        5 => {
            let mut rect = rectangle(&a[1..]);
            let mut out = vec![u32::from(IsEmptyRect(&rect))];
            SnapToEvenOffsets(&mut rect);
            out.extend(words(rect));
            out
        }
        _ => unreachable!(),
    }
}

pub fn rectangle_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut state = SEED;
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (8, 8), (17, 19)] {
        for pattern in 0..10 {
            let (s, d) = (w + pattern % 3, w + (pattern + 1) % 4);
            let mut src = vec![0xdeadc0de; ((h - 1) * s + w) as usize];
            let mut dst = vec![0xaabbccdd; ((h - 1) * d + w) as usize];
            for y in 0..h {
                for x in 0..w {
                    let a = random(&mut state);
                    let b = match pattern {
                        0 => a,
                        1 => a ^ u32::from(x == w / 2 && y == h / 2),
                        2 => a ^ u32::from(x == w - 1 && y == h - 1),
                        3 => a ^ u32::from(x == 0 || y == 0),
                        4 => a ^ u32::from((x == 0 && y == 0) || (x == w - 1 && y == h - 1)),
                        5 => a ^ 0x010101,
                        6 => (a ^ 0x404040) & 0x00ffffff,
                        7 => a ^ 0x01000000,
                        8 => random(&mut state) | 0xff000000,
                        _ => random(&mut state),
                    };
                    src[(y * s + x) as usize] = if pattern == 6 { a & 0x00ffffff } else { a };
                    dst[(y * d + x) as usize] = b;
                }
            }
            for lossless in [0, 1] {
                for quality in [0f32, 24.5, 75., 98.5, 100.] {
                    let mut base = vec![3, w, h, s, d, 0, 0, w, h, lossless, quality.to_bits(), 0];
                    base.extend_from_slice(&src);
                    base.extend_from_slice(&dst);
                    for snap in [0, 1] {
                        let mut input = base.clone();
                        input[11] = snap;
                        cases.push(Case::new("AnimationLeaves", input));
                    }
                    for guess in [
                        [-3, -5, w as i32 + 4, h as i32 + 8],
                        [w as i32 / 2, h as i32 / 2, w as i32, h as i32],
                        [i32::MIN, i32::MIN, i32::MAX, i32::MAX],
                        [i32::MAX, i32::MAX, i32::MAX, i32::MAX],
                        [0, 0, 0, h as i32],
                        [0, 0, w as i32, -1],
                    ] {
                        let mut input = base.clone();
                        input[0] = 4;
                        input[5..9].copy_from_slice(&guess.map(|v| v as u32));
                        cases.push(Case::new("AnimationLeaves", input));
                    }
                    // Empty initial guesses normalize to the canonical empty rect.
                    let mut input = base.clone();
                    input[5] = w;
                    input[7] = 0;
                    cases.push(Case::new("AnimationLeaves", input));
                }
            }
        }
    }
    // Invalid quality must preserve the caller's rectangle even in lossless mode.
    for quality in [-1., 100.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut input = cases[0].input.clone();
        input[0] = 4;
        input[10] = quality.to_bits();
        cases.push(Case::new("AnimationLeaves", input));
    }
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = rectangle_cases();
    for i in 0..=10000 {
        cases.push(Case::new(
            "AnimationLeaves",
            vec![0, (i as f32 / 100.).to_bits()],
        ));
    }
    // Adjacent float values around every integer-rounding threshold.
    for difference in 1..31 {
        let q = (100.0 * ((31.0 - (difference as f64 + 0.5)) / 30.0).powi(2)) as f32;
        for quality in [q.next_down(), q, q.next_up()] {
            cases.push(Case::new("AnimationLeaves", vec![0, quality.to_bits()]));
        }
    }
    for alpha in [0, 1, 2, 127, 128, 254, 255] {
        for diff in 0..=255 {
            for bound in [0, 1, 2, 7, 31, 255] {
                for shift in [0, 8, 16] {
                    let a = alpha << 24;
                    let b = a | (diff << shift);
                    cases.push(Case::new("AnimationLeaves", vec![1, a, b, bound]));
                    cases.push(Case::new(
                        "AnimationLeaves",
                        vec![1, a, b ^ 0x01000000, bound],
                    ));
                }
            }
        }
    }
    let mut state = SEED;
    for length in [1, 2, 7, 17] {
        for s in [0, 1, 3] {
            for d in [0, 1, 5] {
                for mismatch in 0..=length {
                    let mut input = vec![2, length, s, d, 7];
                    input.extend((0..length * s + 1).map(|_| random(&mut state)));
                    input.extend((0..length * d + 1).map(|_| random(&mut state)));
                    for i in 0..length {
                        // Constant sampled pixels also cover zero strides.
                        input[(5 + i * s) as usize] = 0x80010203;
                        input[(5 + length * s + 1 + i * d) as usize] = 0x80010203;
                    }
                    if mismatch < length {
                        input[(5 + length * s + 1 + mismatch * d) as usize] ^= 0x10;
                    }
                    cases.push(Case::new("AnimationLeaves", input));
                }
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = rectangle_cases().into_iter().step_by(109).collect();
    for quality in [0f32, 24.5, 75., 98.5, 100.] {
        cases.push(Case::new("AnimationLeaves", vec![0, quality.to_bits()]));
    }
    for alpha in [0, 1, 127, 254, 255] {
        for diff in [0, 1, 31, 255] {
            cases.push(Case::new(
                "AnimationLeaves",
                vec![1, alpha << 24, (alpha << 24) | diff, 1],
            ));
        }
    }
    for rect in [[1, 3, 2, 4], [0, 0, 0, 0], [3, 5, 0, 0]] {
        let mut input = vec![5];
        input.extend(rect);
        cases.push(Case::new("AnimationLeaves", input));
    }
    let mut input = vec![2, 3, 2, 1, 1];
    input.extend([0xff010203, 99, 0x00010203, 99, 0x7f010203, 99, 99]);
    input.extend([0xff010204, 0x00ffffff, 0x7f010205, 99]);
    cases.push(Case::new("AnimationLeaves", input));
    // Ensure invalid quality and invalid extents have portable reference evidence.
    for index in [7, 10] {
        let mut input = cases[0].input.clone();
        input[0] = 4;
        input[index] = if index == 7 { 0 } else { f32::NAN.to_bits() };
        cases.push(Case::new("AnimationLeaves", input));
    }
    cases
}
