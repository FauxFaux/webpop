#[path = "support/native.rs"]
mod native;
mod support;

use std::fmt::Write;
use support::entropy_coding::{entropy_coding_cases, entropy_coding_fixture_cases};

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
        let bytes: Vec<_> = output
            .into_iter()
            .map(|v| u8::try_from(v).unwrap())
            .collect();
        image.check_decoded(&bytes);
        eprintln!("{}: {} encoded bytes", image.name, bytes.len());
    }
}
