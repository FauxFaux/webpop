//! Normalization and versioned cache observations, including deferred selection.
use super::{Case, animation_cache, animation_codec_cache as codec, animation_mixed};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub fn options(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let effective = SanitizeEncoderOptions(a[0] as i32, a[1] as i32, a[2] != 0);
    vec![
        effective.kmin as u32,
        effective.kmax as u32,
        effective.capacity as u32,
        0,
        0,
        0,
        0,
        1,
        u32::MAX,
        0,
        0,
    ]
}

pub fn snapshot(cache: &AnimationCache) -> Vec<u32> {
    let mut result = animation_cache::status_words(cache.status()).to_vec();
    let s = cache.window_status();
    result.extend([
        s.effective.kmin as u32,
        s.effective.kmax as u32,
        s.effective.capacity as u32,
        0,
        s.flush_count as u32,
        s.best_penalty as u32,
        ((s.best_penalty as u64) >> 32) as u32,
        s.selected_keyframe.map_or(u32::MAX, |i| i as u32),
        u32::from(s.previous_candidate_undecided),
    ]);
    for f in cache.cached_candidates() {
        result.push(u32::from(f.is_key_frame));
        for alt in [f.sub_frame, f.key_frame] {
            let c = alt.control;
            result.extend([
                c.duration_ms,
                c.x_offset,
                c.y_offset,
                u32::from(c.dispose == FrameDispose::Background),
                u32::from(c.blend == FrameBlend::Replace),
                alt.width,
                alt.height,
                u32::try_from(alt.encoded_size).unwrap(),
            ]);
        }
    }
    result
}

pub fn base(case: &Case) -> Case {
    let mut input = case.input[..8].to_vec();
    input[2] &= !(8 | 16 | 32);
    input.extend_from_slice(&case.input[10..]);
    Case::new("AnimationCache", input)
}

pub fn is_disabled(case: &Case) -> bool {
    let a = &case.input;
    SanitizeEncoderOptions(a[8] as i32, a[9] as i32, a[2] & 1 != 0)
        == SanitizeEncoderOptions(0, 0, false)
}

pub fn policy(case: &Case) -> KeyframePolicy {
    match (case.input[8], case.input[9]) {
        (_, 0) => KeyframePolicy::Disabled,
        (_, 1) => KeyframePolicy::AllFrames,
        (min, max) => KeyframePolicy::Bounded {
            min_interval: min,
            max_interval: max,
        },
    }
}

fn observation(cache: &AnimationCache, case: &Case) -> Vec<u32> {
    let mut result = snapshot(cache);
    if case.input[2] & 16 != 0 && cache.status().submitted_frames != 0 {
        result.extend(&codec::observation(cache)[10..]);
    }
    result
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let base = base(case);
    let mut cache = AnimationCache::new_with_keyframes(
        a[0],
        a[1],
        AnimationCacheOptions {
            minimize_size: a[2] & 1 != 0,
            allow_mixed: a[2] & 2 != 0,
            container: animation_cache::container_options(a[5] != 0),
        },
        policy(case),
    )
    .unwrap();
    let mut timestamp = u64::from(a[6]) | (u64::from(a[7]) << 32);
    let mut result = Vec::new();
    let frames = codec::frames(&base);
    for (i, f) in frames.iter().enumerate() {
        result.extend(observation(&cache, case));
        // Reuse the existing isolated-trial evaluator to compare every original
        // WebPEncode call, including losers. Timing overflow may reset prev_rect.
        let mut rect = cache.status().previous_rect;
        let split = cache.status().pending_duration_ms.is_some_and(|d| {
            let previous_duration = frames[i - 1].duration;
            d + previous_duration >= 1 << 24
        });
        let mut distance = cache.status().count_since_key_frame;
        if split {
            distance = cache
                .window_status()
                .selected_keyframe
                .map_or(distance + 1, |selected| {
                    cache.status().cached_frames - selected
                });
            rect = FrameRectangle {
                width: 1,
                height: 1,
                ..Default::default()
            };
        }
        let (previous, carryover) = cache.canvases();
        let current = ArgbView::new(f.words, a[0] as i32, a[1] as i32, f.stride).unwrap();
        let mut restored = carryover.to_vec();
        if i != 0 {
            CopyIdenticalPixels(
                ArgbView::new(previous, a[0] as i32, a[1] as i32, a[0] as usize).unwrap(),
                current,
                &mut ArgbViewMut::new(&mut restored, a[0] as i32, a[1] as i32, a[0] as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        let settings = MixedCandidateOptions {
            codec: f.config,
            is_key_frame: i == 0,
            is_first_frame: i == 0,
            minimize_size: a[2] & 1 != 0,
            previous_candidate_undecided: !split
                && cache.window_status().previous_candidate_undecided,
        };
        let previous_view =
            ArgbView::new(&restored, a[0] as i32, a[1] as i32, a[0] as usize).unwrap();
        let mut trials = animation_mixed::trials_for_codecs(
            ArgbView::new(&restored, a[0] as i32, a[1] as i32, a[0] as usize).unwrap(),
            current,
            rect,
            &settings,
            a[2] & 2 != 0,
        );
        if i != 0
            && !trials.is_empty()
            && distance as i64 + 1 > i64::from(cache.window_status().effective.kmin)
        {
            trials.extend(animation_mixed::trials_for_codecs(
                previous_view,
                current,
                rect,
                &MixedCandidateOptions {
                    is_key_frame: true,
                    ..settings
                },
                a[2] & 2 != 0,
            ));
        }
        result.push(trials.len() as u32);
        for trial in trials {
            result.extend(trial.record);
        }
        cache.add_argb(current, timestamp, &f.config).unwrap();
        result.extend(observation(&cache, case));
        timestamp += u64::from(f.duration);
    }
    result.extend(observation(&cache, case));
    let mut output = Vec::new();
    cache.finish(timestamp, &mut output).unwrap();
    result.extend(observation(&cache, case));
    result.push(output.len() as u32);
    result.extend(output.into_iter().map(u32::from));
    result
}

pub fn option_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let values = [
        i32::MIN,
        -31,
        -1,
        0,
        1,
        2,
        3,
        4,
        5,
        15,
        30,
        31,
        32,
        59,
        60,
        61,
        100,
        i32::MAX - 1,
        i32::MAX,
    ];
    for min in values {
        for max in values {
            for minimize in [false, true] {
                cases.push(Case::new(
                    "AnimationKeyframeOptions",
                    vec![min as u32, max as u32, u32::from(minimize)],
                ));
            }
        }
    }
    cases
}

pub fn traces() -> Vec<Case> {
    let mut result = Vec::new();
    for timing in [0, 1, 2] {
        let old = codec::repeats(2, true, timing, 4, false);
        let mut input = old.input[..8].to_vec();
        input[2] |= 8;
        input.extend([0, 0]);
        input.extend_from_slice(&old.input[8..]);
        result.push(Case::new("AnimationCache", input));
    }
    for (min, max, flags, pattern) in [
        (0, 0, 2, 10),
        (0, 1, 3, 11),
        (0, 1, 2, 10),
        (0, 3, 2, 10),
        (2, 5, 2, 11),
    ] {
        let old = codec::sequence(19, 17, pattern, 4, flags, 3);
        let mut input = old.input[..8].to_vec();
        input[2] |= 8;
        input.extend([min as u32, max as u32]);
        input.extend_from_slice(&old.input[8..]);
        result.push(Case::new("AnimationCache", input));
    }
    result
}

/// New ordinary-duration corpus; V3 adds original/carryover and saved configs.
pub fn selection_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (min, max) in [(0, 1), (0, 2), (0, 3), (2, 5), (3, 7), (0, 100)] {
        for flags in [0, 2, 3] {
            for pattern in [2, 4, 6, 10, 11] {
                let old = codec::sequence(19, 17, pattern, pattern % 7, flags, 0);
                let mut input = old.input[..8].to_vec();
                input[2] |= 8 | 16;
                input.extend([min, max]);
                input.extend_from_slice(&old.input[8..]);
                cases.push(Case::new("AnimationCache", input));
            }
        }
    }
    // Equal-size full-canvas alternatives and later-tie demotion; many cycles
    // exercise flushing/rebasing. Repeats change settings without advancing k.
    for (w, h) in [(1, 1), (8, 8), (19, 17)] {
        for primary in [0, 1, 2] {
            for (min, max) in [(0, 1), (0, 3), (2, 5), (4, 9)] {
                let mut input = vec![
                    w,
                    h,
                    4 | 8 | 16 | if primary == 2 { 2 } else { 0 },
                    0,
                    24,
                    1,
                    0xfffffff0,
                    0x1234,
                    min,
                    max,
                ];
                for i in 0..24 {
                    input.extend([
                        i % 3,
                        w + 1,
                        if primary < 2 { primary } else { i % 2 },
                        (if i % 2 == 0 { 61.5f32 } else { 75.25 }).to_bits(),
                        i % 7,
                        if i % 3 == 0 { 60 } else { 100 },
                        i % 2,
                        100,
                        i % 2,
                    ]);
                    // Includes exact repeats, global replacements, sparse
                    // changes, fades and a late return to a simple canvas.
                    let t = i - u32::from(i % 5 == 2);
                    for y in 0..h {
                        for x in 0..w {
                            let word = match t % 8 {
                                0 => 0xff102030,
                                1 | 2 => 0xff405060,
                                3 => {
                                    if x < 3 && y < 3 {
                                        0xffff0022
                                    } else {
                                        0xff405060
                                    }
                                }
                                4 => ((x * 7919 + y * 104729) * (t + 1) & 0xffffff) | 0xff000000,
                                5 => 0x8000ff40,
                                6 => {
                                    if x < w / 2 {
                                        0
                                    } else {
                                        0xff402080
                                    }
                                }
                                _ => 0xff102030,
                            };
                            input.push(word);
                        }
                        if y + 1 < h {
                            input.push(0xdeadbeef);
                        }
                    }
                }
                cases.push(Case::new("AnimationCache", input));
            }
        }
    }
    cases
}

pub fn selection_fixtures() -> Vec<Case> {
    let cases = selection_cases();
    [
        0, 4, 5, 10, 15, 24, 30, 45, 47, 60, 62, 63, 64, 69, 75, 89, 90, 93, 101, 105, 109, 113,
        117, 121, 125,
    ]
    .into_iter()
    .map(|i| cases[i].clone())
    .collect()
}

// Prefixes end with no selection, an undecided selection, or an earlier
// retained selection. These reuse the named 4c pixels/configs, changing timing.
pub fn overflow_cases() -> Vec<Case> {
    let sources = selection_cases();
    let mut result = Vec::new();
    for (source, prefix) in [
        (0, 1),
        (15, 1),
        (15, 2),
        (15, 3),
        (47, 4),
        (47, 5),
        (63, 6),
        (63, 7),
        (69, 7),
        (93, 9),
        (105, 12),
        (109, 12),
        (117, 12),
        (121, 12),
        (125, 12),
    ] {
        for timing in 0..5 {
            for reverse in [false, true] {
                result.push(overflow_case(&sources[source], prefix, timing, reverse));
            }
        }
    }
    result
}

fn overflow_case(source: &Case, prefix: usize, timing: u32, reverse: bool) -> Case {
    let base = base(source);
    let frames = codec::frames(&base);
    let target = &frames[prefix - 1];
    let mut input = source.input[..10].to_vec();
    input[2] |= 32;
    input[4] = 0;
    let mut append = |frame: &codec::Frame<'_>, config: MixedEncodingConfig, duration, change| {
        input[4] += 1;
        input.extend([duration, frame.stride as u32]);
        input.extend(animation_mixed::config_words(config));
        input.extend(
            frame
                .words
                .iter()
                .map(|&word| if change { word ^ 0x00770011 } else { word }),
        );
    };
    for (i, frame) in frames[..prefix].iter().enumerate() {
        append(
            frame,
            frame.config,
            if i + 1 != prefix {
                0
            } else if timing == 0 {
                0xfffffe
            } else {
                0xffffff
            },
            false,
        );
    }
    let config = if reverse {
        target.config.reversed()
    } else {
        target.config
    };
    append(target, config, if timing == 2 { 2 } else { 1 }, false);
    if timing == 3 {
        // The changed input splits timing before its settings can replace the
        // saved previous settings used to choose the hold codec.
        append(target, config.reversed(), 1, true);
    } else if timing == 4 {
        for (i, duration) in [0xffffff, 0xffffff, 1].into_iter().enumerate() {
            append(
                target,
                if i % 2 == 0 {
                    config.reversed()
                } else {
                    config
                },
                duration,
                false,
            );
        }
    }
    Case::new("AnimationCache", input)
}

pub fn overflow_fixtures() -> Vec<Case> {
    let cases = overflow_cases();
    // Both hold codecs, reversed saved settings, undecided/earlier selections,
    // threshold neighbors, capacity, before-change and repeated/final splits.
    [
        0, 2, 4, 6, 8, 21, 22, 23, 26, 28, 52, 62, 66, 68, 73, 82, 92, 96, 102, 106, 108, 122, 128,
        132, 142, 146,
    ]
    .into_iter()
    .map(|i| cases[i].clone())
    .collect()
}
