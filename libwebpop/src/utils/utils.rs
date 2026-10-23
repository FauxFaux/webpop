// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/utils.h. See COPYING and PATENTS.

/// Store a logical word in little-endian order, without alignment requirements.
#[inline]
pub fn PutLE32(data: &mut [u8; 4], val: u32) {
    *data = val.to_le_bytes();
}

/// Floor of log2(n). As in upstream, the input must be nonzero.
#[inline]
pub const fn BitsLog2Floor(n: u32) -> u32 {
    assert!(n != 0);
    31 - n.leading_zeros()
}

/// Count trailing zeros. As in upstream's builtin path, n must be nonzero.
#[inline]
pub const fn BitsCtz(n: u32) -> u32 {
    assert!(n != 0);
    n.trailing_zeros()
}
