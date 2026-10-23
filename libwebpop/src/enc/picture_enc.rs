// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_enc.c (owned ARGB storage).
// See COPYING and PATENTS.

/// The owned ARGB subset of WebPPicture. Import validates geometry and stores
/// tightly packed rows. Vec ownership replaces C allocation/free and callbacks.
pub(crate) struct WebPPicture {
    pub width: u32,
    pub height: u32,
    pub argb: Vec<u32>,
}
