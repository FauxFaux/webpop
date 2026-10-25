//! Encode-only measurements on prepared packed RGBA; see scripts/benchmark-lossy.py.
#![forbid(unsafe_code)]

use libwebpop::internal::{LossyOptions, encode_lossy_rgba};
use std::{env, fs, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = env::args().collect();
    if a.len() != 9 {
        return Err("usage: lossy_benchmark INPUT.rgba WIDTH HEIGHT QUALITY ALPHA_QUALITY METHOD RUNS OUTPUT.webp".into());
    }
    let source = fs::read(&a[1])?;
    let (width, height): (u32, u32) = (a[2].parse()?, a[3].parse()?);
    let options = LossyOptions {
        quality: a[4].parse()?,
        alpha_quality: a[5].parse()?,
        method: a[6].parse()?,
        use_sharp_yuv: false,
    };
    let runs: usize = a[7].parse()?;
    if !(1..=100).contains(&runs) {
        return Err("RUNS must be 1..=100".into());
    }
    let mut first = None;
    let mut times = Vec::new();
    for run in 0..=runs {
        let mut output = Vec::new();
        let started = Instant::now();
        encode_lossy_rgba(
            &source,
            width,
            height,
            width as usize * 4,
            &options,
            &mut output,
        )?;
        let elapsed = started.elapsed().as_secs_f64();
        if run == 0 {
            continue;
        } // One warm-up, then fresh output per sample.
        times.push(elapsed);
        if let Some(first) = &first {
            assert_eq!(&output, first);
        } else {
            first = Some(output);
        }
    }
    fs::write(&a[8], first.unwrap())?;
    println!("{times:?}");
    Ok(())
}
