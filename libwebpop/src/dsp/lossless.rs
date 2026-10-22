// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/lossless.h. See COPYING and PATENTS.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VP8LMultipliers {
    // Byte representations of signed coefficients, as in upstream.
    pub green_to_red: u8,
    pub green_to_blue: u8,
    pub red_to_blue: u8,
}
