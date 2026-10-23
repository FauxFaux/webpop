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
        Some("prepare") if args.len() == 4 => {
            let rgba = load(&args[2])?;
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
        _ => return Err("usage: benchmark_images prepare INPUT OUTPUT.pam | verify INPUT OUTPUT.webp exact|visible".into()),
    }
    Ok(())
}
