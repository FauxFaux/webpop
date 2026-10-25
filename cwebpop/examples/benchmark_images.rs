//! Untimed input preparation and independent decoding for scripts/benchmark.py.
#![forbid(unsafe_code)]

use std::{env, fs, io::Write, path::Path};

fn load(path: &str) -> image::ImageResult<image::RgbaImage> {
    Ok(image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?
        .into_rgba8())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") if args.len() == 4 || args.len() == 5 => {
            let mut rgba = load(&args[2])?;
            if args.len() == 5 {
                let edge: u32 = args[4].parse()?;
                if edge == 0 { return Err("maximum edge must be positive".into()); }
                if rgba.width().max(rgba.height()) > edge {
                    rgba = image::DynamicImage::ImageRgba8(rgba).resize(edge, edge, image::imageops::FilterType::Lanczos3).into_rgba8();
                }
            }
            let (w, h) = rgba.dimensions();
            let mut file = fs::File::create(Path::new(&args[3]))?;
            write!(file, "P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n")?;
            file.write_all(rgba.as_raw())?;
            println!("{{\"width\":{w},\"height\":{h}}}");
        }
        Some("verify") if args.len() == 5 => {
            let expected = load(&args[2])?;
            let actual = load(&args[3])?;
            if expected.dimensions() != actual.dimensions() {
                return Err("decoded dimensions differ".into());
            }
            let exact = args[4] == "exact";
            for (i, (a, b)) in expected.pixels().zip(actual.pixels()).enumerate() {
                if a[3] != b[3] || ((exact || a[3] != 0) && a != b) {
                    return Err(format!("decoded pixel {i} differs: {a:?} != {b:?}").into());
                }
            }
        }
        Some("metrics") if args.len() == 4 => {
            let source = load(&args[2])?;
            let actual = load(&args[3])?;
            if source.dimensions() != actual.dimensions() { return Err("decoded dimensions differ".into()); }
            let (mut rgb_sse, mut alpha_sse, mut rgb_samples) = (0u64, 0u64, 0u64);
            for (a, b) in source.pixels().zip(actual.pixels()) {
                let square = |c: usize| (i64::from(a[c]) - i64::from(b[c])).pow(2) as u64;
                alpha_sse += square(3);
                if a[3] != 0 { rgb_sse += square(0) + square(1) + square(2); rgb_samples += 3; }
            }
            let alpha_samples = u64::from(source.width()) * u64::from(source.height());
            let psnr = |sse, count| if sse == 0 { "null".to_owned() } else { format!("{:.6}", 10. * (255. * 255. * count as f64 / sse as f64).log10()) };
            println!("{{\"rgb_sse\":{rgb_sse},\"rgb_samples\":{rgb_samples},\"alpha_sse\":{alpha_sse},\"alpha_samples\":{alpha_samples},\"rgb_psnr_db\":{},\"alpha_psnr_db\":{}}}", psnr(rgb_sse, rgb_samples), psnr(alpha_sse, alpha_samples));
        }
        _ => return Err("usage: benchmark_images prepare INPUT OUTPUT.pam [MAX_EDGE] | verify INPUT OUTPUT.webp exact|visible | metrics INPUT OUTPUT.webp".into()),
    }
    Ok(())
}
