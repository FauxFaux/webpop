#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::utils::bit_writer_utils::*;
use support::vp8_boolean::{self as boolean, Decoder};

#[test]
fn committed_boolean_writer_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_boolean.txt"));
    let cases = boolean::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn emitted_fields_decode_independently() {
    let mut streams = 0;
    let mut bits = 0;
    for case in boolean::cases() {
        let a = &case.input;
        if (a.len() - 2) % 3 != 0 {
            continue;
        }
        let ops: Vec<_> = a[2..].chunks_exact(3).collect();
        if ops.last().map(|v| v[0]) != Some(4) || ops[..ops.len() - 1].iter().any(|v| v[0] > 3) {
            continue;
        }
        let output = case.rust();
        let state_words = 9 * if a[1] != 0 { ops.len() + 1 } else { 2 };
        let bytes: Vec<_> = output[state_words + 1..].iter().map(|&v| v as u8).collect();
        assert_eq!(output[state_words] as usize, bytes.len());
        let mut decoder = Decoder::new(&bytes);
        for op in &ops[..ops.len() - 1] {
            match op[0] {
                0 | 1 => {
                    assert_eq!(
                        decoder.bit(if op[0] == 0 { op[2] as u8 } else { 128 }),
                        op[1] != 0
                    );
                    bits += 1;
                }
                2 => {
                    for shift in (0..op[2]).rev() {
                        assert_eq!(decoder.bit(128), op[1] & (1 << shift) != 0);
                        bits += 1;
                    }
                }
                3 => {
                    let value = op[1] as i32;
                    assert_eq!(decoder.bit(128), value != 0);
                    bits += 1;
                    if value != 0 {
                        let mut magnitude = 0u32;
                        for _ in 0..op[2] {
                            magnitude = 2 * magnitude + u32::from(decoder.bit(128));
                            bits += 1;
                        }
                        assert_eq!(magnitude, value.unsigned_abs());
                        assert_eq!(decoder.bit(128), value < 0);
                        bits += 1;
                    }
                }
                _ => unreachable!(),
            }
        }
        streams += 1;
    }
    assert!(
        streams > 8000 && bits > 1_000_000,
        "{streams} streams, {bits} bits"
    );
    eprintln!("independently decoded {streams} streams and {bits} bits");
}

#[test]
fn deferred_runs_resolve_to_ff_or_zero_and_can_carry_into_prefix() {
    let mut max_run = 0;
    let mut carries = 0;
    let mut non_carries = 0;
    // Repeated one bits create long 0xff runs. Resolve them with each extreme
    // probability/outcome after multiple prefix lengths, then finish.
    for len in [8, 9, 16, 31, 127, 1023, 4096] {
        for probability in [0, 1, 127, 128, 254, 255] {
            for bit in [0, 1] {
                let mut bw = VP8BitWriterInit(0).unwrap();
                let mut previous: Vec<u8> = Vec::new();
                let mut run = 0;
                for index in 0..len + 2 {
                    if index < len {
                        VP8PutBitUniform(&mut bw, 1).unwrap();
                    } else if index == len {
                        VP8PutBit(&mut bw, bit, probability).unwrap();
                    } else {
                        VP8BitWriterFinish(&mut bw).unwrap();
                    }
                    max_run = max_run.max(bw.run);
                    let bytes = VP8BitWriterBuf(&bw);
                    if run > 0 && bw.run < run {
                        let pos = previous.len();
                        let resolved = &bytes[pos..pos + run];
                        if resolved.iter().all(|&v| v == 0) {
                            carries += 1;
                            if pos > 0 {
                                assert_eq!(bytes[pos - 1], previous[pos - 1].wrapping_add(1));
                            }
                        } else {
                            non_carries += 1;
                            assert!(resolved.iter().all(|&v| v == 255));
                            assert_eq!(&bytes[..pos], previous);
                        }
                    }
                    previous = bytes.to_vec();
                    run = bw.run;
                    assert!((127..=254).contains(&bw.range));
                    assert!((-8..=0).contains(&bw.nb_bits));
                    assert_eq!(VP8BitWriterSize(&bw), bytes.len());
                }
            }
        }
    }
    let mut bw = VP8BitWriterInit(0).unwrap();
    for (bit, probability) in boolean::CARRY_BITS {
        let prefix = VP8BitWriterBuf(&bw).to_vec();
        let run = bw.run;
        VP8PutBit(&mut bw, bit, probability).unwrap();
        let bytes = VP8BitWriterBuf(&bw);
        if run > 0 && bw.run < run {
            assert_eq!(&bytes[prefix.len()..prefix.len() + run], vec![0; run]);
            assert_eq!(bytes[prefix.len() - 1], prefix.last().unwrap() + 1);
            assert_eq!(&bytes[..prefix.len() - 1], &prefix[..prefix.len() - 1]);
            carries += 1;
        }
    }
    assert!(max_run >= 500, "longest run {max_run}");
    assert!(
        carries > 0 && non_carries > 0,
        "carry {carries}, no carry {non_carries}"
    );
    eprintln!(
        "maximum deferred run {max_run}, carry resolutions {carries}, no-carry resolutions {non_carries}"
    );
}

#[test]
fn append_alignment_reservation_failure_and_wipe() {
    assert!(VP8BitWriterInit(usize::MAX).is_err());
    let mut bw = VP8BitWriterInit(0).unwrap();
    assert_eq!(VP8BitWriterPos(&bw), 0);
    assert!(VP8BitWriterAppend(&mut bw, &[7, 255, 3]).unwrap());
    assert_eq!(VP8BitWriterPos(&bw), 24);
    VP8PutBitUniform(&mut bw, 1).unwrap();
    let mut before = Vec::new();
    boolean::inspect(&bw, 0, &mut before);
    assert!(!VP8BitWriterAppend(&mut bw, &[9; 1025]).unwrap());
    assert!(!VP8BitWriterAppend(&mut bw, &[]).unwrap());
    let mut after = Vec::new();
    boolean::inspect(&bw, 0, &mut after);
    assert_eq!(before, after);
    VP8BitWriterFinish(&mut bw).unwrap();
    let coded_size = VP8BitWriterSize(&bw);
    assert_eq!(bw.nb_bits, -8);
    assert!(VP8BitWriterAppend(&mut bw, &[0x5a; 4096]).unwrap());
    assert_eq!(VP8BitWriterSize(&bw), coded_size + 4096);
    assert_eq!(&VP8BitWriterBuf(&bw)[coded_size..], &[0x5a; 4096]);
    VP8BitWriterWipeOut(&mut bw);
    VP8BitWriterWipeOut(&mut bw);
    assert_eq!((bw.range, bw.value, bw.run, bw.nb_bits), (0, 0, 0, 0));
    assert_eq!(VP8BitWriterSize(&bw), 0);
    bw = VP8BitWriterInit(2048).unwrap();
    VP8PutBits(&mut bw, 0x5ad, 11).unwrap();
    let bytes = VP8BitWriterFinish(&mut bw).unwrap();
    let mut d = Decoder::new(bytes);
    let value = (0..11).fold(0, |v, _| 2 * v + u32::from(d.bit(128)));
    assert_eq!(value, 0x5ad);
}

#[cfg(feature = "native-oracle")]
#[test]
fn boolean_writer_matches_pinned_reference() {
    let cases = boolean::cases();
    native::parallel_cases("vp8-boolean", &cases, |case, output| {
        native::compare(case, output, &case.rust());
    });
    eprintln!("{} exact boolean writer trace comparisons", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn boolean_writer_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = boolean::fixture_cases();
    let outputs = native::run("vp8-boolean-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 boolean writer logical traces.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_boolean.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_boolean.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
