use std::{
    borrow::Cow,
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

use image::AnimationDecoder;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "cwebpop-gif-{}-{}",
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

// Exact indexed colors avoid quantization in the GIF test encoder. Later
// rectangles exercise restore-previous, clear-background and transparent skips.
fn gif(repeat: Option<gif::Repeat>, frame_count: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(
            &mut bytes,
            4,
            3,
            &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
        )
        .unwrap();
        if let Some(repeat) = repeat {
            encoder.set_repeat(repeat).unwrap();
        }
        let frames = [
            (0, 0, 4, 3, vec![1; 12], 0, gif::DisposalMethod::Keep),
            (1, 0, 1, 1, vec![2], 1, gif::DisposalMethod::Previous),
            (2, 1, 1, 1, vec![3], 7, gif::DisposalMethod::Background),
            (0, 2, 1, 1, vec![4], 3, gif::DisposalMethod::Keep),
            (1, 1, 2, 1, vec![0, 3], 0, gif::DisposalMethod::Keep),
        ];
        for (left, top, width, height, pixels, delay, dispose) in
            frames.into_iter().take(frame_count)
        {
            encoder
                .write_frame(&gif::Frame {
                    left,
                    top,
                    width,
                    height,
                    delay,
                    dispose,
                    transparent: Some(0),
                    buffer: Cow::Owned(pixels),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    bytes
}

fn expected_canvases() -> Vec<image::RgbaImage> {
    let red = image::Rgba([255, 0, 0, 255]);
    let mut canvas = image::RgbaImage::from_pixel(4, 3, red);
    let mut frames = vec![canvas.clone()];
    canvas.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    frames.push(canvas.clone());
    canvas.put_pixel(1, 0, red);
    canvas.put_pixel(2, 1, image::Rgba([0, 0, 255, 255]));
    frames.push(canvas.clone());
    canvas.put_pixel(2, 1, image::Rgba([0; 4]));
    canvas.put_pixel(0, 2, image::Rgba([255, 255, 0, 255]));
    frames.push(canvas.clone());
    canvas.put_pixel(2, 1, image::Rgba([0, 0, 255, 255]));
    frames.push(canvas);
    frames
}

fn decoded_frames(bytes: &[u8]) -> Vec<image::Frame> {
    let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).unwrap();
    assert!(decoder.has_animation());
    decoder.into_frames().collect_frames().unwrap()
}

fn expected_webp(loop_count: u16, config: &libwebpop::OpConfig, noalpha: bool) -> Vec<u8> {
    use libwebpop::{OpAnimEncoder, OpAnimOptions, OpImage};
    let mut options = OpAnimOptions::default();
    options.loop_count = loop_count;
    let mut encoder = OpAnimEncoder::new(4, 3, options).unwrap();
    for (mut rgba, delay) in expected_canvases().into_iter().zip([0, 10, 70, 30, 0]) {
        if noalpha {
            for pixel in rgba.pixels_mut() {
                pixel.0[3] = 255;
            }
        }
        encoder
            .add(
                OpImage::Rgba {
                    pixels: rgba.as_raw(),
                    width: 4,
                    height: 3,
                    stride: 16,
                },
                delay,
                config,
            )
            .unwrap();
    }
    encoder.finish().unwrap()
}

fn default_config() -> libwebpop::OpConfig {
    libwebpop::OpConfig::new(libwebpop::OpQuality::Lossless {
        near_lossless: 100,
        effort: 75,
        exact: false,
    })
}

fn loop_count(bytes: &[u8]) -> u16 {
    let mut rest = &bytes[12..];
    while !rest.is_empty() {
        let size = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
        if &rest[..4] == b"ANIM" {
            return u16::from_le_bytes(rest[12..14].try_into().unwrap());
        }
        rest = &rest[8 + size + (size & 1)..];
    }
    panic!("missing ANIM");
}

#[test]
fn gif_disposal_offsets_transparency_and_delays_roundtrip() {
    let temp = Scratch::new();
    // Content sniffing must select animated GIF even with a different suffix.
    let input = temp.file("animation.png");
    let output = temp.file("animation.webp");
    fs::write(&input, gif(Some(gif::Repeat::Finite(2)), 5)).unwrap();
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
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("5 GIF frames"));
    let bytes = fs::read(output).unwrap();
    assert_eq!(loop_count(&bytes), 3);
    let frames = decoded_frames(&bytes);
    assert_eq!(frames.len(), 5);
    // Exact pixel checks use the hand-constructed display canvases via the
    // established encoder, avoiding image-webp 0.2.4's known blend-rounding bug.
    assert_eq!(bytes, expected_webp(3, &default_config(), false));
    for (frame, delay) in frames.iter().zip([0, 10, 70, 30, 0]) {
        assert_eq!(frame.delay().numer_denom_ms(), (delay, 1));
    }
}

#[test]
fn gif_repetitions_map_to_webp_total_plays_including_late_extensions() {
    let temp = Scratch::new();
    let input = temp.file("input.gif");
    for (repeat, expected) in [
        (None, 1),
        (Some(gif::Repeat::Infinite), 0),
        (Some(gif::Repeat::Finite(1)), 2),
        (Some(gif::Repeat::Finite(65535)), 65535),
    ] {
        fs::write(&input, gif(repeat, 2)).unwrap();
        let result = command()
            .arg(&input)
            .args(["--quiet", "-o", "-"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        assert_eq!(loop_count(&result.stdout), expected);
    }
    let mut bytes = gif(None, 2);
    assert_eq!(bytes.pop(), Some(0x3b));
    bytes.extend_from_slice(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x04\x00\x00\x3b");
    fs::write(&input, bytes).unwrap();
    let result = command()
        .arg(&input)
        .args(["--quiet", "-o", "-"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(loop_count(&result.stdout), 5);
}

#[test]
fn gif_stdin_stdout_and_codec_options() {
    use libwebpop::{OpConfig, OpQuality};

    let temp = Scratch::new();
    let input = temp.file("input.gif");
    let bytes = gif(Some(gif::Repeat::Infinite), 5);
    fs::write(&input, &bytes).unwrap();
    for (flags, config, noalpha) in [
        (
            vec!["--exact"],
            OpConfig::new(OpQuality::Lossless {
                near_lossless: 100,
                effort: 75,
                exact: true,
            }),
            false,
        ),
        (
            vec!["--near_lossless", "60"],
            OpConfig::new(OpQuality::Lossless {
                near_lossless: 60,
                effort: 75,
                exact: false,
            }),
            false,
        ),
        (
            vec!["--lossy", "-q", "98.5", "--alpha_q", "70", "--sharp_yuv"],
            OpConfig::new(OpQuality::Lossy {
                quality: 98.5,
                alpha_quality: 70,
                sharp_yuv: true,
            }),
            false,
        ),
        (vec!["--noalpha"], default_config(), true),
    ] {
        let result = command()
            .arg(&input)
            .args(&flags)
            .args(["--quiet", "-o", "-"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
        assert_eq!(result.stdout, expected_webp(0, &config, noalpha));
        assert_eq!(loop_count(&result.stdout), 0);
        let frames = decoded_frames(&result.stdout);
        assert_eq!(frames.len(), 5);
        for (frame, delay) in frames.iter().zip([0, 10, 70, 30, 0]) {
            assert_eq!(frame.delay().numer_denom_ms(), (delay, 1));
            assert_eq!(frame.buffer().dimensions(), (4, 3));
        }
        if noalpha {
            assert!(
                frames
                    .iter()
                    .all(|frame| frame.buffer().pixels().all(|p| p.0[3] == 255))
            );
        }
        if flags.first() == Some(&"--lossy") {
            assert!(result.stdout.windows(4).any(|w| w == b"VP8 "));
        }
    }
    let mut child = command()
        .args(["--quiet", "-", "-o", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    assert_eq!(result.stdout, expected_webp(0, &default_config(), false));
}

#[test]
fn single_frame_gif_becomes_still_and_damaged_later_frames_preserve_output() {
    let temp = Scratch::new();
    let input = temp.file("input.gif");
    let output = temp.file("output.webp");
    fs::write(&input, gif(None, 1)).unwrap();
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
    let bytes = fs::read(&output).unwrap();
    assert!(
        !image::codecs::webp::WebPDecoder::new(Cursor::new(&bytes))
            .unwrap()
            .has_animation()
    );
    assert_eq!(
        image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8(),
        expected_canvases()[0]
    );

    let mut broken = gif(None, 5);
    broken.truncate(broken.len() - 4);
    for invalid in [broken, gif(None, 0), b"GIF89a".to_vec()] {
        fs::write(&input, invalid).unwrap();
        fs::write(&output, b"keep this").unwrap();
        let result = command()
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
        assert_eq!(fs::read(&output).unwrap(), b"keep this");
    }
}
