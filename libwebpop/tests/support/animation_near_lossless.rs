use super::{Case, animation_cache as cache, animation_lossless as lossless};
use libwebpop::internal::mux::anim_encode::*;

pub fn base(case: &Case) -> Case {
    Case::new("AnimationLossless", case.input[1..].to_vec())
}

pub fn candidate(case: &Case) -> Option<LosslessCandidate> {
    let base = base(case);
    let a = &base.input;
    let (src, dst) = lossless::canvases(&base);
    let source = ArgbView::new(src, a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
    let current = ArgbView::new(dst, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
    let options = lossless::options(&base);
    if a[0] == 2 {
        let rect = lossless::rectangle(&a[5..9]);
        Some(
            encode_near_lossless_candidate(
                current.sub_view(rect).unwrap(),
                rect,
                &options.codec,
                case.input[0] as i32,
                a[9] & 16 != 0,
            )
            .unwrap(),
        )
    } else {
        set_frame_near_lossless(
            source,
            current,
            lossless::rectangle(&a[13..17]),
            &options,
            case.input[0] as i32,
        )
        .unwrap()
    }
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    if case.function == "AnimationNearLosslessCache" {
        return cache::evaluate(case);
    }
    candidate(case).map_or(vec![0], lossless::candidate_words)
}

pub fn candidate_case(
    w: u32,
    h: u32,
    pattern: u32,
    method: u32,
    strength: u32,
    flags: u32,
) -> Case {
    let mut base = lossless::base(w, h, pattern.min(12), flags);
    base.input[11] = method;
    base.input[12] = method % 2;
    if pattern >= 100 {
        let src_size = ((h - 1) * base.input[3] + w) as usize;
        let pixels = super::analysis::pixels(
            w as usize,
            h as usize,
            base.input[4] as usize,
            [2, 9, 12, 24][(pattern - 100) as usize % 4],
        );
        base.input[18 + src_size..].copy_from_slice(&pixels);
    }
    let mut a = vec![strength];
    a.extend(base.input);
    Case::new("AnimationNearLossless", a)
}

pub fn cache_case(
    w: u32,
    h: u32,
    pattern: u32,
    method: u32,
    strength: u32,
    timing: u32,
    minimize: bool,
) -> Case {
    let base = cache::case(w, h, pattern.min(7), method, timing, minimize, 8);
    let mut a = base.input[..8].to_vec();
    a[3] = u32::from(pattern != 0); // Full assembly for nonempty/changing inputs.
    let originals = cache::frames(&base);
    let rich = (pattern >= 100).then(|| {
        super::analysis::pixels(
            w as usize,
            h as usize,
            w as usize + 3,
            [2, 9, 12, 24][(pattern - 100) as usize % 4],
        )
    });
    for (index, f) in originals.iter().enumerate() {
        // Include a codec-strength change on a repeat: it must not force an
        // unchanged original to be encoded again or replace carryover by a decode.
        let strength = if index == 3 { 100 } else { strength };
        a.extend([
            f.duration,
            f.stride as u32,
            f.codec.quality,
            f.codec.method,
            u32::from(f.codec.exact),
            strength,
        ]);
        if let Some(pixels) = &rich {
            let mut current = vec![0xdeadbeef; (h as usize - 1) * f.stride + w as usize];
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let p = pixels[y * (w as usize + 3) + x];
                    // Several unchanged noisy inputs interleaved with changed
                    // opposite corners, alpha endpoints and transparent holds.
                    current[y * f.stride + x] = if index >= 2
                        && (x == 0 && y == 0 || x + 1 == w as usize && y + 1 == h as usize)
                    {
                        p ^ (index as u32 / 2) * 0x00010203
                    } else {
                        p
                    };
                }
            }
            a.extend(current);
        } else {
            a.extend_from_slice(f.words);
        }
    }
    Case::new("AnimationNearLosslessCache", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in [0, 4, 5, 6, 8, 12, 100, 101, 102, 103] {
        for strength in [0, 60, 100] {
            let (w, h) = if pattern >= 100 { (65, 5) } else { (16, 16) };
            cases.push(candidate_case(
                w,
                h,
                pattern,
                4,
                strength,
                if pattern >= 100 { 3 } else { 8 },
            ));
        }
    }
    for pattern in [0, 2, 4, 7, 100, 101, 102, 103] {
        for strength in [0, 60, 100] {
            let (w, h) = if pattern >= 100 { (65, 5) } else { (8, 7) };
            cases.push(cache_case(w, h, pattern, 4, strength, 1, true));
        }
    }
    for strength in [0, 100] {
        for (w, h, pattern, exact, cropped) in [
            (65, 5, 100, true, false),
            (65, 5, 100, true, true),
            (19, 17, 101, true, false),
            (19, 17, 103, false, false),
        ] {
            let mut case = candidate_case(w, h, pattern, 4, strength, 3);
            case.input[1] = 2;
            case.input[13] = u32::from(exact);
            if cropped {
                case.input[6..10].copy_from_slice(&[2, 0, 63, 5]);
            }
            cases.push(case);
        }
        cases.push(transparency_palette_case(strength));
    }
    cases
}

pub fn transparency_palette_case(strength: u32) -> Case {
    let mut case = candidate_case(65, 5, 100, 4, strength, 8);
    let a = &mut case.input[1..];
    let src_size = ((a[2] - 1) * a[3] + a[1]) as usize;
    a[13..17].fill(0); // Identical disposal alternatives, preserving NONE on ties.
    for y in 0..5usize {
        for x in 0..65usize {
            let index = 18 + src_size + y * a[4] as usize + x;
            let p = a[index] | 0xff000000;
            a[18 + y * a[3] as usize + x] = p;
            a[index] = if (x == 0 && y == 0) || (x == 64 && y == 4) {
                p ^ 0x42
            } else {
                p
            };
        }
    }
    case
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 9), (9, 1), (19, 17), (63, 5), (64, 5), (65, 5)] {
        for pattern in [4, 6, 12, 100, 101, 102, 103] {
            for method in 0..7 {
                for strength in [0, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100] {
                    cases.push(candidate_case(
                        w,
                        h,
                        pattern,
                        method,
                        strength,
                        if pattern >= 100 { 3 } else { 8 },
                    ));
                }
            }
        }
    }
    for (w, h) in [(1, 1), (5, 7), (19, 17), (65, 5)] {
        for pattern in [0, 2, 4, 7, 100, 101, 102, 103] {
            for method in 0..7 {
                for strength in [0, 20, 60, 99, 100] {
                    cases.push(cache_case(
                        w,
                        h,
                        pattern,
                        method,
                        strength,
                        method % 2,
                        method % 2 != 0,
                    ));
                }
            }
        }
    }
    cases
}
