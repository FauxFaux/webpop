//! Generate an optimized animation without an image-loader dependency.
//! Run: cargo run -p libwebpop --example animation -- /tmp/animation.webp
use libwebpop::{OpAnimEncoder, OpAnimKeyframes, OpAnimOptions, OpConfig, OpImage, OpQuality};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .ok_or("usage: animation <output.webp>")?;
    let (width, height) = (16, 12);
    let mut options = OpAnimOptions::default();
    options.allow_mixed = true;
    options.keyframes = OpAnimKeyframes::Bounded {
        min_interval: 2,
        max_interval: 5,
    };
    let mut encoder = OpAnimEncoder::new(width, height, options)?;
    for (frame, duration) in [100, 0, 40, 100, 50, 1].into_iter().enumerate() {
        let mut rgba = vec![0; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                if x >= frame as u32 && x < frame as u32 + 5 && (3..9).contains(&y) {
                    rgba[((y * width + x) * 4) as usize..][..4]
                        .copy_from_slice(&[20, 100, 220, 192]);
                }
            }
        }
        // Mixed trials may choose either codec. Lossless effort is also used
        // as reversed VP8 quality; VP8 quality is truncated for reversed VP8L.
        let config = OpConfig::new(if frame % 2 == 0 {
            OpQuality::Lossless {
                near_lossless: 100,
                effort: 75,
                exact: false,
            }
        } else {
            OpQuality::Lossy {
                quality: 75.25,
                alpha_quality: 100,
                sharp_yuv: true,
            }
        });
        encoder.add(
            OpImage::Rgba {
                pixels: &rgba,
                width,
                height,
                stride: width as usize * 4,
            },
            duration,
            &config,
        )?;
    }
    std::fs::write(destination, encoder.finish()?)?;
    Ok(())
}
