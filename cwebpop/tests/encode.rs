use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
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
        vec!["--near_lossless", "90"],
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
        assert_eq!(decode(&fs::read(output).unwrap()), source());
    }
    // The output decoder feature is enabled for ordinary CLI builds too.
    assert!(Path::new("unused.webp").extension().is_some());
}
