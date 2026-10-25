//! FlattenSimilarBlocks, lossy SetFrame and original-word CacheFrame snapshots.
use super::{Case, SEED, animation_lossless, random};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub fn candidate_words(candidate: LossyCandidate) -> Vec<u32> {
    let r = candidate.rect;
    let mut result = vec![
        1,
        r.x_offset as u32,
        r.y_offset as u32,
        r.width as u32,
        r.height as u32,
        u32::from(candidate.control.blend == FrameBlend::Replace),
        match candidate.previous_dispose {
            None => 0,
            Some(FrameDispose::None) => 1,
            Some(FrameDispose::Background) => 2,
        },
        u32::from(candidate.carries_over()),
        candidate.carryover_mask.len() as u32,
    ];
    assert_eq!(candidate.control.duration_ms, 0);
    assert_eq!(candidate.control.dispose, FrameDispose::None);
    let size = candidate.encoded_size();
    result.extend(candidate.carryover_mask.into_iter().map(u32::from));
    let mut mux =
        AnimationContainer::new(r.width as u32, r.height as u32, Default::default()).unwrap();
    mux.push_frame(candidate.frame, Default::default()).unwrap();
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    assert_eq!(bytes.len(), size);
    result.push(size as u32);
    result.extend(bytes.into_iter().map(u32::from));
    result
}

pub fn base(case: &Case) -> Case {
    Case::new("AnimationLossless", case.input[2..].to_vec())
}

pub fn options(case: &Case) -> LossyCandidateOptions {
    let a = &case.input[2..];
    LossyCandidateOptions {
        codec: LossyOptions {
            quality: f32::from_bits(a[10]),
            method: a[11],
            alpha_quality: case.input[0] as i32,
            use_sharp_yuv: case.input[1] != 0,
        },
        is_key_frame: a[9] & 1 != 0,
        is_first_frame: a[9] & 2 != 0,
        previous_candidate_undecided: a[9] & 4 != 0,
        minimize_size: a[9] & 8 != 0,
    }
}

pub fn candidate(case: &Case) -> Option<LossyCandidate> {
    let base = base(case);
    let a = &base.input;
    let (s, d) = animation_lossless::canvases(&base);
    SetFrameLossy(
        ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        ArgbView::new(d, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
        animation_lossless::rectangle(&a[13..]),
        &options(case),
    )
    .unwrap()
}

pub struct Step {
    pub candidate: Option<LossyCandidate>,
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
        let codec = LossyOptions {
            quality: f32::from_bits(a[at + 1]),
            method: a[at + 2],
            alpha_quality: a[at + 3] as i32,
            use_sharp_yuv: a[at + 4] != 0,
        };
        at += 5;
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
        let candidate = SetFrameLossy(
            ArgbView::new(&carryover, w, h, w as usize).unwrap(),
            current,
            previous_rect,
            &LossyCandidateOptions {
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
    if case.function == "AnimationLossyCarryoverSequence" {
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
    let base = base(case);
    let a = &base.input;
    if a[0] == 3 {
        return candidate(case).map_or(vec![0], candidate_words);
    }
    assert_eq!(a[0], 0);
    let (s, d) = animation_lossless::canvases(&base);
    let mut dst = d.to_vec();
    let mut mask: Vec<_> = a[18 + s.len() + d.len()..]
        .iter()
        .map(|&v| v as u8)
        .collect();
    let changed = FlattenSimilarBlocks(
        ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        &animation_lossless::rectangle(&a[5..]),
        &mut ArgbViewMut::new(&mut dst, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
        options(case).codec.quality,
        &mut mask,
    )
    .unwrap();
    let mut result = vec![u32::from(changed)];
    result.extend(dst);
    result.extend(mask.into_iter().map(u32::from));
    result
}

pub fn selection_case(w: u32, h: u32, pattern: u32, flags: u32, method: u32) -> Case {
    let mut base = animation_lossless::base(w, h, pattern, flags);
    if pattern == 10 {
        let (s, d) = (base.input[3], base.input[4]);
        let source_size = ((h - 1) * s + w) as usize;
        for y in 0..h {
            for x in 0..w {
                let p = 0xff000000 | ((x * 7919 + y * 104729) & 0xffffff);
                base.input[18 + (y * s + x) as usize] = if flags & 2 != 0 { 0 } else { p };
                base.input[18 + source_size + (y * d + x) as usize] =
                    if (x == 2.min(w - 1) && y == 2.min(h - 1)) || (x == w - 1 && y == h - 1) {
                        p ^ 0x808080
                    } else {
                        p
                    };
            }
        }
    }
    base.input[10] = [0f32, 75., 98.5, 100.][pattern as usize % 4].to_bits();
    base.input[11] = method;
    let mut input = vec![[0, 71, 99, 100][pattern as usize % 4], pattern % 2];
    input.extend(base.input);
    Case::new("AnimationLossy", input)
}

pub fn flatten_case(w: u32, h: u32, left: u32, top: u32, pattern: u32, quality: f32) -> Case {
    let (s, d) = (w + pattern % 3, w + (pattern + 1) % 4);
    let (rw, rh) = (w - left, h - top);
    let mut input = vec![
        100,
        0,
        0,
        w,
        h,
        s,
        d,
        left,
        top,
        rw,
        rh,
        0,
        quality.to_bits(),
        4,
        0,
        0,
        0,
        w,
        h,
        rw * rh + 3,
    ];
    let max_diff = QualityToMaxDiff(quality) as u32;
    for (plane, stride) in [(0, s), (1, d)] {
        let mut pixels = vec![0xdeadbeef + plane; ((h - 1) * stride + w) as usize];
        for y in 0..h {
            for x in 0..w {
                let p = 0xff000000
                    | ((64 + (x * 3 + y) % 64) << 16)
                    | ((96 + (x + y * 3) % 64) << 8)
                    | (32 + (x + y) % 64);
                pixels[(y * stride + x) as usize] = match pattern {
                    0 => p,
                    1 if plane == 1 => p + max_diff,
                    2 if plane == 1 && x % 8 == 7 && y % 8 == 7 => p + max_diff + 1,
                    3 if plane == 0 && x % 8 == 7 && y % 8 == 7 => p & 0xfeffffff,
                    4 if plane == 1 && x % 8 == 7 && y % 8 == 7 => p & 0xfeffffff,
                    5 => p & 0x7fffffff,
                    6 if plane == 1 => p + max_diff + 1,
                    7 => {
                        if plane == 0 {
                            0xffabcdef
                        } else {
                            0xffabcdef - max_diff
                        }
                    }
                    _ => p,
                };
            }
        }
        input.extend(pixels);
    }
    input.extend((0..rw * rh + 3).map(|i| [0, 7, 255][i as usize % 3]));
    Case::new("AnimationLossy", input)
}

pub fn sequence_case(w: u32, h: u32, pattern: u32, method: u32, minimize: bool) -> Case {
    if pattern < 8 && pattern != 6 {
        let old = super::animation_carryover::sequence_case(w, h, pattern, method, minimize);
        let a = &old.input;
        let mut input = a[..4].to_vec();
        let mut at = 4;
        for frame in 0..8 {
            let stride = a[at];
            input.extend([
                stride,
                [75f32, 98.5, 100.][frame % 3].to_bits(),
                a[at + 2],
                [100, 71, 99][frame % 3],
                (frame + pattern as usize) as u32 % 2,
            ]);
            at += 4;
            let size = ((h - 1) * stride + w) as usize;
            input.extend_from_slice(&a[at..at + size]);
            at += size;
        }
        return Case::new("AnimationLossyCarryoverSequence", input);
    }
    let mut input = vec![w, h, u32::from(minimize), 8];
    let mut state = SEED;
    for frame in 0..8 {
        let stride = w + frame % 4;
        input.extend([
            stride,
            100f32.to_bits(),
            (method + frame) % 7,
            100,
            frame % 2,
        ]);
        let mut pixels = vec![0xdeadbeef; ((h - 1) * stride + w) as usize];
        for y in 0..h {
            for x in 0..w {
                let original = 0xff304050 + (((x * 3 + y) % 32) << 16) + (((y * 3 + x) % 32) << 8);
                let p = match pattern {
                    10 => {
                        // Establish a small sprite rectangle, then remove it
                        // while changing distant corners. BG can flatten the
                        // opaque interior of the new full rectangle.
                        let p = 0xff000000 | ((x * 7919 + y * 104729) & 0xffffff);
                        if x < 8 && y < 8 {
                            match frame {
                                0 => p,
                                1 => 0xffabcdef,
                                _ => 0,
                            }
                        } else if frame >= 2 && ((x + 1 == w && y == 0) || (x == 0 && y + 1 == h)) {
                            p ^ (0x808080 + frame)
                        } else {
                            p
                        }
                    }
                    6 if (x == 0 && y == 0) || (x + 1 == w && y + 1 == h) => 0xffabcdef + 2 * frame,
                    6 => original + frame / 2,
                    8 => original + frame,
                    9 if frame == 3 || frame == 4 => original,
                    _ if x >= 8 && y >= 8 => original + frame / 2,
                    _ => 0xff000000 | (random(&mut state) & 0xffffff),
                };
                pixels[(y * stride + x) as usize] = p;
            }
        }
        input.extend(pixels);
    }
    Case::new("AnimationLossyCarryoverSequence", input)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..8 {
        cases.push(flatten_case(
            25,
            23,
            pattern,
            (pattern + 1) % 9,
            pattern,
            75.,
        ));
    }
    for method in 0..7 {
        for flags in [0, 1, 3, 8] {
            cases.push(selection_case(25, 23, 5, flags, method));
        }
        for flags in [0, 8] {
            cases.push(selection_case(25, 23, 10, flags, method));
        }
    }
    for pattern in 0..11 {
        for minimize in [false, true] {
            cases.push(sequence_case(25, 23, pattern, pattern % 7, minimize));
        }
    }
    let mut tie = selection_case(25, 23, 10, 8, 4);
    tie.input[15..19].fill(0); // Identical NONE/BG trials retain NONE and its mask.
    cases.push(tie);
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(8, 8), (9, 9), (16, 16), (17, 19), (25, 23), (33, 25)] {
        for (left, top) in [(0, 0), (1, 7), (7, 1), (8, 8), (9, 9)] {
            if left > w || top > h {
                continue;
            }
            for pattern in 0..8 {
                for quality in [0., 30.25, 75., 98.5, 99.99, 100.] {
                    cases.push(flatten_case(w, h, left, top, pattern, quality));
                }
            }
        }
    }
    for (w, h) in [(1, 1), (1, 17), (17, 1), (17, 19), (25, 23), (33, 25)] {
        for pattern in 0..11 {
            for method in 0..7 {
                for flags in [0, 1, 3, 4, 8, 12] {
                    cases.push(selection_case(w, h, pattern, flags, method));
                }
                for minimize in [false, true] {
                    cases.push(sequence_case(w, h, pattern, method, minimize));
                }
            }
        }
    }
    cases
}
