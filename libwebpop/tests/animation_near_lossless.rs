#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use libwebpop::*;
use support::{
    animation_cache as cache, animation_lossless as lossless, animation_near_lossless as corpus,
};

fn still(frame: EncodedFrame) -> Vec<u8> {
    let mut mux =
        AnimationContainer::new(frame.width(), frame.height(), Default::default()).unwrap();
    mux.push_frame(frame, Default::default()).unwrap();
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    bytes
}

fn rgba(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
    words
        .into_iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8])
        .collect()
}

// Reconstruct only the selected codec input from the original rectangle and
// winning transparency mask, then compare with the independently validated
// prepared near-lossless API. Preprocessing a whole canvas first is different.
fn check_candidate(case: &support::Case, words: &[u32]) -> (usize, usize) {
    let Some(candidate) = corpus::candidate(case) else {
        assert_eq!(words, [0]);
        return (0, 0);
    };
    let base = corpus::base(case);
    let a = &base.input;
    let (_, target) = lossless::canvases(&base);
    let view = ArgbView::new(target, a[1] as i32, a[2] as i32, a[4] as usize)
        .unwrap()
        .sub_view(candidate.rect)
        .unwrap();
    let mask = &candidate.carryover_mask;
    let original: Vec<_> = (0..view.height() as usize)
        .flat_map(|y| view.row(y).unwrap().iter().copied())
        .collect();
    let pixels = rgba(original.iter().enumerate().map(|(i, &p)| {
        if !mask.is_empty() && mask[i] != 0 {
            0
        } else {
            p
        }
    }));
    let expected = still(
        EncodedFrame::encode_near_lossless_rgba(
            &pixels,
            view.width() as u32,
            view.height() as u32,
            view.width() as usize * 4,
            case.input[0] as i32,
            &lossless::options(&base).codec,
        )
        .unwrap(),
    );
    let len = words[8] as usize;
    let actual: Vec<_> = words[10 + len..].iter().map(|&v| v as u8).collect();
    assert_eq!(actual.len(), words[9 + len] as usize);
    assert_eq!(actual, expected);
    let decoded = image::load_from_memory_with_format(&actual, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    let mut changes = (0, 0);
    for (i, p) in decoded.as_raw().chunks_exact(4).enumerate() {
        let before = &pixels[i * 4..][..4];
        if before[3] == 0 || before[3] == 255 {
            assert_eq!(p[3], before[3]);
        }
        if before[3] != 0 && p[3] != 0 {
            changes.0 += usize::from(p[..3] != before[..3]);
        }
        changes.1 += usize::from(p[3] != before[3]);
    }
    if case.input[0] == 100 {
        assert_eq!(changes, (0, 0));
    }
    changes
}

fn check_cache(case: &support::Case, words: &[u32]) -> (support::Case, Vec<u32>) {
    let bytes = cache::output_bytes(case, words);
    // The near-lossless cache fixtures deliberately retain multiple ANMFs.
    let decoded = lossless::composite_lossless(&bytes);
    let frames = cache::frames(case);
    assert_eq!(decoded[2], words[case.input[4] as usize * 10 + 1]);
    let last_time =
        4 + (decoded[2] as usize - 1) * (1 + (case.input[0] * case.input[1] * 4) as usize);
    assert_eq!(
        decoded[last_time],
        frames.iter().map(|f| f.duration).sum::<u32>()
    );
    if frames.iter().all(|f| f.strength == 100) {
        let mut input = case.input[..8].to_vec();
        for f in frames {
            input.extend([
                f.duration,
                f.stride as u32,
                f.codec.quality,
                f.codec.method,
                u32::from(f.codec.exact),
            ]);
            input.extend_from_slice(f.words);
        }
        let reference = support::Case::new("AnimationCache", input);
        assert_eq!(case.rust(), reference.rust());
    }
    let mut request = vec![bytes.len() as u32];
    request.extend(bytes.into_iter().map(u32::from));
    (support::Case::new("AnimationDecode", request), decoded)
}

#[test]
fn animation_near_lossless_committed_candidates_and_caches() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_near_lossless.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut changes = (0, 0);
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        if case.function == "AnimationNearLossless" {
            let changed = check_candidate(&case, expected);
            changes.0 += changed.0;
            changes.1 += changed.1;
        } else {
            let _ = check_cache(&case, expected);
        }
    }
    assert!(changes.0 > 0 && changes.1 > 0);
}

#[test]
fn animation_near_lossless_strength_validation_precedes_cache_mutation() {
    let words = [0xff123456];
    let view = ArgbView::new(&words, 1, 1, 1).unwrap();
    let rect = FrameRectangle {
        width: 1,
        height: 1,
        ..Default::default()
    };
    let codec = LosslessOptions::default();
    let mut cache = LosslessAnimationCache::new(1, 1, Default::default()).unwrap();
    cache.add_argb(view, 100, &codec).unwrap();
    let status = cache.status();
    for strength in [i32::MIN, -1, 101, i32::MAX] {
        assert!(matches!(
            encode_near_lossless_candidate(view, rect, &codec, strength, false),
            Err(AnimationCandidateError::Encoding(
                WebPEncodingError::InvalidNearLosslessStrength
            ))
        ));
        assert!(matches!(
            set_frame_near_lossless(view, view, rect, &Default::default(), strength),
            Err(AnimationCandidateError::Encoding(
                WebPEncodingError::InvalidNearLosslessStrength
            ))
        ));
        assert!(matches!(
            cache.add_near_lossless_argb(view, 200, strength, &codec),
            Err(AnimationError::Encoding {
                frame_index: 1,
                source: WebPEncodingError::InvalidNearLosslessStrength
            })
        ));
        assert_eq!(cache.status(), status);
    }
    cache.add_near_lossless_argb(view, 200, 0, &codec).unwrap();
    assert_eq!(cache.status().cached_frames, 1);
    let mut output = Vec::new();
    cache.finish(300, &mut output).unwrap();
    assert_eq!(words, [0xff123456]);
}

#[test]
fn animation_near_lossless_routes_palette_predictor_and_standalone_after_crop() {
    let mut changes = (0, 0);
    for (w, h, pattern, exact) in [
        (19, 17, 103, false),
        (19, 17, 101, true),
        (65, 5, 100, true),
    ] {
        let mut case = corpus::candidate_case(w, h, pattern, 4, 0, 3);
        case.input[1] = 2; // Direct codec candidate, bypassing rectangle refinement.
        case.input[13] = u32::from(exact);
        let base = corpus::base(&case);
        let (_, target) = lossless::canvases(&base);
        let analysis = libwebpop::enc::vp8l_enc::EncoderAnalyze(
            target,
            w as usize,
            h as usize,
            base.input[4] as usize,
            4,
            70,
        )
        .unwrap();
        use libwebpop::enc::vp8l_enc::EntropyIx::*;
        assert!(analysis.crunch_configs.iter().all(|config| match pattern {
            103 => matches!(config.entropy_idx, kPalette | kPaletteAndSpatial),
            101 => matches!(config.entropy_idx, kSpatial | kSpatialSubGreen),
            _ => matches!(config.entropy_idx, kDirect | kSubGreen),
        }));
        let near = case.rust();
        let changed = check_candidate(&case, &near);
        changes.0 += changed.0;
        changes.1 += changed.1;
        let mut full = case.clone();
        full.input[0] = 100;
        if pattern == 103 || pattern == 101 {
            assert_eq!(near, full.rust());
        } else {
            assert_ne!(near, full.rust());
            assert!(changed.0 > 0 && changed.1 > 0);
        }
    }
    assert!(changes.0 > 0 && changes.1 > 0);
    // A 65x5 noisy canvas has active standalone preprocessing, but its 63x5
    // rectangle falls below the shortcut threshold. Codec routing sees the crop.
    let mut cropped = corpus::candidate_case(65, 5, 100, 4, 0, 3);
    cropped.input[1] = 2;
    cropped.input[6..10].copy_from_slice(&[2, 0, 63, 5]);
    cropped.input[13] = 1;
    let near = cropped.rust();
    let _ = check_candidate(&cropped, &near);
    cropped.input[0] = 100;
    assert_eq!(near, cropped.rust());
    // A noisy full rectangle becomes a three-color palette only after animation
    // replaces identical opaque pixels with transparent holds.
    let palette = corpus::transparency_palette_case(0);
    let near = palette.rust();
    assert_eq!(near[5], 0);
    assert_eq!(near[7], 1);
    assert_eq!(
        near[9..9 + near[8] as usize]
            .iter()
            .filter(|&&v| v != 0)
            .count(),
        65 * 5 - 2
    );
    let _ = check_candidate(&palette, &near);
    assert_eq!(near, corpus::transparency_palette_case(100).rust());
    let base = corpus::base(&palette);
    let (_, target) = lossless::canvases(&base);
    let view = ArgbView::new(target, 65, 5, base.input[4] as usize).unwrap();
    let mut pixels: Vec<_> = (0..5)
        .flat_map(|y| view.row(y).unwrap().iter().copied())
        .collect();
    assert!(
        pixels
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 256
    );
    for (index, pixel) in pixels.iter_mut().enumerate() {
        if near[9 + index] != 0 {
            *pixel = 0;
        }
    }
    let analysis = libwebpop::enc::vp8l_enc::EncoderAnalyze(&pixels, 65, 5, 65, 4, 70).unwrap();
    assert_eq!(analysis.palette_size, 3);
    assert!(analysis.crunch_configs.iter().all(|config| matches!(
        config.entropy_idx,
        libwebpop::enc::vp8l_enc::EntropyIx::kPalette
            | libwebpop::enc::vp8l_enc::EntropyIx::kPaletteAndSpatial
    )));
}

#[test]
fn animation_near_lossless_repeats_compare_originals_despite_decoded_changes() {
    let case = corpus::cache_case(65, 5, 100, 4, 0, 0, true);
    let output = case.rust();
    assert_eq!(&output[10..13], &[2, 0, 1]);
    let (_, decoded) = check_cache(&case, &output);
    let f = &cache::frames(&case)[0];
    let view = ArgbView::new(f.words, 65, 5, f.stride).unwrap();
    let original = rgba((0..5).flat_map(|y| view.row(y).unwrap().iter().copied()));
    let first = &decoded[5..5 + 65 * 5 * 4];
    assert!(
        first
            .chunks_exact(4)
            .zip(original.chunks_exact(4))
            .any(|(a, b)| a[3] != u32::from(b[3]))
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_near_lossless_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-near-lossless", &cases, |case, expected| {
        let mut candidates = 0;
        let mut caches = 0;
        native::compare(case, expected, &case.rust());
        if case.function == "AnimationNearLossless" {
            candidates += 1;
        } else {
            caches += 1;
        }
        [candidates, caches]
    });
    let candidates: usize = stats.iter().map(|counts| counts[0]).sum();
    let caches: usize = stats.iter().map(|counts| counts[1]).sum();

    eprintln!(
        "{} near-lossless animation cases match scalar C; candidates={candidates}, caches={caches}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_near_lossless_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-near-lossless-fixtures", &cases);
    let mut requests = Vec::new();
    let mut observations = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        if case.function == "AnimationNearLossless" {
            let _ = check_candidate(case, expected);
        } else {
            let (request, observation) = check_cache(case, expected);
            requests.push(request);
            observations.push(observation);
        }
    }
    let decoded = native::run("animation-near-lossless-decodes", &requests);
    for ((request, expected), actual) in requests.iter().zip(&observations).zip(&decoded) {
        native::compare(request, expected, actual);
    }
    eprintln!(
        "{} all-frame near-lossless composites match pinned decoder",
        requests.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C near-lossless animation fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
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
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/animation_near_lossless.txt"
            ),
            text,
        )
        .unwrap();
    }
}
