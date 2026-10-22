// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/webp/format_constants.h. See COPYING and PATENTS.

pub const MAX_CACHE_BITS: u32 = 11;
pub const MAX_ALLOWED_CODE_LENGTH: usize = 15;
pub const CODE_LENGTH_CODES: usize = 19;
pub const MIN_HUFFMAN_BITS: u32 = 2;
pub const NUM_HUFFMAN_BITS: u32 = 3;
pub const NUM_LITERAL_CODES: usize = 256;
pub const NUM_LENGTH_CODES: usize = 24;
pub const NUM_DISTANCE_CODES: usize = 40;
pub const MAX_LENGTH_BITS: u32 = 12;
pub const MAX_LENGTH: u32 = (1 << MAX_LENGTH_BITS) - 1;
pub const LZ77_WINDOW_SIZE_BITS: u32 = 20;
pub const LZ77_WINDOW_SIZE: u32 = (1 << LZ77_WINDOW_SIZE_BITS) - 120;
