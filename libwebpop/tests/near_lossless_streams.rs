#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod preprocessing;
#[path = "support/near_lossless_streams.rs"]
mod streams;
mod support;

// The private encoder is evaluated by the unit test in vp8l_enc.rs against
// these same C outputs. No public advanced near-lossless configuration is added.
#[cfg(feature = "native-oracle")]
#[test]
fn candidate_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = streams::fixture_cases();
    let outputs = native::run("near-lossless-stream-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C near-lossless candidate replay and full-loop fixtures.\n# upstream={} seed={:08x}\n# See ../reference/NEAR_LOSSLESS.md.\n",
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
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/near_lossless_streams.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_streams.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(outputs)) {
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, &output);
        }
    }
    eprintln!("{} C candidate replay/full-loop comparisons", cases.len());
}
