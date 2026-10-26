//! Public RGBA mapping onto the existing C animation-cache protocol.
use crate::support::{
    Case, animation_cache, animation_codec_cache as codec, animation_keyframes as key,
    animation_mixed,
};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub fn base(case: &Case) -> Case {
    if case.input[2] & 8 != 0 {
        key::base(case)
    } else {
        case.clone()
    }
}

pub fn frame_options(config: MixedEncodingConfig, mixed: bool) -> AnimationFrameOptions {
    let ordinary = match config.primary {
        AnimationCandidateCodec::Vp8l => {
            config.codec.quality.fract() == 0.
                && config.codec.alpha_quality == 100
                && !config.codec.use_sharp_yuv
        }
        AnimationCandidateCodec::Vp8 => config.near_lossless == 100 && !config.exact,
    };
    if mixed && !ordinary {
        AnimationFrameOptions::Mixed {
            primary: config.primary,
            options: config.codec,
            near_lossless: config.near_lossless,
            exact: config.exact,
        }
    } else if config.primary == AnimationCandidateCodec::Vp8l {
        assert_eq!(config.codec.quality.fract(), 0.);
        assert_eq!(config.codec.alpha_quality, 100);
        assert!(!config.codec.use_sharp_yuv);
        let options = LosslessOptions {
            quality: config.codec.quality as u32,
            method: config.codec.method,
            exact: config.exact,
        };
        if config.near_lossless == 100 {
            AnimationFrameOptions::Lossless(options)
        } else {
            AnimationFrameOptions::NearLossless {
                strength: config.near_lossless,
                options,
            }
        }
    } else {
        assert_eq!(config.near_lossless, 100);
        assert!(!config.exact);
        AnimationFrameOptions::Lossy(config.codec)
    }
}

pub fn encode(case: &Case) -> Vec<u8> {
    let a = &case.input;
    let mut encoder = AnimationEncoder::new(
        a[0],
        a[1],
        AnimationOptions {
            policy: AnimationEncodingPolicy::Optimized,
            container: animation_cache::container_options(a[5] != 0),
            optimizer: AnimationOptimizerOptions {
                minimize_size: a[2] & 1 != 0,
                allow_mixed: a[2] & 2 != 0,
                keyframes: if a[2] & 8 != 0 {
                    key::policy(case)
                } else {
                    AnimationKeyframePolicy::Disabled
                },
            },
        },
    )
    .unwrap();
    let base = base(case);
    let mut timestamp = u64::from(a[6]) | (u64::from(a[7]) << 32);
    for (i, frame) in codec::frames(&base).iter().enumerate() {
        // Byte padding deliberately need not align to ARGB words. The last row
        // has no padding. Reusing the buffer after add verifies owned history.
        let stride = a[0] as usize * 4 + i % 7;
        let mut rgba = vec![0xa5; (a[1] as usize - 1) * stride + a[0] as usize * 4];
        for y in 0..a[1] as usize {
            for x in 0..a[0] as usize {
                let [alpha, red, green, blue] = frame.words[y * frame.stride + x].to_be_bytes();
                rgba[y * stride + x * 4..][..4].copy_from_slice(&[red, green, blue, alpha]);
            }
        }
        let original = rgba.clone();
        encoder
            .add_rgba(
                &rgba,
                stride,
                timestamp,
                &frame_options(frame.config, a[2] & 2 != 0),
            )
            .unwrap();
        assert_eq!(rgba, original);
        rgba.fill(0x99);
        timestamp = timestamp.checked_add(u64::from(frame.duration)).unwrap();
    }
    let mut output = vec![0xab; 3];
    encoder.finish(timestamp, &mut output).unwrap();
    output
}

// Extract complete files from existing V2/V3/V4 observations. No candidate or
// timestamp selection is repeated here; all observations precede the files.
pub fn file_bytes(case: &Case, words: &[u32]) -> Vec<u8> {
    if case.input[2] & 8 == 0 {
        return codec::output_bytes(case, words);
    }
    let mut at = 0;
    let snapshot = |at: &mut usize| {
        let count = words[*at + 2] as usize;
        let history = if case.input[2] & 16 != 0 && words[*at] != 0 {
            19 + if count != 0 {
                2 * (case.input[0] * case.input[1]) as usize
            } else {
                0
            }
        } else {
            0
        };
        *at += 19 + count * 17 + history;
    };
    for _ in 0..case.input[4] {
        snapshot(&mut at);
        let trials = words[at];
        at += 1;
        for _ in 0..trials {
            at += 12 + words[at + 11] as usize;
        }
        snapshot(&mut at);
    }
    snapshot(&mut at);
    if case.input[2] & 32 != 0 {
        snapshot(&mut at);
        snapshot(&mut at);
    }
    snapshot(&mut at);
    let len = words[at] as usize;
    assert_eq!(at + 1 + len, words.len());
    words[at + 1..].iter().map(|&v| v as u8).collect()
}

// Map dormant settings to the published defaults for the three established
// variants. Mixed requests retain every original field, including fractions.
fn normalize(source: Case, min: u32, max: u32) -> Case {
    let base = base(&source);
    let a = &source.input;
    let mut input = a[..8].to_vec();
    input[2] = (a[2] & 3) | 4 | 8;
    input[3] = 0;
    input.extend([min, max]);
    for f in codec::frames(&base) {
        let mut c = f.config;
        if a[2] & 2 == 0 {
            if c.primary == AnimationCandidateCodec::Vp8l {
                c.codec.quality = c.codec.quality.trunc();
                c.codec.alpha_quality = 100;
                c.codec.use_sharp_yuv = false;
            } else {
                c.near_lossless = 100;
                c.exact = false;
            }
        }
        input.extend([f.duration, f.stride as u32]);
        input.extend(animation_mixed::config_words(c));
        input.extend(f.words);
    }
    Case::new("AnimationCache", input)
}

pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (width, height) in [(1, 1), (1, 9), (9, 1), (19, 17)] {
        for pattern in [2, 6, 10, 11] {
            for method in [0, 4, 6] {
                for flags in [0, 2, 3] {
                    let (min, max) = match method {
                        0 => (0, 0),
                        4 => (2, 5),
                        _ => (0, 1),
                    };
                    result.push(normalize(
                        codec::sequence(width, height, pattern, method, flags, pattern % 4),
                        min,
                        max,
                    ));
                }
            }
        }
    }
    for mixed in [false, true] {
        for primary in 0..4 {
            result.push(normalize(codec::repeats(primary, mixed, 1, 5, true), 2, 5));
        }
    }
    for case in key::selection_fixtures()
        .into_iter()
        .chain(key::overflow_fixtures())
    {
        let (min, max) = (case.input[8], case.input[9]);
        result.push(normalize(case, min, max));
    }
    result.extend(reconstruction_cases());
    result
}

pub fn reconstruction_cases() -> Vec<Case> {
    let mut result = Vec::new();
    for count in [1, 8] {
        for route in 0..3 {
            let mut input = vec![8, 7, 12, 0, count, 0, 0xfffffff0, 0x1234, 0, 0];
            for _ in 0..count {
                let config = MixedEncodingConfig {
                    primary: if route == 2 {
                        AnimationCandidateCodec::Vp8
                    } else {
                        AnimationCandidateCodec::Vp8l
                    },
                    near_lossless: if route == 1 { 60 } else { 100 },
                    ..Default::default()
                };
                input.extend([1, 9]);
                input.extend(animation_mixed::config_words(config));
                input.extend([0; 62]); // (height-1)*9+width, including row padding.
            }
            result.push(Case::new("AnimationCache", input));
        }
    }
    result
}

pub fn fixtures() -> Vec<Case> {
    let cases = cases();
    [
        0, 1, 2, 6, 9, 18, 36, 45, 72, 81, 108, 117, 135, 136, 137, 144, 145, 147, 148, 150, 153,
        160, 174, 175, 178, 180, 183, 187, 190, 194, 198, 203, 204, 205, 206, 207, 208,
    ]
    .into_iter()
    .map(|i| cases[i].clone())
    .collect()
}
