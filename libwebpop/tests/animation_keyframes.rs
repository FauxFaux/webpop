#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::internal::mux::anim_encode::*;
use support::animation_keyframes as corpus;

struct Trace<'a> {
    snapshots: Vec<&'a [u32]>,
    trials: Vec<Vec<&'a [u32]>>,
    file: Vec<u8>,
}

fn snapshot<'a>(words: &'a [u32], at: &mut usize) -> &'a [u32] {
    let count = words[*at + 2] as usize;
    let len = 19 + count * 17;
    let result = &words[*at..*at + len];
    *at += len;
    assert!(count <= result[12] as usize);
    assert_eq!(result[13], 0); // Original FlushFrames rebases retained slots.
    assert!(result[14] <= result[2]);
    assert!(result[17] == u32::MAX || result[17] < result[2]);
    for slot in result[19..].chunks_exact(17) {
        assert!(slot[0] <= 1);
        let selected = if slot[0] != 0 {
            &slot[9..17]
        } else {
            &slot[1..9]
        };
        assert!(selected[7] > 0);
        for alt in [&slot[1..9], &slot[9..17]] {
            assert!(alt[0] < 1 << 24);
            assert!(alt[3] <= 1 && alt[4] <= 1);
            if alt[7] == 0 {
                assert_eq!(&alt[5..7], &[0, 0]);
            } else {
                assert!(alt[5] > 0 && alt[6] > 0);
            }
        }
    }
    result
}

fn parse<'a>(case: &support::Case, words: &'a [u32]) -> Trace<'a> {
    let mut at = 0;
    let mut snapshots = Vec::new();
    let mut trials = Vec::new();
    for frame in 0..case.input[4] {
        let before = snapshot(words, &mut at);
        assert_eq!(before[0], frame);
        snapshots.push(before);
        let count = words[at] as usize;
        assert!(count <= 8);
        at += 1;
        let mut calls = Vec::new();
        for _ in 0..count {
            let len = 12 + words[at + 11] as usize;
            let call = &words[at..at + len];
            at += len;
            let bytes: Vec<_> = call[12..].iter().map(|&v| v as u8).collect();
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
                bytes.len()
            );
            let decoded =
                image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (call[1], call[2]));
            calls.push(call);
        }
        trials.push(calls);
        let after = snapshot(words, &mut at);
        assert_eq!(after[0], frame + 1);
        snapshots.push(after);
    }
    snapshots.push(snapshot(words, &mut at));
    snapshots.push(snapshot(words, &mut at));
    let len = words[at] as usize;
    at += 1;
    let file = words[at..at + len].iter().map(|&v| v as u8).collect();
    assert_eq!(at + len, words.len());
    Trace {
        snapshots,
        trials,
        file,
    }
}

fn check_trace(case: &support::Case, words: &[u32]) {
    let trace = parse(case, words);
    let expected = SanitizeEncoderOptions(
        case.input[8] as i32,
        case.input[9] as i32,
        case.input[2] & 1 != 0,
    );
    for state in &trace.snapshots {
        assert_eq!(
            &state[10..13],
            &[
                expected.kmin as u32,
                expected.kmax as u32,
                expected.capacity as u32
            ]
        );
    }
    assert_eq!(&trace.snapshots[0][15..19], &[0, 1, u32::MAX, 0]);
    assert_eq!(trace.trials.len(), case.input[4] as usize);
    if corpus::is_disabled(case) {
        assert_eq!(case.rust(), words);
        let (_, base_file) = support::animation_codec_cache::encode(&corpus::base(case));
        assert_eq!(trace.file, base_file);
    } else {
        // These record original C selection for future packages. They do not
        // claim that the Rust disabled cache implements deferred selection.
        assert!(trace.snapshots.iter().any(|s| {
            s[19..]
                .chunks_exact(17)
                .any(|slot| slot[8] != 0 && slot[16] != 0)
        }));
    }
}

#[test]
fn animation_keyframe_options_committed_signed_edges_and_window_capacity() {
    let fixtures =
        fixture_support::fixtures(include_str!("fixtures/animation_keyframe_options.txt"));
    let cases = corpus::option_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(&cases) {
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        let capacity = expected[2];
        assert!((2..=31).contains(&capacity));
    }
    let all = SanitizeEncoderOptions(i32::MIN, 1, false);
    assert_eq!((all.kmin, all.kmax, all.capacity), (0, 0, 2));
    let negative = SanitizeEncoderOptions(i32::MIN, 2, false);
    assert_eq!(
        (negative.kmin, negative.kmax, negative.capacity),
        (-28, 2, 31)
    );
    assert_eq!(SanitizeEncoderOptions(0, 100, false).capacity, 31);
    assert_eq!(SanitizeEncoderOptions(0, 60, false).capacity, 30);
    assert_eq!(
        SanitizeEncoderOptions(0, 1, true),
        SanitizeEncoderOptions(0, 0, false)
    );
}

#[test]
fn animation_keyframe_cache_observer_committed_alternatives_and_calls() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_keyframe_cache.txt"));
    let cases = corpus::traces();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(&cases) {
        assert_eq!(stored.input, case.input);
        check_trace(case, expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_keyframe_foundation_matches_pinned_reference() {
    let options = corpus::option_cases();
    for (case, expected) in options
        .iter()
        .zip(native::run("animation-keyframe-options", &options))
    {
        native::compare(case, &expected, &case.rust());
    }
    let traces = corpus::traces();
    for (case, expected) in traces
        .iter()
        .zip(native::run("animation-keyframe-cache", &traces))
    {
        check_trace(case, &expected);
    }
    eprintln!(
        "{} signed option mappings and {} cache traces match pinned C (enabled selection traces reference-only)",
        options.len(),
        traces.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_keyframe_foundation_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    for (name, cases) in [
        ("options", corpus::option_cases()),
        ("cache", corpus::traces()),
    ] {
        let outputs = native::run(&format!("animation-keyframe-{name}-fixtures"), &cases);
        for (case, output) in cases.iter().zip(&outputs) {
            if name == "options" {
                native::compare(case, output, &case.rust());
            } else {
                check_trace(case, output);
            }
        }
        if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1"))
        {
            let mut text = format!(
                "# Pinned scalar C keyframe {name} foundation.\n# upstream={} seed={:08x}\n# Enabled cache traces are reference-only; see ../reference/ANIMATION.md.\n",
                support::UPSTREAM_REVISION,
                support::SEED
            );
            for (case, output) in cases.iter().zip(outputs) {
                text.push_str(&case.function);
                for word in &case.input {
                    write!(text, " {word:08x}").unwrap();
                }
                text.push_str(" |");
                for word in output {
                    write!(text, " {word:08x}").unwrap();
                }
                text.push('\n');
            }
            std::fs::write(
                format!(
                    "{}/tests/fixtures/animation_keyframe_{name}.txt",
                    env!("CARGO_MANIFEST_DIR")
                ),
                text,
            )
            .unwrap();
        }
    }
}
