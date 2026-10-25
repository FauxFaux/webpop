//! Version 1 of the existing AnimationCache request: flags bit 2 selects the
//! extended header/observations; bits 0/1 select minimize-size/allow-mixed.
use super::{Case, animation_cache, animation_mixed};
use libwebpop::mux::anim_encode::*;
use libwebpop::*;

pub struct Frame<'a> {
    pub duration: u32,
    pub stride: usize,
    pub config: MixedEncodingConfig,
    pub words: &'a [u32],
}

pub fn frames(case: &Case) -> Vec<Frame<'_>> {
    let a = &case.input;
    let mut at = 8;
    let mut result = Vec::new();
    for _ in 0..a[4] {
        let size = ((a[1] - 1) * a[at + 1] + a[0]) as usize;
        result.push(Frame {
            duration: a[at],
            stride: a[at + 1] as usize,
            config: MixedEncodingConfig {
                primary: if a[at + 2] != 0 {
                    AnimationCandidateCodec::Vp8l
                } else {
                    AnimationCandidateCodec::Vp8
                },
                codec: LossyOptions {
                    quality: f32::from_bits(a[at + 3]),
                    method: a[at + 4],
                    alpha_quality: a[at + 7] as i32,
                    use_sharp_yuv: a[at + 8] != 0,
                },
                near_lossless: a[at + 5] as i32,
                exact: a[at + 6] != 0,
            },
            words: &a[at + 9..at + 9 + size],
        });
        at += 9 + size;
    }
    assert_eq!(at, a.len());
    result
}

pub fn observation(cache: &AnimationCache) -> Vec<u32> {
    let mut result = animation_cache::status_words(cache.status()).to_vec();
    let (config, reversed) = cache.saved_configs().unwrap();
    result.extend(animation_mixed::config_words(config));
    result.extend(animation_mixed::config_words(reversed));
    if let Some(control) = cache.pending_control() {
        result.extend([
            control.x_offset,
            control.y_offset,
            u32::from(control.dispose == FrameDispose::Background),
            u32::from(control.blend == FrameBlend::Replace),
        ]);
    } else {
        result.extend([0; 4]);
    }
    let (previous, carryover) = cache.canvases();
    result.push(previous.len() as u32);
    result.extend(previous);
    result.extend(carryover);
    result
}

pub fn encode(case: &Case) -> (Vec<u32>, Vec<u8>) {
    let a = &case.input;
    let mut cache = AnimationCache::new(
        a[0],
        a[1],
        AnimationCacheOptions {
            minimize_size: a[2] & 1 != 0,
            allow_mixed: a[2] & 2 != 0,
            container: animation_cache::container_options(a[5] != 0),
        },
    )
    .unwrap();
    let mut timestamp = u64::from(a[6]) | (u64::from(a[7]) << 32);
    let mut states = Vec::new();
    for f in frames(case) {
        cache
            .add_argb(
                ArgbView::new(f.words, a[0] as i32, a[1] as i32, f.stride).unwrap(),
                timestamp,
                &f.config,
            )
            .unwrap();
        states.extend(observation(&cache));
        timestamp = timestamp.checked_add(u64::from(f.duration)).unwrap();
    }
    let mut output = vec![0xab; 3];
    cache.finish(timestamp, &mut output).unwrap();
    states.extend(observation(&cache));
    (states, output)
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    assert!(case.input[3] < 2);
    let (mut states, bytes) = encode(case);
    states.push(bytes.len() as u32);
    states.extend(bytes.into_iter().map(u32::from));
    states
}

pub fn state_size(case: &Case) -> usize {
    29 + 2 * (case.input[0] * case.input[1]) as usize
}
pub fn file_offset(case: &Case) -> usize {
    state_size(case) * case.input[4] as usize + 29
}
pub fn output_bytes(case: &Case, output: &[u32]) -> Vec<u8> {
    let at = file_offset(case);
    assert_eq!(output.len(), at + 1 + output[at] as usize);
    output[at + 1..].iter().map(|&v| v as u8).collect()
}

pub fn sequence(w: u32, h: u32, pattern: u32, method: u32, flags: u32, timing: u32) -> Case {
    let old = animation_mixed::sequence_case(w, h, pattern, method, flags & 1 != 0);
    let a = &old.input;
    let mut input = vec![w, h, 4 | flags, 0, a[3], timing % 2, 0xfffffff0, 0x1234];
    let mut at = 4;
    for frame in 0..a[3] {
        let stride = a[at];
        let duration = match timing {
            0 => [0, 1, 37, 0][frame as usize % 4],
            1 => [0xffffff, 1, 0, 0xffffff][frame as usize % 4],
            2 => 0xffffff,
            _ => 1,
        };
        input.push(duration);
        input.extend([
            a[at],
            a[at + 3],
            a[at + 1],
            a[at + 2],
            a[at + 4],
            a[at + 5],
            a[at + 6],
            a[at + 7],
        ]);
        at += 8;
        let size = ((h - 1) * stride + w) as usize;
        input.extend_from_slice(&a[at..at + size]);
        at += size;
    }
    Case::new("AnimationCache", input)
}

// Named corpus families. Repeat/similarity inputs vary settings even on skips.
pub fn repeats(primary: u32, mixed: bool, timing: u32, count: u32, similar: bool) -> Case {
    let (w, h) = (8, 8);
    let mut input = vec![
        w,
        h,
        4 | if mixed { 2 } else { 0 },
        0,
        count,
        1,
        0xfffffff0,
        0x1234,
    ];
    for i in 0..count {
        let duration = match timing {
            0 => {
                if i == 0 {
                    0xfffffe
                } else if i == 1 {
                    1
                } else {
                    0
                }
            }
            1 => {
                if i == 0 {
                    0xffffff
                } else if i == 1 {
                    1
                } else {
                    0
                }
            }
            _ => 0xffffff,
        };
        input.extend([
            duration,
            w + 1,
            if primary < 2 {
                primary
            } else {
                (i + primary - 2) % 2
            },
            (if i % 2 == 0 { 61.5f32 } else { 75.25 }).to_bits(),
            i % 7,
            if i % 3 == 0 { 60 } else { 100 },
            i % 2,
            if i % 2 == 0 { 100 } else { 60 },
            i % 2,
        ]);
        for y in 0..h {
            for _ in 0..w {
                input.push(if similar && i % 3 == 1 {
                    0xff304051
                } else {
                    0xff304050
                });
            }
            if y + 1 < h {
                input.push(0xdeadbeef);
            }
        }
    }
    Case::new("AnimationCache", input)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for mixed in [false, true] {
        for primary in 0..4 {
            for timing in 0..3 {
                cases.push(repeats(
                    primary,
                    mixed,
                    timing,
                    if timing == 2 { 16 } else { 3 },
                    false,
                ));
            }
            cases.push(repeats(primary, mixed, 1, 5, true));
        }
        for pattern in [2, 4, 6, 10, 11] {
            cases.push(sequence(
                19,
                17,
                pattern,
                pattern % 7,
                u32::from(mixed) * 2 | pattern % 2,
                1,
            ));
        }
    }
    for pattern in [2, 4, 6, 10, 11] {
        let mut case = sequence(19, 17, pattern, pattern % 7, 1, 1);
        let mut at = 8;
        for _ in 0..case.input[4] {
            let size = ((case.input[1] - 1) * case.input[at + 1] + case.input[0]) as usize;
            case.input[at + 2] = 0;
            at += 9 + size;
        }
        case.input[3] = 1;
        cases.push(case);
        let mut mixed = sequence(19, 17, pattern, pattern % 7, 3, 1);
        mixed.input[3] = 1;
        cases.push(mixed);
    }
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for (w, h) in [(1, 1), (1, 9), (9, 1), (19, 17)] {
        for pattern in 0..12 {
            for method in 0..7 {
                for flags in 0..4 {
                    cases.push(sequence(
                        w,
                        h,
                        pattern,
                        method,
                        flags,
                        (pattern + method) % 4,
                    ));
                }
            }
        }
    }
    cases
}
