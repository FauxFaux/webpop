#[path = "support/native.rs"]
mod native;
mod support;

use std::fmt::Write;
use support::entropy_coding::{entropy_coding_cases, entropy_coding_fixture_cases};
use support::references::{foundation_cases, foundation_fixture_cases};

#[test]
fn public_rgba_encoders_match_pinned_reference() {
    let cases = support::public_images::cases();
    native::parallel_cases("public-rgba", &cases, |case, expected| {
        let encoded = support::public_images::encode(case);
        native::compare(
            case,
            expected,
            &encoded.iter().map(|&v| u32::from(v)).collect::<Vec<_>>(),
        );
        support::public_images::check_decoded(case, &encoded);
    });
    eprintln!(
        "Compared and independently decoded {} public RGBA files",
        cases.len()
    );
}

#[test]
fn public_rgba_fixtures_match_pinned_reference() {
    let mut cases = support::public_images::fixture_cases();
    cases.push(support::public_images::real_image().request());
    let outputs = native::run("public-rgba-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C complete public RGBA encoding fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases[..cases.len() - 1].iter().zip(&outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/public_rgba.txt"
            ),
            text,
        )
        .unwrap();
        let encoded: Vec<_> = outputs
            .last()
            .unwrap()
            .iter()
            .map(|&v| u8::try_from(v).unwrap())
            .collect();
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/corpus/test_ref.webp"
            ),
            encoded,
        )
        .unwrap();
    } else {
        assert_eq!(
            include_str!("fixtures/public_rgba.txt")
                .lines()
                .filter(|s| !s.starts_with('#'))
                .count(),
            cases.len() - 1
        );
        for (line, expected) in include_str!("fixtures/public_rgba.txt")
            .lines()
            .filter(|s| !s.starts_with('#'))
            .zip(&outputs)
        {
            let (_, response) = line.split_once(" | ").unwrap();
            let stored: Vec<_> = response
                .split_whitespace()
                .skip(1)
                .map(|s| u32::from_str_radix(s, 16).unwrap())
                .collect();
            assert_eq!(&stored, expected);
        }
        let stored: Vec<_> = include_bytes!("fixtures/corpus/test_ref.webp")
            .iter()
            .map(|&v| u32::from(v))
            .collect();
        assert_eq!(&stored, outputs.last().unwrap());
    }
}

#[test]
fn encoder_streams_match_pinned_reference() {
    let cases = support::streams::cases();
    let mut features = [false; 4];
    let mut cache = false;
    for stats in native::parallel_cases("encoder-streams", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
        assert_eq!(expected[6], expected[7] + expected[8]);
        [expected[0], expected[4]]
    }) {
        for (i, reached) in features.iter_mut().enumerate() {
            *reached |= stats[0] & (1 << i) != 0;
        }
        cache |= stats[1] != 0;
    }
    assert_eq!(features, [true; 4], "all four selected transforms");
    assert!(cache, "selected color caches");
    eprintln!(
        "Compared {} complete encoder streams and selected statistics",
        cases.len()
    );
}

#[test]
fn encoder_stream_fixtures_match_pinned_reference() {
    let cases = support::streams::fixture_cases();
    let outputs = native::run("encoder-stream-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C complete encoder streams and selected statistics.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/streams.txt"),
            text,
        )
        .unwrap();
    }
}

#[test]
fn analysis_matches_pinned_reference() {
    let mut cases = support::analysis::cases();
    cases.extend(support::analysis::sizing_cases());
    let mut modes = [false; 5];
    let mut flags = [[false; 2]; 2];
    let mut trials = [false; 6];
    let mut sorts = [false; 6];
    let mut exhaustive = [false; 2];
    for (case, expected) in cases.iter().zip(native::run("analysis", &cases)) {
        native::compare(case, &expected, &case.rust());
        if case.function == "AnalyzeEntropy" {
            modes[expected[0] as usize] = true;
            flags[0][expected[1] as usize] = true;
            flags[1][expected[2] as usize] = true;
        } else if case.function == "EncoderAnalyze" {
            let mut pos = 1 + expected[0] as usize + 4;
            let count = expected[pos] as usize;
            if case.input[3] == 6 && case.input[4] == 100 {
                exhaustive[usize::from(count == 10)] |= count == 8 || count == 10;
            }
            pos += 1;
            for _ in 0..count {
                trials[expected[pos] as usize] = true;
                sorts[expected[pos + 1] as usize] = true;
                pos += 3 + 2 * expected[pos + 2] as usize;
            }
            assert_eq!(pos, expected.len());
        }
    }
    assert_eq!(modes, [true; 5], "every guessed entropy mode");
    assert_eq!(
        flags, [[true; 2]; 2],
        "both values of original/transformed zero flags"
    );
    assert_eq!(trials, [true; 6], "every transform trial");
    assert_eq!(sorts, [true; 6], "all palette methods and unused marker");
    assert_eq!(
        exhaustive, [true; 2],
        "exhaustive palette trials with/without subtract green"
    );
    eprintln!("Compared {} analysis/entropy/sizing cases", cases.len());
}

#[test]
fn analysis_fixtures_match_pinned_reference() {
    let cases = support::analysis::fixture_cases();
    let outputs = native::run("analysis-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C entropy/encoder analysis and sizing fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/analysis.txt"),
            text,
        )
        .unwrap();
    }
}

#[test]
fn main_image_streams_match_pinned_reference() {
    let cases = support::main_images::cases();
    for (case, expected) in cases.iter().zip(native::run("main-images", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} main-image streams/cache choices/sizes",
        cases.len()
    );
}

#[test]
fn main_image_fixtures_match_pinned_reference() {
    let cases = support::main_images::fixture_cases();
    let outputs = native::run("main-image-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C main-image stream fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/main_images.txt"
            ),
            text,
        )
        .unwrap();
    }
}

#[test]
fn clustering_and_sampling_match_pinned_reference() {
    fn skip_set(words: &[u32], pos: &mut usize) -> (u32, u32) {
        let size = words[*pos];
        *pos += 1;
        let mut used = 0;
        for _ in 0..size {
            let present = words[*pos];
            *pos += 1;
            if present == 0 {
                continue;
            }
            used += 1;
            for _ in 0..5 {
                let nonzero = words[*pos] as usize;
                *pos += 1 + 2 * nonzero;
            }
            *pos += 23; // trivial symbols, cached costs, used flags and bin id
        }
        (size, used)
    }
    let cases = support::clustering::cases();
    let mut reached = [false; 7];
    for flags in native::parallel_cases("clustering", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
        let mut reached = [false; 7];
        if case.function != "VP8LClustering" {
            return reached;
        }
        let mut pos = 0;
        let (raw, used) = skip_set(expected, &mut pos);
        reached[0] |= used < raw;
        let (initial, _) = skip_set(expected, &mut pos);
        let combine = expected[pos] != 0;
        pos += 1;
        skip_set(expected, &mut pos);
        let (binned, _) = skip_set(expected, &mut pos);
        reached[1] |= combine && binned < initial && case.input[3] == 0;
        reached[2] |= combine && binned < initial && case.input[3] != 0;
        let greedy = expected[pos] != 0;
        pos += 1;
        let (stochastic, _) = skip_set(expected, &mut pos);
        reached[3] |= stochastic < binned;
        reached[4] |= !greedy && stochastic > 1;
        let (final_size, _) = skip_set(expected, &mut pos);
        reached[5] |= greedy && final_size < stochastic;
        reached[6] |= final_size > 1;
        reached
    }) {
        for (reached, flag) in reached.iter_mut().zip(flags) {
            *reached |= flag;
        }
    }
    assert_eq!(
        reached, [true; 7],
        "empty tiles, full/low bin merges, stochastic merges/early stop, greedy merges, multi-group"
    );
    eprintln!("Compared {} staged clustering/sampling cases", cases.len());
}

#[test]
fn clustering_fixtures_match_pinned_reference() {
    let cases = support::clustering::fixture_cases();
    let outputs = native::run("clustering-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C clustering and sampling fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/clustering.txt"),
            text,
        )
        .unwrap();
    }
}

#[test]
fn traceback_matches_pinned_reference() {
    let cases = support::matches::traceback_cases();
    let outputs = native::run("traceback", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} traceback cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn selection_matches_pinned_reference() {
    let cases = support::matches::selection_cases();
    let outputs = native::run("selection", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} selection cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn matching_matches_pinned_reference() {
    let cases = support::matches::matching_cases();
    let outputs = native::run("matches", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} matching cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn reference_and_histogram_foundations_match_pinned_reference() {
    let cases = foundation_cases();
    let outputs = native::run("foundations", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} reference/histogram cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn entropy_coding_matches_pinned_reference() {
    let cases = entropy_coding_cases();
    let outputs = native::run("entropy-coding", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} entropy coding cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn leaves_match_pinned_scalar_reference() {
    let cases = support::differential_cases();
    let outputs = native::run("leaves", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} leaf cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn scalar_dsp_matches_pinned_reference() {
    let cases = support::dsp_cases();
    let outputs = native::run("dsp", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!(
        "Compared {} scalar DSP cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[test]
fn committed_fixtures_match_reference() {
    let fixtures = support::fixtures();
    let mut cases: Vec<_> = fixtures.iter().map(|(case, _)| case.clone()).collect();
    // Include new compact cases when regenerating, without duplicating them on
    // subsequent runs. Committed outputs must always come from the C oracle.
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        for case in support::dsp_fixture_cases()
            .into_iter()
            .chain(entropy_coding_fixture_cases())
            .chain(foundation_fixture_cases())
            .chain(support::matches::matching_fixture_cases())
            .chain(support::matches::traceback_fixture_cases())
        {
            if !cases
                .iter()
                .any(|c| c.function == case.function && c.input == case.input)
            {
                cases.push(case);
            }
        }
    }
    let outputs = native::run("fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Scalar C oracle outputs; upstream {}\n\
            # Inputs/outputs are logical u32 words in hex: function input... | output...\n\
            # Build configuration and regeneration: tests/reference/README.md\n",
            support::UPSTREAM_REVISION
        );
        for (case, output) in cases.iter().zip(&outputs) {
            write!(text, "{}", case.function).unwrap();
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/leaves.txt"),
            text,
        )
        .unwrap();
    } else {
        for ((case, expected), actual) in fixtures.iter().zip(&outputs) {
            native::compare(case, expected, actual);
        }
    }
    for (case, output) in cases.iter().zip(outputs) {
        if case.function == "WebPEncodeLosslessRGBA" {
            native::compare(case, &output, &case.rust());
            let image = support::ImageCase {
                name: "public encoding fixture",
                width: case.input[0],
                height: case.input[1],
                stride: case.input[2] as usize,
                rgba: case.input[3..].iter().map(|&v| v as u8).collect(),
            };
            image.check_decoded(
                &output
                    .iter()
                    .map(|&v| u8::try_from(v).unwrap())
                    .collect::<Vec<_>>(),
            );
        } else {
            native::compare(case, &output, &case.rust());
        }
    }
}

#[test]
fn pinned_public_encoder_round_trips() {
    let images = support::image_cases();
    let cases: Vec<_> = images.iter().map(|image| image.request()).collect();
    for (image, output) in images.iter().zip(native::run("public", &cases)) {
        native::compare(&image.request(), &output, &image.request().rust());
        let bytes: Vec<_> = output
            .into_iter()
            .map(|v| u8::try_from(v).unwrap())
            .collect();
        image.check_decoded(&bytes);
        eprintln!("{}: {} encoded bytes", image.name, bytes.len());
    }
}

#[test]
fn auxiliary_streams_match_pinned_reference() {
    use libwebpop::enc::backward_references_enc::*;
    use libwebpop::enc::vp8l_enc::EncodeImageNoHuffman;
    use libwebpop::utils::bit_writer_utils::*;
    let mut cases = Vec::new();
    let mut seed = support::SEED;
    for (w, h) in [(1, 1), (1, 37), (41, 1), (17, 19), (64, 33)] {
        for pattern in 0..6 {
            let pixels: Vec<u32> = (0..w * h)
                .map(|i| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    match pattern {
                        0 => 0xff123456,
                        1 => {
                            if i % 2 == 0 {
                                0
                            } else {
                                0xffffffff
                            }
                        }
                        2 => 0xff000000 | (i % w) * 0x010203,
                        3 => seed,
                        4 => [0xff102030, 0x80506070, 0x00123456][(i / 7 % 3) as usize],
                        _ => seed % 17 * 0x0f0f0f0f,
                    }
                })
                .collect();
            for quality in [0, 25, 70, 100] {
                for low_effort in [0, 1] {
                    for prefix in [0, 7, 31] {
                        let mut input = vec![
                            w,
                            h,
                            quality,
                            low_effort,
                            5 & ((1u32 << prefix) - 1),
                            prefix,
                        ];
                        input.extend_from_slice(&pixels);
                        cases.push(support::Case::new("EncodeImageNoHuffman", input));
                    }
                }
            }
        }
    }
    let outputs = native::run("auxiliary", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        let a = &case.input;
        let mut bw = VP8LBitWriterInit(0).unwrap();
        VP8LPutBits(&mut bw, a[4], a[5]).unwrap();
        let mut chain = VP8LHashChainInit((a[0] * a[1]) as usize).unwrap();
        let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
        EncodeImageNoHuffman(
            &mut bw,
            &a[6..],
            &mut chain,
            &mut refs,
            a[0] as usize,
            a[1] as usize,
            a[2],
            a[3] != 0,
        )
        .unwrap();
        VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
        let actual: Vec<u32> = VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&b| u32::from(b))
            .collect();
        native::compare(case, &expected, &actual);
    }
    eprintln!("Compared {} auxiliary streams", cases.len());
}

#[test]
fn image_headers_match_pinned_reference() {
    use libwebpop::enc::vp8l_enc::{WriteImage, WriteImageSize, WriteRealAlphaAndVersion};
    use libwebpop::utils::bit_writer_utils::{VP8LBitWriterInit, VP8LPutBits};
    let mut cases = Vec::new();
    for width in [1, 2, 255, 256, 16383] {
        for height in [1, 2, 255, 256, 16383] {
            for alpha in [0, 1] {
                for bits in 0..=31 {
                    let value = 0x5ad5ad5 & ((1u32 << bits) - 1);
                    cases.push(support::Case::new(
                        "ImageHeaders",
                        vec![width, height, alpha, value, bits],
                    ));
                }
            }
        }
    }
    for (case, expected) in cases.iter().zip(native::run("image-headers", &cases)) {
        let a = &case.input;
        let mut bw = VP8LBitWriterInit(0).unwrap();
        WriteImageSize(&mut bw, a[0], a[1]).unwrap();
        WriteRealAlphaAndVersion(&mut bw, a[2] != 0).unwrap();
        VP8LPutBits(&mut bw, a[3], a[4]).unwrap();
        let actual: Vec<u32> = WriteImage(&mut bw)
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect();
        native::compare(case, &expected, &actual);
    }
    eprintln!("Compared {} image headers/containers", cases.len());
}

#[test]
fn token_streams_match_pinned_reference() {
    let cases = support::token_streams::cases();
    for (case, expected) in cases.iter().zip(native::run("token-streams", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} token streams and Huffman groups", cases.len());
}

#[test]
fn palette_leaves_match_pinned_reference() {
    let cases = support::palette::cases();
    for (case, expected) in cases.iter().zip(native::run("palette", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} palette cases", cases.len());
}

#[test]
fn palette_fixtures_match_pinned_reference() {
    let cases: Vec<_> = support::palette::cases().into_iter().step_by(13).collect();
    let outputs = native::run("palette-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C palette fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/palette.txt"),
            text,
        )
        .unwrap();
    }
}

#[test]
fn spatial_palette_fixtures_match_pinned_reference() {
    let cases: Vec<_> = support::palette::spatial_cases()
        .into_iter()
        .step_by(31)
        .collect();
    let outputs = native::run("spatial-palette-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C spatial palette fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/spatial_palette.txt"
            ),
            text,
        )
        .unwrap();
    }
}

#[test]
fn spatial_palette_orders_match_pinned_reference() {
    let cases = support::palette::spatial_cases();
    for (case, expected) in cases.iter().zip(native::run("spatial-palette", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} spatial palette cases", cases.len());
}
#[test]
fn palette_transform_fixtures_match_pinned_reference() {
    let cases: Vec<_> = support::palette::transform_cases()
        .into_iter()
        .step_by(23)
        .collect();
    let outputs = native::run("palette-transform-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C palette transform fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/palette_transform.txt"
            ),
            text,
        )
        .unwrap();
    }
}

#[test]
fn palette_transforms_match_pinned_reference() {
    let cases = support::palette::transform_cases();
    for (case, expected) in cases.iter().zip(native::run("palette-transform", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} palette transform cases", cases.len());
}

#[test]
fn predictor_fixtures_match_pinned_reference() {
    let cases: Vec<_> = support::predictors::fixture_cases();
    let outputs = native::run("predictor-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C predictor fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/predictors.txt"),
            text,
        )
        .unwrap();
    }
}

#[test]
fn predictor_foundations_match_pinned_reference() {
    let cases = support::predictors::cases();
    for (case, expected) in cases
        .iter()
        .zip(native::run("predictor-foundations", &cases))
    {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} predictor foundation cases", cases.len());
}

#[test]
fn predictor_search_matches_pinned_reference() {
    let cases = support::predictors::search_cases();
    native::parallel_cases("predictor-search", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
    });
    eprintln!("Compared {} automatic predictor search cases", cases.len());
}

#[test]
fn predictor_transforms_match_pinned_reference() {
    let cases: Vec<_> = support::predictors::clamp_cases()
        .into_iter()
        .chain(support::predictors::transform_cases())
        .collect();
    native::parallel_cases("predictor-transform", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
    });
    eprintln!(
        "Compared {} predictor transform/clamping cases",
        cases.len()
    );
}

#[test]
fn cross_color_leaves_match_pinned_reference() {
    let cases = support::cross_color::leaf_cases();
    for (case, expected) in cases.iter().zip(native::run("cross-color-leaves", &cases)) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("Compared {} cross-color leaf cases", cases.len());
}

#[test]
fn cross_color_search_matches_pinned_reference() {
    let cases = support::cross_color::search_cases();
    native::parallel_cases("cross-color-search", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
    });
    eprintln!("Compared {} cross-color search cases", cases.len());
}

#[test]
fn cross_color_transforms_match_pinned_reference() {
    let cases = support::cross_color::transform_cases();
    native::parallel_cases("cross-color-transform", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
    });
    eprintln!("Compared {} cross-color transform streams", cases.len());
}

#[test]
fn cross_color_fixtures_match_pinned_reference() {
    let cases = support::cross_color::fixture_cases();
    let outputs = native::run("cross-color-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C cross-color fixtures.\n# upstream={} seed={:08x}\n# See ../reference/README.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED,
        );
        for (case, expected) in cases.iter().zip(outputs) {
            text.push_str(native::request_text(case).trim_end());
            write!(text, " | {}", expected.len()).unwrap();
            for value in expected {
                write!(text, " {value:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/cross_color.txt"
            ),
            text,
        )
        .unwrap();
    }
}
