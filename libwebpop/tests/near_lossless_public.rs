#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod preprocessing;
#[path = "support/near_lossless_public.rs"]
mod public;
mod support;

#[test]
fn committed_near_lossless_reference_files_decode_and_100_matches_lossless() {
    let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_public.txt"));
    let cases = public::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut changes = (0, 0);
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        let changed = public::check_reference(case, expected);
        changes.0 += changed.0;
        changes.1 += changed.1;
    }
    assert!(
        changes.0 > 0 && changes.1 > 0,
        "changed RGB/alpha: {changes:?}"
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn near_lossless_public_reference_corpus() {
    let cases = public::cases();
    let outputs = native::run("near-lossless-public", &cases);
    let mut changes = (0, 0);
    // Inputs are consecutive strength runs for each image. Bucket equality is
    // a pinned mapping property, not a monotonicity assumption about file size.
    for (index, (case, output)) in cases.iter().zip(&outputs).enumerate() {
        let changed = public::check_reference(case, output);
        changes.0 += changed.0;
        changes.1 += changed.1;
        if index > 0
            && case.input[3] / 20 == cases[index - 1].input[3] / 20
            && case.input[..3] == cases[index - 1].input[..3]
            && case.input[4..] == cases[index - 1].input[4..]
        {
            native::compare(case, &outputs[index - 1], output);
        }
    }
    assert!(
        changes.0 > 0 && changes.1 > 0,
        "changed RGB/alpha: {changes:?}"
    );
    let error_case = support::Case::new("NearLosslessWrapperErrors", vec![]);
    assert_eq!(
        native::run("near-lossless-wrapper-errors", &[error_case]),
        [vec![1; 5]]
    );
    eprintln!(
        "Independently decoded {} near-lossless C files; strength 100 matches Rust lossless",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn near_lossless_public_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = public::fixture_cases();
    let outputs = native::run("near-lossless-public-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        public::check_reference(case, output);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C custom public near-lossless wrapper fixtures.\n# upstream={} seed={:08x}\n# See ../reference/NEAR_LOSSLESS.md for wrapper and fixture provenance.\n",
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
                "/tests/fixtures/near_lossless_public.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_public.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
