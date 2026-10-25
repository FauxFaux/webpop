use super::{Case, SEED, animation_lossless::candidate_words, random};
use libwebpop::LosslessOptions;
use libwebpop::mux::anim_encode::*;

pub struct Step {
    pub candidate: Option<LosslessCandidate>,
    pub current: Vec<u32>,
    pub previous: Vec<u32>,
    pub carryover: Vec<u32>,
    pub previous_rect: FrameRectangle,
}

pub fn sequence_steps(case: &Case) -> Vec<Step> {
    let a = &case.input;
    let (w, h) = (a[0] as i32, a[1] as i32);
    let area = (w * h) as usize;
    let mut previous = vec![0; area];
    let mut carryover = vec![0; area];
    let mut previous_rect = FrameRectangle::default();
    let mut first = true;
    let mut at = 4;
    let mut result = Vec::new();
    for _ in 0..a[3] {
        let stride = a[at] as usize;
        let codec = LosslessOptions {
            quality: a[at + 1],
            method: a[at + 2],
            exact: a[at + 3] != 0,
        };
        at += 4;
        let size = (h as usize - 1) * stride + w as usize;
        let current = ArgbView::new(&a[at..at + size], w, h, stride).unwrap();
        at += size;
        if !first {
            CopyIdenticalPixels(
                ArgbView::new(&previous, w, h, w as usize).unwrap(),
                current,
                &mut ArgbViewMut::new(&mut carryover, w, h, w as usize).unwrap(),
            )
            .unwrap();
        }
        let candidate = SetFrameLossless(
            ArgbView::new(&carryover, w, h, w as usize).unwrap(),
            current,
            previous_rect,
            &LosslessCandidateOptions {
                codec,
                is_key_frame: first,
                is_first_frame: first,
                minimize_size: a[2] != 0,
                ..Default::default()
            },
        )
        .unwrap();
        if let Some(candidate) = &candidate {
            candidate
                .update_canvases(
                    current,
                    previous_rect,
                    &mut ArgbViewMut::new(&mut previous, w, h, w as usize).unwrap(),
                    &mut ArgbViewMut::new(&mut carryover, w, h, w as usize).unwrap(),
                )
                .unwrap();
            previous_rect = candidate.rect;
            first = false;
        }
        result.push(Step {
            candidate,
            current: (0..h as usize)
                .flat_map(|y| current.row(y).unwrap().iter().copied())
                .collect(),
            previous: previous.clone(),
            carryover: carryover.clone(),
            previous_rect,
        });
    }
    assert_eq!(at, a.len());
    result
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    if case.function == "AnimationCarryoverSequence" {
        let mut result = Vec::new();
        for step in sequence_steps(case) {
            result.extend(step.candidate.map_or(vec![0], candidate_words));
            let r = step.previous_rect;
            result.extend([
                r.x_offset as u32,
                r.y_offset as u32,
                r.width as u32,
                r.height as u32,
            ]);
            result.extend(step.previous);
            result.extend(step.carryover);
        }
        return result;
    }
    let a = &case.input;
    let (w, h) = (a[1] as i32, a[2] as i32);
    let size = |s: u32| (h as usize - 1) * s as usize + w as usize;
    let (na, nb, nd) = (size(a[3]), size(a[4]), size(a[5]));
    let src = ArgbView::new(&a[7..7 + na], w, h, a[3] as usize).unwrap();
    let other = ArgbView::new(&a[7 + na..7 + na + nb], w, h, a[4] as usize).unwrap();
    let mut dest = a[7 + na + nb..7 + na + nb + nd].to_vec();
    let mask: Vec<_> = a[7 + na + nb + nd..].iter().map(|&v| v as u8).collect();
    let mut dst = ArgbViewMut::new(&mut dest, w, h, a[5] as usize).unwrap();
    if a[0] == 0 {
        CopyIdenticalPixels(src, other, &mut dst).unwrap();
    } else {
        CopyMaskedPixels(src, &mask, &mut dst).unwrap();
    }
    dest
}

fn leaf_case(w: u32, h: u32, pattern: u32, op: u32) -> Case {
    let strides = [
        w + pattern % 3,
        w + (pattern + 1) % 4,
        w + (pattern + 2) % 5,
    ];
    let mut input = vec![op, w, h, strides[0], strides[1], strides[2], w * h + 3];
    let mut state = SEED;
    let words = [
        0, 0x00123456, 0x01010203, 0x7f010203, 0xfeabcdef, 0xffabcdef,
    ];
    for (plane, stride) in strides.into_iter().enumerate() {
        let mut pixels = vec![0xdeadc0de + plane as u32; ((h - 1) * stride + w) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = words[((x + y + pattern) % words.len() as u32) as usize];
                pixels[(y * stride + x) as usize] = match plane {
                    0 => p,
                    1 if (x + y + pattern) % 3 != 0 => p,
                    1 => p ^ [1, 0x100, 0x10000, 0x1000000][pattern as usize % 4],
                    _ => random(&mut state),
                };
            }
        }
        input.extend(pixels);
    }
    input.extend((0..w * h + 3).map(|i| [0, 1, 7, 255][(i + pattern) as usize % 4]));
    Case::new("AnimationCarryover", input)
}

pub fn sequence_case(w: u32, h: u32, pattern: u32, method: u32, minimize: bool) -> Case {
    let mut input = vec![w, h, u32::from(minimize), 8];
    let mut previous = vec![0; (w * h) as usize];
    let mut state = SEED;
    for frame in 0..8 {
        let stride = w + (frame + pattern) % 4;
        // Vary codec settings within a sequence as well as between sequences.
        input.extend([
            stride,
            [0, 70, 100][frame as usize % 3],
            (method + frame) % 7,
            (frame + pattern) % 2,
        ]);
        let mut pixels = vec![0xdeadbeef; ((h - 1) * stride + w) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = match pattern {
                    0 => 0,          // Empty first frame is forced; later frames skip.
                    1 => 0xff123456, // Repeats, including codec changes.
                    2 if frame == 3 || frame == 6 => previous[(y * w + x) as usize],
                    2 => {
                        if x == frame % w && y == frame % h {
                            0xffabcdef
                        } else {
                            0
                        }
                    }
                    3 => {
                        if x < w / 2 && y < h / 2 {
                            if frame % 2 == 0 { 0xff224466 } else { 0 }
                        } else if x + 1 == w && y + 1 == h {
                            0xffabcde0 + frame
                        } else {
                            0xff304050
                        }
                    }
                    4 => [
                        0, 0x00123456, 0x01010203, 0x7f123456, 0xfe123456, 0xffabcdef,
                    ][((x + y + frame) % 6) as usize],
                    5 => {
                        (random(&mut state) & 0xffffff)
                            | if frame % 2 == 0 { 0xff000000 } else { 0 }
                    }
                    7 => {
                        if x < w / 2 && y < h / 2 {
                            match frame {
                                0 => 0xff304050,
                                1 => 0xff224466,
                                _ => 0,
                            }
                        } else if x + 1 == w && y + 1 == h && frame >= 2 {
                            0xffabcde0 + frame
                        } else {
                            0xff304050
                        }
                    }
                    _ => {
                        if (x == 0 && y == 0) || (x + 1 == w && y + 1 == h) {
                            0xffabcde0 + frame
                        } else {
                            0xff304050
                        }
                    }
                };
                pixels[(y * stride + x) as usize] = p;
                previous[(y * w + x) as usize] = p;
            }
        }
        input.extend(pixels);
    }
    Case::new("AnimationCarryoverSequence", input)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..6 {
        for op in 0..2 {
            cases.push(leaf_case(3, 5, pattern, op));
        }
    }
    for pattern in 0..8 {
        for minimize in [false, true] {
            cases.push(sequence_case(8, 7, pattern, pattern % 7, minimize));
        }
    }
    cases.push(sequence_case(16, 16, 7, 4, true));
    for (w, h) in [(1, 1), (1, 9), (9, 1)] {
        cases.push(sequence_case(w, h, 2, 6, true));
    }
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 9), (9, 1), (5, 7), (16, 16), (32, 17)] {
        for pattern in 0..8 {
            for op in 0..2 {
                cases.push(leaf_case(w, h, pattern, op));
            }
            for method in 0..7 {
                for minimize in [false, true] {
                    cases.push(sequence_case(w, h, pattern, method, minimize));
                }
            }
        }
    }
    cases
}
