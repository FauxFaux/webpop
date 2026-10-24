// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_enc.c (owned ARGB and YUVA storage).
// See COPYING and PATENTS.

/// The owned ARGB subset of WebPPicture. Import validates geometry and stores
/// tightly packed rows. Vec ownership replaces C allocation/free and callbacks.
pub(crate) struct WebPPicture {
    pub width: u32,
    pub height: u32,
    pub argb: Vec<u32>,
}

/// Owned YUV420 picture used by the lossy path. Rows are tightly packed;
/// chroma dimensions round up, and an alpha plane exists only for nonopaque
/// input. Typed encoder routes keep ARGB and YUVA storage separate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebPYUVAPicture {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub a: Option<Vec<u8>>,
}
