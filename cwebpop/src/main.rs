#![forbid(unsafe_code)]

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;

/// Rust WebP encoder CLI scaffold, based on upstream/examples/cwebp.c.
#[derive(Debug, Parser)]
#[command(
    version,
    about = "WebP encoder CLI scaffold",
    after_help = "Encoding is not implemented yet. All encoding options are parsed only.\nUse --long_option for upstream's -long_option flags; -h/--help and -V/--version work."
)]
struct Args {
    /// Input image (or - for stdin).
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Write the encoded WebP to this file (or - for stdout).
    #[arg(short = 'o', long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Quality factor, 0..100 (upstream default: 75).
    #[arg(short = 'q', long)]
    quality: Option<f32>,

    /// Compression method, 0..6 (upstream default: 4).
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

    /// Alpha quality, 0..100 (upstream default: 100).
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

    /// Near-lossless preprocessing, 0..100 (100 disables it).
    #[arg(long = "near_lossless")]
    near_lossless: Option<u8>,

    /// Print extra information map.
    #[arg(long = "map")]
    map: Option<u8>,

    /// Experimental preprocessing filter.
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

    /// Use sharper RGB to YUV conversion.
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

    /// Preserve RGB values in transparent pixels.
    #[arg(long = "exact")]
    exact: bool,

    /// Clean up RGB values in transparent pixels (legacy upstream option).
    #[arg(long = "alpha_cleanup")]
    alpha_cleanup: bool,

    /// Discard transparency.
    #[arg(long = "noalpha")]
    noalpha: bool,

    /// Encode losslessly.
    #[arg(long = "lossless")]
    lossless: bool,

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
    let _args = Args::parse();
    eprintln!("cwebpop: encoding is not implemented yet; no output was written");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    #[test]
    fn command_is_consistent() {
        Args::command().debug_assert();
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
