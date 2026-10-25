#![forbid(unsafe_code)]

use std::{
    fs,
    io::{self, Cursor, Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::Instant,
};

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use libwebpop::internal::{
    LosslessOptions, LossyOptions, encode_lossy_rgba, encode_near_lossless_rgba,
};

/// WebP encoder, based on upstream/examples/cwebp.c.
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Lossless, near-lossless and lossy WebP encoder",
    after_help = "Lossless encoding is the default; use --lossy for VP8. -o is required.\nUse --long_option for upstream's -long_option flags; -h/--help and -V/--version work."
)]
struct Args {
    /// Input image (or - for stdin).
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Write the encoded WebP to this file (or - for stdout).
    #[arg(short = 'o', long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Quality, 0..100 (default 75): color fidelity with --lossy, compression effort otherwise.
    #[arg(short = 'q', long)]
    quality: Option<f32>,

    /// Compression method, 0..6 (default 4).
    #[arg(short = 'm', long)]
    method: Option<u8>,

    /// Lossless preset level, 0..9.
    #[arg(short = 'z', long)]
    lossless_preset: Option<u8>,

    /// Filter strength, 0..100 (upstream default: 60).
    #[arg(short = 'f', long)]
    filter_strength: Option<u8>,

    /// Dump compressed output as PGM.
    #[arg(short = 'd', long)]
    dump: Option<PathBuf>,

    /// Raw YUV input dimensions.
    #[arg(short = 's', long, num_args = 2, value_names = ["WIDTH", "HEIGHT"])]
    input_size: Vec<u32>,

    /// Alpha quality with --lossy, 0..100 (default 100, preserving alpha exactly).
    #[arg(long = "alpha_q")]
    alpha_q: Option<u8>,

    /// Number of segments, 1..4 (upstream default: 4).
    #[arg(long = "segments")]
    segments: Option<u8>,

    /// Target output size in bytes.
    #[arg(long = "size")]
    size: Option<u64>,

    /// Target PSNR in dB.
    #[arg(long = "psnr")]
    psnr: Option<f32>,

    /// Spatial noise shaping, 0..100 (upstream default: 50).
    #[arg(long = "sns")]
    sns: Option<u8>,

    /// Filter sharpness, 0..7 (upstream default: 0).
    #[arg(long = "sharpness")]
    sharpness: Option<u8>,

    /// First partition quality degradation limit, 0..100.
    #[arg(long = "partition_limit")]
    partition_limit: Option<u8>,

    /// Analysis passes, 1..10.
    #[arg(long = "pass")]
    pass: Option<u8>,

    /// Alpha compression method, 0..1 (upstream default: 1).
    #[arg(long = "alpha_method")]
    alpha_method: Option<u8>,

    /// Near-lossless fidelity, 0..100 (default 100). Lower values can change RGB and alpha.
    #[arg(long = "near_lossless")]
    near_lossless: Option<u8>,

    /// Print extra information map.
    #[arg(long = "map")]
    map: Option<u8>,

    /// Lossy preprocessing: 0 for ordinary conversion, 4 for SharpYUV.
    #[arg(long = "pre")]
    pre: Option<u32>,

    /// Encoder preset; upstream requires this before other options.
    #[arg(long = "preset", value_parser = ["default", "photo", "picture", "drawing", "icon", "text"])]
    preset: Option<String>,

    /// Alpha predictive filtering (upstream default: fast).
    #[arg(long = "alpha_filter", value_parser = ["none", "fast", "best"])]
    alpha_filter: Option<String>,

    /// Image characteristics hint.
    #[arg(long = "hint", value_parser = ["photo", "picture", "graph"])]
    hint: Option<String>,

    /// Resize policy (upstream default: always).
    #[arg(long = "resize_mode", value_parser = ["up_only", "down_only", "always"])]
    resize_mode: Option<String>,

    /// Metadata to copy (upstream default: none).
    #[arg(long, value_delimiter = ',', value_parser = ["all", "none", "exif", "icc", "xmp"])]
    metadata: Vec<String>,

    /// Blend against an RGB hexadecimal background, e.g. 0xc0e0d0.
    #[arg(long = "blend_alpha")]
    blend_alpha: Option<String>,

    /// Permissible quality range (upstream default: 0 100).
    #[arg(long, num_args = 2, value_names = ["MIN", "MAX"])]
    qrange: Vec<f32>,

    /// Crop the input rectangle.
    #[arg(long, num_args = 4, value_names = ["X", "Y", "WIDTH", "HEIGHT"])]
    crop: Vec<u32>,

    /// Resize after cropping; zero preserves aspect ratio.
    #[arg(long, num_args = 2, value_names = ["WIDTH", "HEIGHT"])]
    resize: Vec<u32>,

    /// Use the strong filter (upstream default).
    #[arg(long = "strong")]
    strong: bool,

    /// Use the simple filter.
    #[arg(long = "nostrong")]
    nostrong: bool,

    /// Use sharper RGB to YUV conversion (requires --lossy).
    #[arg(long = "sharp_yuv")]
    sharp_yuv: bool,

    /// Enable multithreading.
    #[arg(long = "mt")]
    mt: bool,

    /// Reduce memory use at the cost of speed.
    #[arg(long = "low_memory")]
    low_memory: bool,

    /// Print average PSNR distortion.
    #[arg(long = "print_psnr")]
    print_psnr: bool,

    /// Print average SSIM distortion.
    #[arg(long = "print_ssim")]
    print_ssim: bool,

    /// Print local similarity distortion.
    #[arg(long = "print_lsim")]
    print_lsim: bool,

    /// Disable transparent RGB cleanup (near-lossless may still change pixels).
    #[arg(long = "exact")]
    exact: bool,

    /// Clean up RGB values in transparent pixels (legacy upstream option).
    #[arg(long = "alpha_cleanup")]
    alpha_cleanup: bool,

    /// Discard transparency.
    #[arg(long = "noalpha")]
    noalpha: bool,

    /// Encode losslessly (the default).
    #[arg(long = "lossless")]
    lossless: bool,

    /// Encode lossy VP8 with the default preset; -q controls color quality.
    #[arg(long, conflicts_with_all = ["lossless", "near_lossless", "exact"])]
    lossy: bool,

    /// Condense encoding statistics.
    #[arg(long = "short")]
    short: bool,

    /// Suppress encoding statistics.
    #[arg(long = "quiet")]
    quiet: bool,

    /// Disable assembly optimizations.
    #[arg(long = "noasm")]
    noasm: bool,

    /// Report encoding progress.
    #[arg(long = "progress")]
    progress: bool,

    /// Experimental: roughly match expected JPEG size.
    #[arg(long = "jpeg_like")]
    jpeg_like: bool,

    /// Experimental: automatically adjust filter strength.
    #[arg(long = "af")]
    af: bool,

    /// Print timing information.
    #[arg(short = 'v', long)]
    verbose: bool,
}

fn main() -> ExitCode {
    match run(&Args::command().get_matches()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cwebpop: {error}");
            ExitCode::FAILURE
        }
    }
}

type Error = Box<dyn std::error::Error>;

#[derive(Debug, PartialEq)]
enum EncoderOptions {
    Lossless(LosslessOptions),
    Lossy(LossyOptions),
}

fn encoder_options(args: &Args, matches: &ArgMatches) -> Result<EncoderOptions, Error> {
    // Keep the existing option inventory, but reject unimplemented operations
    // before opening files instead of silently ignoring requested behavior.
    const SUPPORTED: &[&str] = &[
        "input",
        "output",
        "quality",
        "method",
        "exact",
        "noalpha",
        "lossless",
        "quiet",
        "short",
        "verbose",
        "noasm",
        "near_lossless",
        "lossy",
        "alpha_q",
        "sharp_yuv",
        "pre",
    ];
    let command = Args::command();
    // matches.ids() also includes clap's derived "Args" group. Walk actual
    // argument definitions so groups never need to appear in this allowlist.
    for arg in command.get_arguments() {
        let id = arg.get_id();
        if matches.value_source(id.as_str()) == Some(ValueSource::CommandLine)
            && !SUPPORTED.contains(&id.as_str())
        {
            let flag = if let Some(long) = arg.get_long() {
                format!("--{long}")
            } else if let Some(short) = arg.get_short() {
                format!("-{short}")
            } else {
                id.to_string()
            };
            return Err(format!("{flag} is not implemented").into());
        }
    }
    if args.near_lossless.is_some_and(|q| q > 100) {
        return Err("near-lossless strength must be in 0..=100".into());
    }
    let quality = args.quality.unwrap_or(75.0);
    if !quality.is_finite() || !(0.0..=100.0).contains(&quality) {
        return Err("quality must be a finite number in 0..=100".into());
    }
    let method = args.method.unwrap_or(4);
    if method > 6 {
        return Err("method must be in 0..=6".into());
    }
    if args.lossy {
        let alpha_quality = i32::from(args.alpha_q.unwrap_or(100));
        if alpha_quality > 100 {
            return Err("alpha quality must be in 0..=100".into());
        }
        if args.pre.is_some_and(|pre| !matches!(pre, 0 | 4)) {
            return Err("--pre currently supports only 0 or 4 (SharpYUV)".into());
        }
        return Ok(EncoderOptions::Lossy(LossyOptions {
            quality,
            method: u32::from(method),
            alpha_quality,
            use_sharp_yuv: args.sharp_yuv || args.pre == Some(4),
        }));
    }
    if args.alpha_q.is_some() {
        return Err("--alpha_q requires --lossy".into());
    }
    if args.sharp_yuv || args.pre.is_some() {
        return Err("--sharp_yuv and --pre require --lossy".into());
    }
    Ok(EncoderOptions::Lossless(LosslessOptions {
        quality: quality as u32,
        method: u32::from(method),
        exact: args.exact,
    }))
}

fn run(matches: &ArgMatches) -> Result<(), Error> {
    let args = Args::from_arg_matches(matches)?;
    let options = encoder_options(&args, matches)?;
    let output = args.output.as_ref().ok_or("-o OUTPUT is required")?;
    let started = Instant::now();
    let bytes = if args.input.as_os_str() == "-" {
        let mut bytes = Vec::new();
        io::stdin().lock().read_to_end(&mut bytes)?;
        bytes
    } else {
        fs::read(&args.input).map_err(|e| format!("cannot read {}: {e}", args.input.display()))?
    };
    let mut reader = image::ImageReader::new(Cursor::new(bytes));
    // TGA has no reliable magic bytes. Retain the filename's format as a
    // fallback while still sniffing content for other formats and stdin.
    if let Ok(format) = image::ImageFormat::from_path(&args.input) {
        reader.set_format(format);
    }
    let mut rgba = reader
        .with_guessed_format()?
        .decode()
        .map_err(|e| format!("cannot decode {}: {e}", args.input.display()))?
        .into_rgba8();
    if args.noalpha {
        for pixel in rgba.pixels_mut() {
            pixel.0[3] = 255;
        }
    }
    let (width, height) = rgba.dimensions();
    let near_lossless = args.near_lossless.unwrap_or(100);
    let mut encoded = Vec::new();
    match &options {
        EncoderOptions::Lossless(options) => encode_near_lossless_rgba(
            rgba.as_raw(),
            width,
            height,
            width as usize * 4,
            i32::from(near_lossless),
            options,
            &mut encoded,
        )?,
        EncoderOptions::Lossy(options) => encode_lossy_rgba(
            rgba.as_raw(),
            width,
            height,
            width as usize * 4,
            options,
            &mut encoded,
        )?,
    };
    if output.as_os_str() == "-" {
        let mut stdout = io::stdout().lock();
        stdout.write_all(&encoded)?;
        stdout.flush()?;
    } else {
        fs::write(output, &encoded)
            .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    }
    if !args.quiet {
        if args.short {
            eprintln!("{}", encoded.len());
        } else {
            let mode = if args.lossy {
                "lossy"
            } else if near_lossless < 100 {
                "near-lossless"
            } else {
                "lossless"
            };
            eprintln!(
                "Encoded {width}x{height} {mode} WebP: {} bytes",
                encoded.len()
            );
        }
        if args.verbose {
            eprintln!("Elapsed: {:.3}s", started.elapsed().as_secs_f64());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn command_is_consistent() {
        Args::command().debug_assert();
        let matches = Args::command()
            .try_get_matches_from(["cwebpop", "input.png"])
            .unwrap();
        let args = Args::from_arg_matches(&matches).unwrap();
        assert_eq!(
            encoder_options(&args, &matches).unwrap(),
            EncoderOptions::Lossless(LosslessOptions {
                quality: 75,
                method: 4,
                exact: false,
            })
        );
    }

    #[test]
    fn parses_lossless_and_image_operations() {
        let args = Args::try_parse_from([
            "cwebpop",
            "--lossless",
            "-q",
            "80",
            "--exact",
            "--crop",
            "1",
            "2",
            "30",
            "40",
            "--resize",
            "15",
            "0",
            "--metadata",
            "icc,exif",
            "input.png",
            "-o",
            "output.webp",
        ])
        .unwrap();
        assert!(args.lossless && args.exact);
        assert_eq!(args.quality, Some(80.0));
        assert_eq!(args.crop, [1, 2, 30, 40]);
        assert_eq!(args.resize, [15, 0]);
        assert_eq!(args.metadata, ["icc", "exif"]);
        assert_eq!(args.output, Some(PathBuf::from("output.webp")));
    }

    #[test]
    fn rejects_missing_values_and_invalid_choices() {
        for argv in [
            vec!["cwebpop", "input.png", "--crop", "1", "2", "3"],
            vec!["cwebpop", "input.png", "--preset", "unknown"],
            vec!["cwebpop", "input.png", "--metadata", "unknown"],
        ] {
            assert!(Args::try_parse_from(argv).is_err());
        }
    }

    #[test]
    fn help_and_version_work_without_input() {
        for (flag, kind) in [
            ("--help", ErrorKind::DisplayHelp),
            ("--version", ErrorKind::DisplayVersion),
        ] {
            assert_eq!(
                Args::try_parse_from(["cwebpop", flag]).unwrap_err().kind(),
                kind
            );
        }
        assert_eq!(
            Args::try_parse_from(["cwebpop"]).unwrap_err().kind(),
            ErrorKind::MissingRequiredArgument
        );
        let args = Args::try_parse_from(["cwebpop", "--", "-input.png"]).unwrap();
        assert_eq!(args.input, PathBuf::from("-input.png"));
    }
}
