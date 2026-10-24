// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/webp_enc.c (synchronous ARGB and YUVA paths).
// See COPYING and PATENTS.

use super::{
    alpha_enc::VP8EncStartAlpha,
    analysis_enc::VP8EncAnalyze,
    config_enc::{EncoderConfig, LossyConfig, WebPValidateConfig},
    frame_enc::VP8EncTokenLoop,
    picture_enc::{WebPPicture, WebPYUVAPicture},
    picture_tools_enc::{WebPCleanupTransparentArea, WebPReplaceTransparentPixels},
    syntax_enc::VP8EncWriteWithAlpha,
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

pub(crate) fn WebPEncode(
    config: &EncoderConfig,
    picture: &mut WebPPicture,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    WebPValidateConfig(&config.options)?;
    if !config.options.exact {
        WebPReplaceTransparentPixels(picture, 0);
    }
    VP8LEncodeImage(config, picture, output)
}

/// The default method-4 token path, with synchronous analysis/alpha jobs.
/// Separate owned picture types make the required storage explicit: callers
/// import ARGB for VP8L and YUVA for VP8 before entering either encoder.
pub(crate) fn WebPEncodeLossy(
    config: &LossyConfig,
    picture: &mut WebPYUVAPicture,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    WebPCleanupTransparentArea(picture);
    let mut enc = VP8Encoder::new_with_method(
        picture.width,
        picture.height,
        config.quality <= 98.0,
        config.method,
    )?;
    VP8EncAnalyze(&mut enc, picture);
    let alpha = VP8EncStartAlpha(picture, config.alpha_quality)?;
    let tokens = VP8EncTokenLoop(&mut enc, picture, config.quality)?;
    Ok(VP8EncWriteWithAlpha(
        &enc,
        picture.width,
        picture.height,
        0, // Default preset's complex filter uses VP8 profile 0.
        VP8BitWriterBuf(&tokens),
        alpha.as_ref().map(|a| a.data.as_slice()),
        output,
    )?)
}
