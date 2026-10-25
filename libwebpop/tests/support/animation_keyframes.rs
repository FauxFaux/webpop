//! 4b normalization and version-2 cache observations. Enabled traces remain
//! reference-only until deferred selection/overflow integration are ported.
use super::{Case, animation_cache, animation_codec_cache as codec, animation_mixed};
use libwebpop::mux::anim_encode::*;
use libwebpop::*;

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
    input[2] &= !8;
    input.extend_from_slice(&case.input[10..]);
    Case::new("AnimationCache", input)
}

pub fn is_disabled(case: &Case) -> bool {
    let a = &case.input;
    SanitizeEncoderOptions(a[8] as i32, a[9] as i32, a[2] & 1 != 0)
        == SanitizeEncoderOptions(0, 0, false)
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    assert!(
        is_disabled(case),
        "enabled selection is reference-only in 4b"
    );
    let a = &case.input;
    let base = base(case);
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
    let mut result = Vec::new();
    let frames = codec::frames(&base);
    for (i, f) in frames.iter().enumerate() {
        result.extend(snapshot(&cache));
        // Reuse the existing isolated-trial evaluator to compare every original
        // WebPEncode call, including losers. Timing overflow may reset prev_rect.
        let mut rect = cache.status().previous_rect;
        if cache.status().pending_duration_ms.is_some_and(|d| {
            let previous_duration = frames[i - 1].duration;
            d + previous_duration >= 1 << 24
        }) {
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
            ..Default::default()
        };
        // V2 disabled fixture traces use mixed mode; VP8/VP8L-only parity stays
        // covered by V1 and the existing exhaustive candidate/cache suites.
        assert!(a[2] & 2 != 0);
        let trials = animation_mixed::trials(
            ArgbView::new(&restored, a[0] as i32, a[1] as i32, a[0] as usize).unwrap(),
            current,
            rect,
            &settings,
        );
        result.push(trials.len() as u32);
        for trial in trials {
            result.extend(trial.record);
        }
        cache.add_argb(current, timestamp, &f.config).unwrap();
        result.extend(snapshot(&cache));
        timestamp += u64::from(f.duration);
    }
    result.extend(snapshot(&cache));
    let mut output = Vec::new();
    cache.finish(timestamp, &mut output).unwrap();
    result.extend(snapshot(&cache));
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
