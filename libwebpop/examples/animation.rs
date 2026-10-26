//! Generate an optimized animation without an image-loader dependency.
//! Run: cargo run -p libwebpop --example animation -- /tmp/animation.webp
//! Animation currently uses the unsupported `internal` integration namespace.
use libwebpop::internal::{
    AnimationEncoder, AnimationEncodingPolicy, AnimationFrameCodec, AnimationFrameOptions,
    AnimationKeyframePolicy, AnimationOptimizerOptions, AnimationOptions, LossyOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args_os()
        .nth(1)
        .ok_or("usage: animation <output.webp>")?;
    let (width, height) = (16, 12);
    let mut encoder = AnimationEncoder::new(
        width,
        height,
        AnimationOptions {
            policy: AnimationEncodingPolicy::Optimized,
            optimizer: AnimationOptimizerOptions {
                allow_mixed: true,
                keyframes: AnimationKeyframePolicy::Bounded {
                    min_interval: 2,
                    max_interval: 5,
                },
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    let mut timestamp = 500;
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
        // Quality is fractional VP8 fidelity and truncated VP8L effort, not a
        // common visual-fidelity scale. Primary can differ from the winner.
        let settings = AnimationFrameOptions::Mixed {
            primary: if frame % 2 == 0 {
                AnimationFrameCodec::Vp8l
            } else {
                AnimationFrameCodec::Vp8
            },
            options: LossyOptions {
                quality: 75.25,
                method: 4,
                alpha_quality: 100,
                use_sharp_yuv: true,
            },
            near_lossless: 100,
            exact: false,
        };
        encoder.add_rgba(&rgba, width as usize * 4, timestamp, &settings)?;
        timestamp += duration;
    }
    let mut webp = Vec::new();
    encoder.finish(timestamp, &mut webp)?;
    std::fs::write(destination, webp)?;
    Ok(())
}
