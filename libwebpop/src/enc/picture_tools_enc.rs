// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_tools_enc.c and scalar AlphaReplace_C.
// See COPYING and PATENTS.

use super::picture_enc::WebPPicture;

pub(crate) fn WebPReplaceTransparentPixels(picture: &mut WebPPicture, color: u32) {
    let color = color & 0xffffff;
    for pixel in &mut picture.argb {
        if *pixel >> 24 == 0 {
            *pixel = color;
        }
    }
}
