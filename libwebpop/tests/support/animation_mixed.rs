//! Mixed-mode trials, shared/reversed config and original CacheFrame snapshots.
use super::{Case, animation_lossless as ll, animation_lossy_carryover as vp8};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub fn config_words(c: MixedEncodingConfig) -> Vec<u32> {
    vec![
        u32::from(c.primary == AnimationCandidateCodec::Vp8l),
        c.codec.quality.to_bits(),
        c.codec.method,
        c.near_lossless as u32,
        u32::from(c.exact),
        c.codec.alpha_quality as u32,
        u32::from(c.codec.use_sharp_yuv),
    ]
}

pub fn base(case: &Case) -> Case {
    Case::new("AnimationLossless", case.input[4..].to_vec())
}

pub fn options(case: &Case) -> MixedCandidateOptions {
    let a = &case.input[4..];
    MixedCandidateOptions {
        codec: MixedEncodingConfig {
            primary: if case.input[0] != 0 {
                AnimationCandidateCodec::Vp8l
            } else {
                AnimationCandidateCodec::Vp8
            },
            codec: LossyOptions {
                quality: f32::from_bits(a[10]),
                method: a[11],
                alpha_quality: case.input[2] as i32,
                use_sharp_yuv: case.input[3] != 0,
            },
            near_lossless: case.input[1] as i32,
            exact: a[12] != 0,
        },
        is_key_frame: a[9] & 1 != 0,
        is_first_frame: a[9] & 2 != 0,
        previous_candidate_undecided: a[9] & 4 != 0,
        minimize_size: a[9] & 8 != 0,
    }
}

pub fn candidate_words(c: MixedCandidate) -> Vec<u32> {
    match c.codec {
        AnimationCandidateCodec::Vp8l => ll::candidate_words(LosslessCandidate {
            frame: c.frame,
            rect: c.rect,
            control: c.control,
            previous_dispose: c.previous_dispose,
            carryover_mask: c.carryover_mask,
        }),
        AnimationCandidateCodec::Vp8 => vp8::candidate_words(LossyCandidate {
            frame: c.frame,
            rect: c.rect,
            control: c.control,
            previous_dispose: c.previous_dispose,
            carryover_mask: c.carryover_mask,
        }),
    }
}

pub fn file_words(words: &[u32]) -> &[u32] {
    &words[10 + words[8] as usize..]
}

pub struct Trial {
    pub codec: AnimationCandidateCodec,
    pub dispose: FrameDispose,
    pub words: Vec<u32>,
    pub record: Vec<u32>,
}

// Rebuild each isolated trial using public pixel/codec leaves. The native
// observer records actual WebPEncode invocations, including every loser's bytes.
fn trial(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    rect: FrameRectangle,
    dispose: FrameDispose,
    options: &MixedCandidateOptions,
    codec: AnimationCandidateCodec,
) -> Trial {
    let config = options.codec;
    let mut pixels: Vec<_> = (0..current.height() as usize)
        .flat_map(|y| current.row(y).unwrap().iter().copied())
        .collect();
    let mut target = ArgbViewMut::new(
        &mut pixels,
        current.width(),
        current.height(),
        current.width() as usize,
    )
    .unwrap();
    let mut mask = vec![0; (rect.width * rect.height) as usize];
    let blending = !options.is_key_frame
        && match codec {
            AnimationCandidateCodec::Vp8l => IsLosslessBlendingPossible(previous, current, &rect),
            AnimationCandidateCodec::Vp8 => {
                IsLossyBlendingPossible(previous, current, &rect, config.codec.quality)
            }
        };
    let carries = if blending {
        match codec {
            AnimationCandidateCodec::Vp8l => {
                IncreaseTransparency(previous, &rect, &mut target, &mut mask).unwrap()
            }
            AnimationCandidateCodec::Vp8 => FlattenSimilarBlocks(
                previous,
                &rect,
                &mut target,
                config.codec.quality,
                &mut mask,
            )
            .unwrap(),
        }
    } else {
        false
    };
    let sub = target.as_view().sub_view(rect).unwrap();
    let mut c = encode_mixed_candidate(sub, rect, &config, codec, blending).unwrap();
    if !options.is_key_frame {
        c.previous_dispose = Some(dispose);
    }
    if carries {
        c.carryover_mask = mask;
    }
    let mut record = vec![
        u32::from(codec == AnimationCandidateCodec::Vp8l),
        rect.width as u32,
        rect.height as u32,
        config.codec.quality.to_bits(),
        config.codec.method,
        config.near_lossless as u32,
        u32::from(config.exact),
        config.codec.alpha_quality as u32,
        u32::from(config.codec.use_sharp_yuv),
        if codec == AnimationCandidateCodec::Vp8 && blending {
            0
        } else {
            60
        },
        0,
        c.encoded_size() as u32,
    ];
    let words = candidate_words(c);
    record.extend(file_words(&words));
    Trial {
        codec,
        dispose,
        words,
        record,
    }
}

pub fn trials(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &MixedCandidateOptions,
) -> Vec<Trial> {
    let mut none = SubFrameParams::new(true, !options.is_first_frame);
    GetSubRects(
        previous,
        current,
        options.is_key_frame,
        options.is_first_frame,
        options.codec.codec.quality,
        &mut none,
    )
    .unwrap();
    if IsEmptyRect(&none.rect_ll) || IsEmptyRect(&none.rect_lossy) {
        return vec![];
    }
    let mut disposed: Vec<_> = (0..previous.height() as usize)
        .flat_map(|y| previous.row(y).unwrap().iter().copied())
        .collect();
    let mut bg = SubFrameParams::new(false, false);
    let mut bg_view = ArgbViewMut::new(
        &mut disposed,
        previous.width(),
        previous.height(),
        previous.width() as usize,
    )
    .unwrap();
    if !options.is_key_frame && !options.previous_candidate_undecided {
        DisposeFrameRectangle(FrameDispose::Background, &previous_rect, &mut bg_view).unwrap();
        GetSubRects(
            bg_view.as_view(),
            current,
            options.is_key_frame,
            options.is_first_frame,
            options.codec.codec.quality,
            &mut bg,
        )
        .unwrap();
        let area = |p: &SubFrameParams<'_>| {
            let r = if options.codec.primary == AnimationCandidateCodec::Vp8l {
                p.rect_ll
            } else {
                p.rect_lossy
            };
            r.width as u64 * r.height as u64
        };
        if options.minimize_size {
            bg.should_try = true;
        } else if area(&bg) < area(&none) {
            bg.should_try = true;
            none.should_try = false;
        }
    }
    let mut result = Vec::new();
    for (src, params, dispose) in [
        (previous, none, FrameDispose::None),
        (bg_view.as_view(), bg, FrameDispose::Background),
    ] {
        if !params.should_try {
            continue;
        }
        let eligible =
            mixed_candidate_eligibility(params.sub_frame_ll.unwrap(), options.minimize_size);
        if eligible.lossless {
            result.push(trial(
                src,
                current,
                params.rect_ll,
                dispose,
                options,
                AnimationCandidateCodec::Vp8l,
            ));
        }
        if eligible.lossy {
            result.push(trial(
                src,
                current,
                params.rect_lossy,
                dispose,
                options,
                AnimationCandidateCodec::Vp8,
            ));
        }
    }
    result
}

pub fn observation(config: MixedEncodingConfig, trials: &[Trial], words: Vec<u32>) -> Vec<u32> {
    let mut result = config_words(config);
    result.extend(config_words(config.reversed()));
    result.push(trials.len() as u32);
    for trial in trials {
        result.extend(&trial.record);
    }
    result.extend(words);
    result
}

pub fn candidate(case: &Case) -> Option<MixedCandidate> {
    let b = base(case);
    let a = &b.input;
    let (s, d) = ll::canvases(&b);
    SetFrameMixed(
        ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        ArgbView::new(d, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
        ll::rectangle(&a[13..]),
        &options(case),
    )
    .unwrap()
}

pub struct Step {
    pub candidate: Option<MixedCandidate>,
    pub current: Vec<u32>,
    pub previous: Vec<u32>,
    pub carryover: Vec<u32>,
    pub previous_rect: FrameRectangle,
    pub config: MixedEncodingConfig,
    pub trials: Vec<Trial>,
}

pub fn sequence_steps(case: &Case) -> Vec<Step> {
    let a = &case.input;
    let (w, h) = (a[0] as i32, a[1] as i32);
    let mut previous = vec![0; (w * h) as usize];
    let mut carry = previous.clone();
    let mut previous_rect = FrameRectangle::default();
    let mut first = true;
    let mut at = 4;
    let mut result = Vec::new();
    for _ in 0..a[3] {
        let stride = a[at] as usize;
        let config = MixedEncodingConfig {
            primary: if a[at + 3] != 0 {
                AnimationCandidateCodec::Vp8l
            } else {
                AnimationCandidateCodec::Vp8
            },
            codec: LossyOptions {
                quality: f32::from_bits(a[at + 1]),
                method: a[at + 2],
                alpha_quality: a[at + 6] as i32,
                use_sharp_yuv: a[at + 7] != 0,
            },
            near_lossless: a[at + 4] as i32,
            exact: a[at + 5] != 0,
        };
        at += 8;
        let size = (h as usize - 1) * stride + w as usize;
        let current = ArgbView::new(&a[at..at + size], w, h, stride).unwrap();
        at += size;
        if !first {
            CopyIdenticalPixels(
                ArgbView::new(&previous, w, h, w as usize).unwrap(),
                current,
                &mut ArgbViewMut::new(&mut carry, w, h, w as usize).unwrap(),
            )
            .unwrap();
        }
        let options = MixedCandidateOptions {
            codec: config,
            is_key_frame: first,
            is_first_frame: first,
            minimize_size: a[2] != 0,
            ..Default::default()
        };
        let src = ArgbView::new(&carry, w, h, w as usize).unwrap();
        let trials = trials(src, current, previous_rect, &options);
        let candidate = SetFrameMixed(src, current, previous_rect, &options).unwrap();
        if let Some(c) = &candidate {
            c.update_canvases(
                current,
                previous_rect,
                &mut ArgbViewMut::new(&mut previous, w, h, w as usize).unwrap(),
                &mut ArgbViewMut::new(&mut carry, w, h, w as usize).unwrap(),
            )
            .unwrap();
            previous_rect = c.rect;
            first = false;
        }
        result.push(Step {
            candidate,
            current: (0..h as usize)
                .flat_map(|y| current.row(y).unwrap().iter().copied())
                .collect(),
            previous: previous.clone(),
            carryover: carry.clone(),
            previous_rect,
            config,
            trials,
        });
    }
    assert_eq!(at, a.len());
    result
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    if case.function == "AnimationMixedCarryoverSequence" {
        let mut result = Vec::new();
        for step in sequence_steps(case) {
            result.extend(observation(
                step.config,
                &step.trials,
                step.candidate.map_or(vec![0], candidate_words),
            ));
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
    let b = base(case);
    let a = &b.input;
    let (s, d) = ll::canvases(&b);
    let options = options(case);
    let trials = trials(
        ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        ArgbView::new(d, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
        ll::rectangle(&a[13..]),
        &options,
    );
    observation(
        options.codec,
        &trials,
        candidate(case).map_or(vec![0], candidate_words),
    )
}

pub fn palette_case(
    count: u32,
    primary: bool,
    minimize: bool,
    method: u32,
    quality: f32,
    first: bool,
) -> Case {
    let mut b = ll::base(
        19,
        17,
        5,
        if first { 3 } else { 0 } | if minimize { 8 } else { 0 },
    );
    b.input[10] = quality.to_bits();
    b.input[11] = method;
    let (w, h, ss, ds) = (b.input[1], b.input[2], b.input[3], b.input[4]);
    let size = ((h - 1) * ss + w) as usize;
    for y in 0..h {
        for x in 0..w {
            let color = 0xff000000 | ((((y * w + x) % count) * 0x010307) & 0xffffff);
            b.input[18 + (y * ss + x) as usize] = if first {
                0
            } else if (x == 0 && y == 0) || (x == w - 1 && y == h - 1) {
                color ^ 0x808080
            } else {
                color
            };
            b.input[18 + size + (y * ds + x) as usize] = color;
        }
    }
    let mut input = vec![u32::from(primary), 100, 100, 0];
    input.extend(b.input);
    Case::new("AnimationMixed", input)
}

pub fn selection_case(
    w: u32,
    h: u32,
    pattern: u32,
    method: u32,
    primary: bool,
    flags: u32,
) -> Case {
    let old = vp8::selection_case(w, h, pattern, flags, method);
    let mut input = vec![
        u32::from(primary),
        if pattern % 2 == 0 { 100 } else { 60 },
        old.input[0],
        old.input[1],
    ];
    input.extend_from_slice(&old.input[2..]);
    input[16] = pattern % 2;
    Case::new("AnimationMixed", input)
}

pub fn primary_area_case(primary: bool) -> Case {
    let mut case = palette_case(2, primary, false, 4, 75., false);
    let a = &mut case.input[4..];
    let (w, h, ss, ds) = (a[1], a[2], a[3], a[4]);
    let size = ((h - 1) * ss + w) as usize;
    for y in 0..h {
        for x in 0..w {
            let inside = (8..10).contains(&x) && (8..10).contains(&y);
            a[18 + (y * ss + x) as usize] = if inside { 0xff304050 } else { 0x00123456 };
            a[18 + size + (y * ds + x) as usize] = if inside {
                if x == 8 && y == 8 {
                    0xffabcdef
                } else {
                    0xff304050
                }
            } else {
                0
            };
        }
    }
    case
}

pub fn fractional_case(primary: bool) -> Case {
    let mut case = palette_case(1, primary, true, 4, 61.5, false);
    let a = &mut case.input[4..];
    let (w, h, ss, ds) = (a[1], a[2], a[3], a[4]);
    let size = ((h - 1) * ss + w) as usize;
    for y in 0..h {
        for x in 0..w {
            a[18 + (y * ss + x) as usize] = 0xff304050;
            a[18 + size + (y * ds + x) as usize] = 0xff304058;
        }
    }
    a[13..17].fill(0);
    case
}

pub fn exact_case(exact: bool, sharp: bool) -> Case {
    let mut case = palette_case(31, true, true, 4, 75., false);
    case.input[3] = u32::from(sharp);
    let a = &mut case.input[4..];
    a[9] = 9; // Later keyframe: force full canvas, both codecs.
    a[12] = u32::from(exact);
    let (w, h, ss, ds) = (a[1], a[2], a[3], a[4]);
    let size = ((h - 1) * ss + w) as usize;
    for y in 0..h {
        for x in 0..w {
            a[18 + (y * ss + x) as usize] = 0;
            let rgb = ((x * 7919 + y * 104729) * 257) & 0xffffff;
            a[18 + size + (y * ds + x) as usize] =
                rgb | if x < 4 && y < 4 { 0xff000000 } else { 0 };
        }
    }
    case
}

pub fn tie_case() -> Case {
    // With minimize-size, 30 colors still try VP8. Both files are 118 bytes,
    // and VP8L must win even though the submitted primary codec is VP8.
    palette_case(30, false, true, 1, 3., true)
}

pub fn sequence_case(w: u32, h: u32, pattern: u32, method: u32, minimize: bool) -> Case {
    let old = vp8::sequence_case(w, h, pattern.min(10), method, minimize);
    let a = &old.input;
    let mut input = a[..4].to_vec();
    let mut at = 4;
    for frame in 0..a[3] {
        let stride = a[at];
        input.extend([
            stride,
            a[at + 1],
            a[at + 2],
            (frame + pattern) % 2,
            if frame % 3 == 0 { 100 } else { 60 },
            frame % 2,
            a[at + 3],
            a[at + 4],
        ]);
        at += 5;
        let size = ((h - 1) * stride + w) as usize;
        let start = input.len();
        input.extend_from_slice(&a[at..at + size]);
        at += size;
        if pattern == 11 {
            let colors = [2, 257, 31, 194, 30, 193, 1, 256][frame as usize];
            for y in 0..h {
                for x in 0..w {
                    input[start + (y * stride + x) as usize] =
                        0xff000000 | ((((y * w + x) % colors) * 0x010307) & 0xffffff);
                }
            }
        }
    }
    Case::new("AnimationMixedCarryoverSequence", input)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for count in [1, 2, 30, 31, 32, 193, 194, 195, 256, 257] {
        for primary in [false, true] {
            for minimize in [false, true] {
                cases.push(palette_case(count, primary, minimize, 4, 75., true));
            }
        }
    }
    for method in 0..7 {
        for primary in [false, true] {
            cases.push(selection_case(25, 23, 10, method, primary, 8));
        }
    }
    for pattern in [2, 4, 6, 10, 11] {
        for minimize in [false, true] {
            cases.push(sequence_case(19, 17, pattern, pattern % 7, minimize));
        }
    }
    for primary in [false, true] {
        cases.push(primary_area_case(primary));
        cases.push(fractional_case(primary));
        cases.push(palette_case(194, primary, false, 4, 75., false));
    }
    for exact in [false, true] {
        for sharp in [false, true] {
            cases.push(exact_case(exact, sharp));
        }
    }
    cases.push(tie_case());
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for count in [1, 2, 30, 31, 32, 193, 194, 195, 256, 257] {
        for method in 0..7 {
            for primary in [false, true] {
                for minimize in [false, true] {
                    for quality in [49.999996, 50., 99.99999, 100.] {
                        cases.push(palette_case(
                            count, primary, minimize, method, quality, true,
                        ));
                    }
                }
            }
        }
    }
    for (w, h) in [(1, 1), (19, 17), (25, 23)] {
        for pattern in 0..11 {
            for method in 0..7 {
                for primary in [false, true] {
                    for flags in [0, 1, 3, 4, 8, 12] {
                        cases.push(selection_case(w, h, pattern, method, primary, flags));
                    }
                }
            }
        }
    }
    for (w, h) in [(1, 1), (19, 17)] {
        for pattern in 0..12 {
            for method in 0..7 {
                for minimize in [false, true] {
                    cases.push(sequence_case(w, h, pattern, method, minimize));
                }
            }
        }
    }
    for primary in [false, true] {
        cases.push(primary_area_case(primary));
        cases.push(fractional_case(primary));
    }
    for exact in [false, true] {
        for sharp in [false, true] {
            cases.push(exact_case(exact, sharp));
        }
    }
    cases.push(tie_case());
    cases
}
