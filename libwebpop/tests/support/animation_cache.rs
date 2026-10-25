use super::{Case, animation_carryover};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub struct Frame<'a> {
    pub duration: u32,
    pub stride: usize,
    pub codec: LosslessOptions,
    pub strength: i32,
    pub words: &'a [u32],
}

pub fn frames(case: &Case) -> Vec<Frame<'_>> {
    let a = &case.input;
    let mut at = 8;
    let mut result = Vec::new();
    let near_lossless = case.function == "AnimationNearLosslessCache";
    let header = if near_lossless { 6 } else { 5 };
    for _ in 0..a[4] {
        let size = ((a[1] - 1) * a[at + 1] + a[0]) as usize;
        result.push(Frame {
            duration: a[at],
            stride: a[at + 1] as usize,
            codec: LosslessOptions {
                quality: a[at + 2],
                method: a[at + 3],
                exact: a[at + 4] != 0,
            },
            strength: if near_lossless { a[at + 5] as i32 } else { 100 },
            words: &a[at + header..at + header + size],
        });
        at += header + size;
    }
    assert_eq!(at, a.len());
    result
}

pub fn container_options(metadata: bool) -> AnimationContainerOptions {
    if metadata {
        AnimationContainerOptions {
            loop_count: 65535,
            background: BackgroundColor {
                red: 12,
                green: 34,
                blue: 56,
                alpha: 78,
            },
            icc_profile: Some(vec![1, 2, 3]),
            exif: Some(vec![]),
            xmp: Some(vec![4]),
        }
    } else {
        Default::default()
    }
}

pub fn status_words(s: LosslessCacheStatus) -> [u32; 10] {
    [
        s.submitted_frames as u32,
        s.emitted_frames as u32,
        s.cached_frames as u32,
        s.count_since_key_frame as u32,
        s.previous_rect.x_offset as u32,
        s.previous_rect.y_offset as u32,
        s.previous_rect.width as u32,
        s.previous_rect.height as u32,
        u32::from(s.pending_duration_ms.is_some()),
        s.pending_duration_ms.unwrap_or(0),
    ]
}

pub fn encode(case: &Case) -> (Vec<u32>, Vec<u8>) {
    let a = &case.input;
    let mut encoder = LosslessAnimationCache::new(
        a[0],
        a[1],
        LosslessAnimationCacheOptions {
            minimize_size: a[2] != 0,
            container: container_options(a[5] != 0),
        },
    )
    .unwrap();
    let mut timestamp = u64::from(a[6]) | (u64::from(a[7]) << 32);
    let mut states = Vec::new();
    for f in frames(case) {
        let view = ArgbView::new(f.words, a[0] as i32, a[1] as i32, f.stride).unwrap();
        if case.function == "AnimationNearLosslessCache" {
            encoder
                .add_near_lossless_argb(view, timestamp, f.strength, &f.codec)
                .unwrap();
        } else {
            encoder.add_argb(view, timestamp, &f.codec).unwrap();
        }
        states.extend(status_words(encoder.status()));
        timestamp = timestamp.checked_add(u64::from(f.duration)).unwrap();
    }
    let mut output = vec![0xab; 3];
    encoder.finish(timestamp, &mut output).unwrap();
    states.extend(status_words(encoder.status()));
    (states, output)
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    if case.input[2] & 8 != 0 {
        return super::animation_keyframes::evaluate(case);
    }
    if case.input[2] & 4 != 0 {
        return super::animation_codec_cache::evaluate(case);
    }
    assert!(
        case.input[3] < 2,
        "mode 2 is a reference-only completion gate"
    );
    let (mut states, bytes) = encode(case);
    states.push(bytes.len() as u32);
    states.extend(bytes.into_iter().map(u32::from));
    states
}

pub fn file_offset(case: &Case) -> usize {
    10 * (case.input[4] as usize + 1)
}

pub fn output_bytes(case: &Case, output: &[u32]) -> Vec<u8> {
    let at = file_offset(case);
    let len = output[at] as usize;
    output[at + 1..at + 1 + len]
        .iter()
        .map(|&v| v as u8)
        .collect()
}

pub fn case(
    w: u32,
    h: u32,
    pattern: u32,
    method: u32,
    timing: u32,
    minimize: bool,
    count: u32,
) -> Case {
    let base = animation_carryover::sequence_case(w, h, pattern, method, minimize);
    let mut at = 4;
    let mut originals = Vec::new();
    for _ in 0..8 {
        let size = ((h - 1) * base.input[at] + w) as usize;
        originals.push(&base.input[at..at + 4 + size]);
        at += 4 + size;
    }
    let mut input = vec![
        w,
        h,
        u32::from(minimize),
        0,
        count,
        timing % 2,
        if timing % 2 == 0 {
            0x7ffffff0
        } else {
            0xfffffff0
        },
        0x1234,
    ];
    for frame in 0..count {
        let duration = match timing {
            0 => [0, 1, 37, 0][frame as usize % 4],
            1 => [0xffffff, 1, 0, 0xffffff][frame as usize % 4],
            2 => {
                if frame == 0 {
                    0xffffff
                } else if frame == 1 {
                    1
                } else {
                    0
                }
            }
            3 => {
                if frame == 0 {
                    0xfffffe
                } else if frame == 1 {
                    1
                } else {
                    0
                }
            }
            _ => 0xffffff,
        };
        input.push(duration);
        input.extend_from_slice(originals[frame as usize % 8]);
    }
    Case::new("AnimationCache", input)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..8 {
        for timing in 0..3 {
            let mut case = case(8, 7, pattern, pattern % 7, timing, timing % 2 != 0, 8);
            if pattern != 0 {
                case.input[3] = 1;
            }
            cases.push(case);
        }
    }
    for pattern in [0, 1] {
        for timing in [2, 3, 4] {
            cases.push(case(1, 1, pattern, 6, timing, true, 2));
        }
    }
    cases.push(case(16, 16, 7, 4, 1, true, 16));
    cases.push(case(1, 1, 1, 0, 4, false, 96));
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 9), (9, 1), (5, 7), (16, 16), (32, 17)] {
        for pattern in 0..8 {
            for method in 0..7 {
                for timing in 0..5 {
                    let mut case = case(w, h, pattern, method, timing, method % 2 != 0, 8);
                    if pattern != 0 {
                        case.input[3] = 1;
                    }
                    cases.push(case);
                }
            }
        }
    }
    cases
}
