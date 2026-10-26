use super::*;
use crate::mux::{FrameBlend, FrameDispose};

fn config_words(config: MixedEncodingConfig) -> [u32; 7] {
    [
        u32::from(config.primary == AnimationCandidateCodec::Vp8l),
        config.codec.quality.to_bits(),
        config.codec.method,
        config.near_lossless as u32,
        u32::from(config.exact),
        config.codec.alpha_quality as u32,
        u32::from(config.codec.use_sharp_yuv),
    ]
}

// Read-only V3 snapshot of the actual private owner, including both payload
// alternatives and original-word history. Selection is never reproduced here.
fn snapshot(cache: &AnimationCache) -> Vec<u32> {
    let s = cache.status();
    let w = cache.window_status();
    let r = s.previous_rect;
    let mut out = vec![
        s.submitted_frames as u32,
        s.emitted_frames as u32,
        s.cached_frames as u32,
        s.count_since_key_frame as u32,
        r.x_offset as u32,
        r.y_offset as u32,
        r.width as u32,
        r.height as u32,
        u32::from(s.pending_duration_ms.is_some()),
        s.pending_duration_ms.unwrap_or(0),
        w.effective.kmin as u32,
        w.effective.kmax as u32,
        w.effective.capacity as u32,
        0,
        w.flush_count as u32,
        w.best_penalty as u32,
        ((w.best_penalty as u64) >> 32) as u32,
        w.selected_keyframe.map_or(u32::MAX, |i| i as u32),
        u32::from(w.previous_candidate_undecided),
    ];
    for frame in cache.cached_candidates() {
        out.push(u32::from(frame.is_key_frame));
        for alt in [frame.sub_frame, frame.key_frame] {
            let c = alt.control;
            out.extend([
                c.duration_ms,
                c.x_offset,
                c.y_offset,
                u32::from(c.dispose == FrameDispose::Background),
                u32::from(c.blend == FrameBlend::Replace),
                alt.width,
                alt.height,
                alt.encoded_size as u32,
            ]);
        }
    }
    let (config, reversed) = cache.saved_configs().unwrap();
    out.extend(config_words(config));
    out.extend(config_words(reversed));
    let c = cache.pending_control().unwrap();
    out.extend([
        c.x_offset,
        c.y_offset,
        u32::from(c.dispose == FrameDispose::Background),
        u32::from(c.blend == FrameBlend::Replace),
    ]);
    let (previous, carryover) = cache.canvases();
    out.push(previous.len() as u32);
    out.extend(previous);
    out.extend(carryover);
    out
}

fn take_snapshot<'a>(input: &[u32], words: &'a [u32], at: &mut usize) -> &'a [u32] {
    let count = words[*at + 2] as usize;
    let history = if words[*at] != 0 {
        19 + if count != 0 {
            2 * (input[0] * input[1]) as usize
        } else {
            0
        }
    } else {
        0
    };
    let end = *at + 19 + count * 17 + history;
    let result = &words[*at..end];
    *at = end;
    result
}

#[test]
fn animation_keyframe_overflow_private_duration_and_flush_match_c_snapshots() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/fixtures/animation_keyframe_overflow.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let (request, response) = line.split_once('|').unwrap();
        let input: Vec<_> = request
            .split_whitespace()
            .skip(1)
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        let words: Vec<_> = response
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16).unwrap())
            .collect();
        assert_eq!(input[2] & 60, 60);
        let policy = match input[9] {
            0 => KeyframePolicy::Disabled,
            1 => KeyframePolicy::AllFrames,
            max_interval => KeyframePolicy::Bounded {
                min_interval: input[8],
                max_interval,
            },
        };
        let mut cache = AnimationCache::new_with_keyframes(
            input[0],
            input[1],
            AnimationCacheOptions {
                minimize_size: input[2] & 1 != 0,
                allow_mixed: input[2] & 2 != 0,
                ..Default::default()
            },
            policy,
        )
        .unwrap();
        let mut timestamp = u64::from(input[6]) | (u64::from(input[7]) << 32);
        let mut cursor = 10;
        let mut at = 0;
        let mut duration = 0;
        for _ in 0..input[4] {
            duration = input[cursor];
            let stride = input[cursor + 1] as usize;
            let config = MixedEncodingConfig {
                primary: if input[cursor + 2] == 0 {
                    AnimationCandidateCodec::Vp8
                } else {
                    AnimationCandidateCodec::Vp8l
                },
                codec: LossyOptions {
                    quality: f32::from_bits(input[cursor + 3]),
                    method: input[cursor + 4],
                    alpha_quality: input[cursor + 7] as i32,
                    use_sharp_yuv: input[cursor + 8] != 0,
                },
                near_lossless: input[cursor + 5] as i32,
                exact: input[cursor + 6] != 0,
            };
            cursor += 9;
            let size = (input[1] as usize - 1) * stride + input[0] as usize;
            cache
                .add_argb(
                    ArgbView::new(
                        &input[cursor..cursor + size],
                        input[0] as i32,
                        input[1] as i32,
                        stride,
                    )
                    .unwrap(),
                    timestamp,
                    &config,
                )
                .unwrap();
            cursor += size;
            timestamp += u64::from(duration);
            take_snapshot(&input, &words, &mut at);
            let trials = words[at];
            at += 1;
            for _ in 0..trials {
                at += 12 + words[at + 11] as usize;
            }
            assert_eq!(snapshot(&cache), take_snapshot(&input, &words, &mut at));
        }
        assert_eq!(cursor, input.len());
        assert_eq!(snapshot(&cache), take_snapshot(&input, &words, &mut at));
        cache.IncreasePreviousDuration(duration).unwrap();
        assert_eq!(snapshot(&cache), take_snapshot(&input, &words, &mut at));
        cache.FlushFrames(cache.window.state.flush_count).unwrap();
        assert_eq!(snapshot(&cache), take_snapshot(&input, &words, &mut at));
        checked += 1;
    }
    assert_eq!(checked, 26);
}

#[test]
fn animation_rgba_cache_post_validation_candidate_failure_is_terminal() {
    let mut cache = AnimationCache::new(1, 1, Default::default()).unwrap();
    let config = MixedEncodingConfig::default();
    cache.add_rgba(&[12, 34, 56, 255], 4, 0, &config).unwrap();
    // Inject an internal candidate failure after valid public layout/config/
    // timing checks, without exhausting the allocator or changing caller data.
    cache.status.previous_rect.width = 2;
    assert!(matches!(
        cache.add_rgba(&[56, 34, 12, 255], 4, 1, &config),
        Err(AnimationError::Candidate { frame_index: 1, .. })
    ));
    assert!(matches!(
        cache.add_rgba(&[56, 34, 12, 255], 4, 1, &config),
        Err(AnimationError::InvalidState)
    ));
    let mut output = vec![11, 22, 33];
    assert!(matches!(
        cache.finish(1, &mut output),
        Err(AnimationError::InvalidState)
    ));
    assert_eq!(output, [11, 22, 33]);
}
