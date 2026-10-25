// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/webp_enc.c (synchronous ARGB and YUVA paths).
// See COPYING and PATENTS.

use super::{
    alpha_enc::VP8EncStartAlpha,
    analysis_enc::VP8EncAnalyze,
    config_enc::{EncoderConfig, LossyConfig, WebPValidateConfig},
    frame_enc::{VP8EncLoop, VP8EncTokenLoop},
    picture_enc::{WebPPicture, WebPYUVAPicture},
    picture_tools_enc::{WebPCleanupTransparentArea, WebPReplaceTransparentPixels},
    syntax_enc::{GeneratePartition0, PutVP8FrameHeader},
    vp8i_enc::{VP8Encoder, VP8RDLevel},
    vp8l_enc::VP8LEncodeImage,
};
use crate::{utils::bit_writer_utils::VP8BitWriterBuf, webp::encode::LosslessEncodeError};

/// Normal-memory tool mapping from the pinned webp_enc.c. Method validation
/// precedes construction; partition_limit remains 0 and target search is off.
pub(crate) fn MapConfigToTools(enc: &mut VP8Encoder) {
    enc.rd_opt_level = match enc.config.method {
        0..=2 => VP8RDLevel::RD_OPT_NONE,
        3..=4 => VP8RDLevel::RD_OPT_BASIC,
        5 => VP8RDLevel::RD_OPT_TRELLIS,
        6 => VP8RDLevel::RD_OPT_TRELLIS_ALL,
        _ => unreachable!("VP8 method was validated before encoder allocation"),
    };
    enc.use_tokens = !matches!(enc.rd_opt_level, VP8RDLevel::RD_OPT_NONE);
    enc.max_i4_header_bits = 256 * 16 * 16;
    enc.mb_header_limit = 256 * 510 * 8 * 1024 / (enc.mb_w * enc.mb_h) as i64;
}

pub(crate) fn WebPEncode<T>(
    config: &EncoderConfig,
    picture: &mut WebPPicture,
    write: impl FnOnce(crate::mux::muxinternal::EncodedImage<'_>) -> Result<T, LosslessEncodeError>,
) -> Result<T, LosslessEncodeError> {
    WebPValidateConfig(&config.options)?;
    if !config.options.exact {
        WebPReplaceTransparentPixels(picture, 0);
    }
    VP8LEncodeImage(config, picture, write)
}

/// Method-dependent frame coding with synchronous analysis/alpha jobs.
/// Separate owned picture types make the required storage explicit: callers
/// import ARGB for VP8L and YUVA for VP8 before entering either encoder.
pub(crate) fn WebPEncodeLossy<T>(
    config: &LossyConfig,
    picture: &mut WebPYUVAPicture,
    write: impl FnOnce(crate::mux::muxinternal::EncodedImage<'_>) -> Result<T, LosslessEncodeError>,
) -> Result<T, LosslessEncodeError> {
    WebPCleanupTransparentArea(picture);
    let mut enc = VP8Encoder::new_with_method(
        picture.width,
        picture.height,
        config.quality <= 98.0,
        config.method,
    )?;
    enc.config.quality = config.quality;
    enc.config.filter_strength = config.filter_strength;
    VP8EncAnalyze(&mut enc, picture);
    let alpha = VP8EncStartAlpha(picture, config.alpha_quality, config.method as u32)?;
    let residuals = if enc.use_tokens {
        VP8EncTokenLoop(&mut enc, picture, config.quality)?
    } else {
        VP8EncLoop(&mut enc, picture, config.quality)?
    };
    // InitEncoder selects profile 2 when filtering is disabled. The default
    // complex filter uses profile 0; autofilter is off in both supported paths.
    let profile = if config.filter_strength > 0 { 0 } else { 2 };
    PutVP8FrameHeader(picture.width, picture.height, profile, 0)?;
    let partition0 = GeneratePartition0(&enc)?;
    write(crate::mux::muxinternal::EncodedImage::Vp8 {
        width: picture.width,
        height: picture.height,
        profile,
        partition0: VP8BitWriterBuf(&partition0),
        residuals: VP8BitWriterBuf(&residuals),
        alpha: alpha.as_ref().map(|a| a.data.as_slice()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enc::picture_csp_enc::import_rgba_to_yuva;

    #[test]
    fn internal_method_config_reaches_color_and_alpha_jobs() {
        let mut counts = [0; 7];
        for line in include_str!("../../tests/fixtures/lossy_alpha_methods.txt").lines() {
            let Some(line) = line.strip_prefix("WebPEncodeLossyMethod ") else {
                continue;
            };
            let (input, expected) = line.split_once('|').unwrap();
            let words = |s: &str| {
                s.split_whitespace()
                    .map(|s| u32::from_str_radix(s, 16).unwrap())
                    .collect::<Vec<_>>()
            };
            let a = words(input);
            let rgba: Vec<_> = a[7..].iter().map(|&v| v as u8).collect();
            let mut pic = import_rgba_to_yuva(&rgba, a[0], a[1], a[2] as usize).unwrap();
            let config = LossyConfig::new(f32::from_bits(a[3]), a[4] as i32, a[5] as i32).unwrap();
            let mut output = Vec::new();
            WebPEncodeLossy(&config, &mut pic, |image| image.write_still(&mut output)).unwrap();
            assert_eq!(
                output.into_iter().map(u32::from).collect::<Vec<_>>(),
                words(expected)
            );
            counts[a[5] as usize] += 1;
        }
        assert_eq!(counts, [2; 7]);
    }
}
