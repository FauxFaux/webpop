use super::{Case, SEED, random};
use libwebpop::mux::anim_encode::*;
use libwebpop::*;

pub fn rectangle(a: &[u32]) -> FrameRectangle {
    FrameRectangle {
        x_offset: a[0] as i32,
        y_offset: a[1] as i32,
        width: a[2] as i32,
        height: a[3] as i32,
    }
}

pub fn candidate_words(candidate: LosslessCandidate) -> Vec<u32> {
    let rect = candidate.rect;
    let mut result = vec![
        1,
        rect.x_offset as u32,
        rect.y_offset as u32,
        rect.width as u32,
        rect.height as u32,
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
        AnimationContainer::new(rect.width as u32, rect.height as u32, Default::default()).unwrap();
    mux.push_frame(candidate.frame, Default::default()).unwrap();
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    assert_eq!(bytes.len(), size);
    result.push(bytes.len() as u32);
    result.extend(bytes.into_iter().map(u32::from));
    result
}

pub fn canvases(case: &Case) -> (&[u32], &[u32]) {
    let a = &case.input;
    let s = ((a[2] - 1) * a[3] + a[1]) as usize;
    let d = ((a[2] - 1) * a[4] + a[1]) as usize;
    (&a[18..18 + s], &a[18 + s..18 + s + d])
}

pub fn options(case: &Case) -> LosslessCandidateOptions {
    let a = &case.input;
    LosslessCandidateOptions {
        codec: LosslessOptions {
            quality: a[10],
            method: a[11],
            exact: a[12] != 0,
        },
        is_key_frame: a[9] & 1 != 0,
        is_first_frame: a[9] & 2 != 0,
        previous_candidate_undecided: a[9] & 4 != 0,
        minimize_size: a[9] & 8 != 0,
    }
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let (source, dest) = canvases(case);
    let src = ArgbView::new(source, a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
    let rect = rectangle(&a[5..9]);
    match a[0] {
        0 | 1 => {
            let mut dest = dest.to_vec();
            let mut mask: Vec<_> = a[a.len() - a[17] as usize..]
                .iter()
                .map(|&v| v as u8)
                .collect();
            let mut dst =
                ArgbViewMut::new(&mut dest, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
            let mut result = Vec::new();
            if a[0] == 0 {
                result.push(u32::from(
                    IncreaseTransparency(src, &rect, &mut dst, &mut mask).unwrap(),
                ));
            } else {
                DisposeFrameRectangle(
                    if a[9] & 32 != 0 {
                        FrameDispose::Background
                    } else {
                        FrameDispose::None
                    },
                    &rect,
                    &mut dst,
                )
                .unwrap();
            }
            result.extend(dest);
            if a[0] == 0 {
                result.extend(mask.into_iter().map(u32::from));
            }
            result
        }
        2 => {
            let dst = ArgbView::new(dest, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
            candidate_words(
                EncodeCandidate(
                    dst.sub_view(rect).unwrap(),
                    rect,
                    &options(case).codec,
                    a[9] & 16 != 0,
                )
                .unwrap(),
            )
        }
        3 => {
            let dst = ArgbView::new(dest, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
            SetFrameLossless(src, dst, rectangle(&a[13..17]), &options(case))
                .unwrap()
                .map_or(vec![0], candidate_words)
        }
        _ => unreachable!(),
    }
}

pub fn base(w: u32, h: u32, pattern: u32, flags: u32) -> Case {
    let (s, d) = (w + pattern % 3, w + (pattern + 1) % 4);
    let mut a = vec![3, w, h, s, d, 0, 0, w, h, flags, 70, 4, 0, 0, 0, w, h, 0];
    let mut src = vec![0xdeadc0de; ((h - 1) * s + w) as usize];
    let mut dst = vec![0xaabbccdd; ((h - 1) * d + w) as usize];
    let mut state = SEED;
    for y in 0..h {
        for x in 0..w {
            let p = [0xff304050, 0x7f010203, 0xffabcdef, 0, 0x00123456][((x + y) % 5) as usize];
            let (mut before, after) = match pattern {
                0 => (p, p),
                1 => (
                    p,
                    if x == w / 2 && y == h / 2 {
                        0xff987654
                    } else {
                        p
                    },
                ),
                2 => (p, p ^ u32::from(x == w / 2 && y == h / 2) * 0x01000000),
                3 => (p & 0xffffff, (p ^ 0x42) & 0xffffff),
                4 => (
                    if x < w / 2 { 0xff224466 } else { 0 },
                    if x >= w / 2 { 0xff224466 } else { 0 },
                ),
                5 => (
                    0xff304050,
                    if (x == 0 && y == 0) || (x + 1 == w && y + 1 == h) {
                        0xffabcdef
                    } else {
                        0xff304050
                    },
                ),
                6 => (
                    0x7f334455,
                    if x == w / 2 && y == h / 2 {
                        0x7f334456
                    } else {
                        0x7f334455
                    },
                ),
                7 => (random(&mut state), random(&mut state)),
                8 => (0, 0),
                9 => (0xffabcdef, 0),
                10 => (p, if x + 1 == w { p ^ 0x80 } else { p }),
                12 => (
                    if x < w / 2 && y < h / 2 {
                        0xff224466
                    } else {
                        0xff304050
                    },
                    if x < w / 2 && y < h / 2 {
                        0
                    } else if x + 1 == w && y + 1 == h {
                        0xffabcdef
                    } else {
                        0xff304050
                    },
                ),
                _ => (p, if (x + y) % 3 == 0 { 0xff010203 } else { p }),
            };
            if flags & 2 != 0 {
                before = 0;
            }
            src[(y * s + x) as usize] = before;
            dst[(y * d + x) as usize] = after;
        }
    }
    a.extend(src);
    a.extend(dst);
    Case::new("AnimationLossless", a)
}

fn with_mask(mut case: Case, rect: [u32; 4], op: u32) -> Case {
    case.input[0] = op;
    case.input[5..9].copy_from_slice(&rect);
    case.input[17] = rect[2] * rect[3] + 3;
    case.input
        .extend((0..case.input[17]).map(|i| [0, 1, 7, 255][i as usize % 4]));
    case
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 9), (9, 1), (5, 7), (16, 16), (32, 17)] {
        for pattern in 0..13 {
            for flags in [0, 4, 8, 12, 1, 3, 9, 11] {
                let mut case = base(w, h, pattern, flags);
                case.input[10] = [0, 70, 100][(pattern + flags) as usize % 3];
                case.input[11] = (pattern + flags) % 7;
                case.input[12] = (pattern + flags) % 2;
                for prev in [[0, 0, w, h], [0, 0, w / 2, h / 2], [0, 0, 0, 0]] {
                    case.input[13..17].copy_from_slice(&prev);
                    cases.push(case.clone());
                }
            }
            for rect in [
                [0, 0, w, h],
                [w / 2, h / 2, w - w / 2, h - h / 2],
                [w, h, 0, 0],
            ] {
                cases.push(with_mask(base(w, h, pattern, 0), rect, 0));
                cases.push(with_mask(base(w, h, pattern, 0), rect, 1));
                cases.push(with_mask(base(w, h, pattern, 32), rect, 1));
            }
            for method in 0..7 {
                let mut case = base(w, h, pattern, if method % 2 == 0 { 16 } else { 0 });
                case.input[0] = 2;
                case.input[11] = method;
                case.input[12] = pattern % 2;
                cases.push(case);
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..13 {
        for flags in [0, 3, 4, 8] {
            cases.push(base(5, 7, pattern, flags));
        }
    }
    for pattern in [0, 3, 8] {
        for rect in [[0, 0, 5, 7], [1, 1, 3, 5], [5, 7, 0, 0]] {
            cases.push(with_mask(base(5, 7, pattern, 0), rect, 0));
            cases.push(with_mask(base(5, 7, pattern, 32), rect, 1));
        }
    }
    for method in 0..7 {
        let mut case = base(5, 7, 11, 16);
        case.input[0] = 2;
        case.input[11] = method;
        case.input[12] = method % 2;
        cases.push(case);
    }
    // Identical alternatives force an encoded-size tie; NONE is evaluated first.
    let mut tie = base(5, 7, 5, 8);
    tie.input[13..17].copy_from_slice(&[0, 0, 0, 0]);
    cases.push(tie);
    // BG wins with a nonempty carryover mask after NONE had its own mask.
    let mut background_carry = base(16, 16, 12, 8);
    background_carry.input[13..17].copy_from_slice(&[0, 0, 8, 8]);
    background_carry.input[12] = 1;
    cases.push(background_carry);
    cases
}
