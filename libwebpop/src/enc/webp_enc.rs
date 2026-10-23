// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/webp_enc.c (lossless ARGB branch).
// See COPYING and PATENTS.

use super::{
    config_enc::{EncoderConfig, WebPValidateConfig},
    picture_enc::WebPPicture,
    picture_tools_enc::WebPReplaceTransparentPixels,
    vp8l_enc::VP8LEncodeImage,
};
use crate::webp::encode::LosslessEncodeError;

pub(crate) fn WebPEncode(
    config: &EncoderConfig,
    picture: &mut WebPPicture,
) -> Result<Vec<u8>, LosslessEncodeError> {
    WebPValidateConfig(&config.options)?;
    if !config.options.exact {
        WebPReplaceTransparentPixels(picture, 0);
    }
    VP8LEncodeImage(config, picture)
}
