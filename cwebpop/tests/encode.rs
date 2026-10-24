use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "cwebpop-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cwebpop"))
}

fn source() -> image::RgbaImage {
    image::RgbaImage::from_fn(17, 19, |x, y| {
        image::Rgba([
            (x * 17) as u8,
            (y * 23) as u8,
            (x * y) as u8,
            if (x + y) % 3 == 0 { 0 } else { 255 },
        ])
    })
}

fn decode(bytes: &[u8]) -> image::RgbaImage {
    assert_eq!(&bytes[..4], b"RIFF");
    image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
        .unwrap()
        .to_rgba8()
}

#[test]
fn default_cli_matches_upstream_cli_effort() {
    let temp = Scratch::new();
    let input = temp.file("input.png");
    let output = temp.file("output.webp");
    let image = source();
    image.save(&input).unwrap();
    let result = command()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = libwebpop::encode_lossless_rgba(
        image.as_raw(),
        image.width(),
        image.height(),
        image.width() as usize * 4,
        &libwebpop::LosslessOptions {
            quality: 75,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(output).unwrap(), expected);
}

#[test]
fn files_and_streams_round_trip_rgba() {
    let temp = Scratch::new();
    let input = temp.file("input.png");
    let output = temp.file("output.webp");
    let expected = source();
    expected.save(&input).unwrap();
    let result = command()
        .args(["--lossless", "--exact", "-q", "100", "-m", "6"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert_eq!(decode(&fs::read(&output).unwrap()), expected);

    let mut child = command()
        .args(["--exact", "--quiet", "-", "-o", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&fs::read(&input).unwrap())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(decode(&result.stdout), expected);
}

#[test]
fn opaque_input_and_discarding_alpha_work() {
    let temp = Scratch::new();
    let input = temp.file("opaque.bmp");
    let output = temp.file("output.webp");
    let expected = image::RgbImage::from_fn(7, 9, |x, y| image::Rgb([x as u8, y as u8, 123]));
    expected.save(&input).unwrap();
    let result = command()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .args(["-m", "0", "--quiet"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let decoded = decode(&fs::read(&output).unwrap());
    for (actual, expected) in decoded.pixels().zip(expected.pixels()) {
        assert_eq!(actual.0, [expected.0[0], expected.0[1], expected.0[2], 255]);
    }
    let input = temp.file("alpha.png");
    source().save(&input).unwrap();
    let result = command()
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .args(["--noalpha", "--quiet"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for (actual, expected) in decode(&fs::read(&output).unwrap())
        .pixels()
        .zip(source().pixels())
    {
        assert_eq!(actual.0, [expected.0[0], expected.0[1], expected.0[2], 255]);
    }
}

#[test]
fn malformed_input_and_unsupported_options_do_not_overwrite_output() {
    let temp = Scratch::new();
    let input = temp.file("bad.png");
    let output = temp.file("existing.webp");
    fs::write(&input, b"invalid image").unwrap();
    fs::write(&output, b"keep this").unwrap();
    for flags in [
        vec![],
        vec!["-q", "NaN"],
        vec!["-q", "101"],
        vec!["-m", "7"],
        vec!["--near_lossless", "101"],
        vec!["--resize", "3", "4"],
    ] {
        let result = command()
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .args(flags)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!result.stderr.is_empty());
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"keep this");
    }
    let result = command().arg(&input).output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("-o OUTPUT is required"));
    let result = command()
        .arg(temp.file("missing.png"))
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&output).unwrap(), b"keep this");
}

#[test]
fn decoding_uses_file_content_with_extension_fallback() {
    let temp = Scratch::new();
    for (name, format) in [
        ("image.bin", image::ImageFormat::Png),
        ("image.tga", image::ImageFormat::Tga),
    ] {
        let mut bytes = Cursor::new(Vec::new());
        source().write_to(&mut bytes, format).unwrap();
        let input = temp.file(name);
        let output = temp.file("out.webp");
        fs::write(&input, bytes.into_inner()).unwrap();
        let result = command()
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg("--exact")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(decode(&fs::read(&output).unwrap()), source());
        let copy = temp.file("copy.webp");
        let result = command()
            .arg(&output)
            .arg("-o")
            .arg(&copy)
            .arg("--exact")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(decode(&fs::read(copy).unwrap()), source());
    }
}

#[test]
fn near_lossless_honors_strength_effort_exact_and_alpha_options() {
    let temp = Scratch::new();
    let input = temp.file("noise.png");
    let output = temp.file("noise.webp");
    let mut seed = 0x6d2b79f5u32;
    let source = image::RgbaImage::from_fn(65, 5, |x, _| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        image::Rgba([
            seed as u8,
            (seed >> 8) as u8,
            (seed >> 16) as u8,
            [0, 127, 253, 255][x as usize % 4],
        ])
    });
    source.save(&input).unwrap();
    let mut changed = [false; 4];
    for strength in [0, 20, 60, 99, 100] {
        // Omitted effort keeps CLI defaults. Explicit values must override
        // those defaults independently of the near-lossless strength.
        for (effort, method, exact, noalpha) in [
            (None, None, false, false),
            (Some("70"), Some(4), false, false),
            (Some("90.9"), Some(6), false, false),
            (Some("0"), Some(0), false, false),
            (Some("100"), Some(6), true, false),
            (None, None, false, true),
        ] {
            let mut command = command();
            command
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .arg("--near_lossless")
                .arg(strength.to_string());
            if let Some(q) = effort {
                command.args(["-q", q]);
            }
            if let Some(m) = method {
                command.arg("-m").arg(m.to_string());
            }
            if exact {
                command.arg("--exact");
            }
            if strength == 60 {
                command.arg("--lossless");
            }
            if noalpha {
                command.arg("--noalpha");
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stdout.is_empty());
            let message = String::from_utf8_lossy(&result.stderr);
            assert!(message.contains(if strength < 100 {
                "near-lossless WebP"
            } else {
                "lossless WebP"
            }));
            let mut pixels = source.clone();
            if noalpha {
                for p in pixels.pixels_mut() {
                    p.0[3] = 255;
                }
            }
            let options = libwebpop::LosslessOptions {
                quality: effort
                    .map(|q| q.parse::<f32>().unwrap() as u32)
                    .unwrap_or(75),
                method: method.unwrap_or(4),
                exact,
            };
            let expected = libwebpop::encode_near_lossless_rgba(
                pixels.as_raw(),
                65,
                5,
                65 * 4,
                strength,
                &options,
            )
            .unwrap();
            let actual = fs::read(&output).unwrap();
            assert_eq!(actual, expected);
            let decoded = decode(&actual);
            assert_eq!(decoded.dimensions(), pixels.dimensions());
            for (a, b) in pixels.pixels().zip(decoded.pixels()) {
                if a[3] == 0 || a[3] == 255 {
                    assert_eq!(a[3], b[3]);
                }
                for lane in 0..4 {
                    if lane == 3 || (a[3] != 0 && b[3] != 0) {
                        changed[lane] |= a[lane] != b[lane];
                    }
                }
            }
            if strength == 100 {
                assert_eq!(
                    actual,
                    libwebpop::encode_lossless_rgba(pixels.as_raw(), 65, 5, 65 * 4, &options)
                        .unwrap()
                );
            }
        }
    }
    assert!(
        changed.into_iter().all(|v| v),
        "near-lossless must actually change RGB and intermediate alpha"
    );
}

#[test]
fn near_lossless_streams_and_invalid_strengths_preserve_output_contracts() {
    let temp = Scratch::new();
    let input = temp.file("input.png");
    let output = temp.file("output.webp");
    source().save(&input).unwrap();
    fs::write(&output, b"keep this").unwrap();
    for strength in ["-1", "101", "256"] {
        let result = command()
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .arg(format!("--near_lossless={strength}"))
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"keep this");
    }
    // Settings fail before attempting to open even an absent input file.
    let result = command()
        .arg(temp.file("missing.png"))
        .arg("-o")
        .arg(&output)
        .args(["--near_lossless", "101"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("near-lossless strength must be in 0..=100")
    );
    let mut child = command()
        .args([
            "--near_lossless",
            "60",
            "-q",
            "90",
            "-m",
            "6",
            "--quiet",
            "-",
            "-o",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&fs::read(&input).unwrap())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(
        result.stdout,
        libwebpop::encode_near_lossless_rgba(
            source().as_raw(),
            17,
            19,
            17 * 4,
            60,
            &libwebpop::LosslessOptions {
                quality: 90,
                method: 6,
                exact: false
            }
        )
        .unwrap()
    );
}

#[test]
fn lossy_files_and_streams_preserve_fractional_quality_and_alpha_settings() {
    let temp = Scratch::new();
    let input = temp.file("alpha.png");
    let output = temp.file("alpha.webp");
    let image = image::RgbaImage::from_fn(17, 19, |x, y| {
        image::Rgba([
            (x * 17) as u8,
            (y * 23) as u8,
            (x * y) as u8,
            (x * 17 + y * 13) as u8,
        ])
    });
    image.save(&input).unwrap();
    for (q, aq, noalpha) in [
        (None, None, false),
        (Some("98.5"), Some("0"), false),
        (Some("100"), Some("71"), false),
        (Some("30.25"), Some("0"), true),
    ] {
        let mut command = command();
        command
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .args(["--lossy", "-m", "4"]);
        if let Some(q) = q {
            command.args(["-q", q]);
        }
        if let Some(aq) = aq {
            command.args(["--alpha_q", aq]);
        }
        if noalpha {
            command.arg("--noalpha");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("lossy WebP"));
        let mut pixels = image.clone();
        if noalpha {
            for p in pixels.pixels_mut() {
                p[3] = 255;
            }
        }
        let expected = libwebpop::WebPEncodeRGBAWithAlphaQuality(
            pixels.as_raw(),
            17,
            19,
            17 * 4,
            q.map(|v| v.parse().unwrap()).unwrap_or(75.),
            aq.map(|v| v.parse().unwrap()).unwrap_or(100),
        )
        .unwrap();
        let actual = fs::read(&output).unwrap();
        assert_eq!(actual, expected);
        let decoded = decode(&actual);
        assert_eq!(decoded.dimensions(), (17, 19));
        if aq.is_none() || noalpha {
            for (a, b) in pixels.pixels().zip(decoded.pixels()) {
                assert_eq!(a[3], b[3]);
            }
        }
    }
    let mut child = command()
        .args([
            "--lossy",
            "-q",
            "98.5",
            "--alpha_q",
            "30",
            "--quiet",
            "-",
            "-o",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&fs::read(&input).unwrap())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(
        result.stdout,
        libwebpop::WebPEncodeRGBAWithAlphaQuality(image.as_raw(), 17, 19, 17 * 4, 98.5, 30)
            .unwrap()
    );
}

#[test]
fn invalid_lossy_settings_fail_before_input_and_preserve_output() {
    let temp = Scratch::new();
    let output = temp.file("existing.webp");
    fs::write(&output, b"keep this").unwrap();
    for flags in [
        vec!["--lossy", "-q", "NaN"],
        vec!["--lossy", "-q", "inf"],
        vec!["--lossy", "-q", "101"],
        vec!["--lossy", "--alpha_q=101"],
        vec!["--lossy", "--alpha_q=-1"],
        vec!["--lossy", "-m", "0"],
        vec!["--lossy", "-m", "6"],
        vec!["--lossy", "--lossless"],
        vec!["--lossy", "--near_lossless", "100"],
        vec!["--lossy", "--exact"],
        vec!["--lossy", "--alpha_filter", "best"],
        vec!["--lossy", "--alpha_method", "0"],
        vec!["--alpha_q", "100"],
    ] {
        let result = command()
            .arg(temp.file("absent.png"))
            .arg("-o")
            .arg(&output)
            .args(&flags)
            .output()
            .unwrap();
        assert!(!result.status.success(), "{flags:?}");
        assert!(result.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!stderr.is_empty());
        assert!(!stderr.contains("cannot read"), "{flags:?}: {stderr}");
        assert_eq!(fs::read(&output).unwrap(), b"keep this");
    }
}
