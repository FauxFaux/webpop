// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/bit_writer_utils.{c,h} (VP8L only).
// See COPYING and PATENTS.

use std::collections::TryReserveError;

/// LSB-first lossless writer. The accumulator uses the upstream 64-bit path;
/// output bytes are independent of the host's endianness and word size.
/// Allocation failures propagate immediately instead of setting C's error flag.
#[derive(Debug, Default)]
pub struct VP8LBitWriter {
    bits: u64,
    used: u32,
    buf: Vec<u8>,
}

pub fn VP8LBitWriterInit(expected_size: usize) -> Result<VP8LBitWriter, TryReserveError> {
    let mut bw = VP8LBitWriter::default();
    bw.buf.try_reserve_exact(expected_size)?;
    Ok(bw)
}

pub fn VP8LBitWriterNumBytes(bw: &VP8LBitWriter) -> usize {
    bw.buf.len() + bw.used.div_ceil(8) as usize
}

pub fn VP8LBitWriterClone(src: &VP8LBitWriter) -> Result<VP8LBitWriter, TryReserveError> {
    let mut dst = VP8LBitWriterInit(src.buf.len())?;
    dst.buf.extend_from_slice(&src.buf);
    dst.bits = src.bits;
    dst.used = src.used;
    Ok(dst)
}

/// Restore the cursor/accumulator of an earlier trial in this same stream.
/// As in C, this does not restore buffer contents: the flushed prefix must
/// still match the snapshot. Use Clone to restore an independent stream.
pub fn VP8LBitWriterReset(bw_init: &VP8LBitWriter, bw: &mut VP8LBitWriter) {
    assert!(bw_init.buf.len() <= bw.buf.len());
    bw.buf.truncate(bw_init.buf.len());
    bw.bits = bw_init.bits;
    bw.used = bw_init.used;
}

pub fn VP8LBitWriterSwap(src: &mut VP8LBitWriter, dst: &mut VP8LBitWriter) {
    std::mem::swap(src, dst);
}

pub fn VP8LBitWriterWipeOut(bw: &mut VP8LBitWriter) {
    *bw = VP8LBitWriter::default();
}

fn VP8LPutBitsFlushBits(bw: &mut VP8LBitWriter) -> Result<(), TryReserveError> {
    bw.buf.try_reserve(4)?;
    bw.buf.extend_from_slice(&(bw.bits as u32).to_le_bytes());
    bw.bits >>= 32;
    bw.used -= 32;
    Ok(())
}

/// Append up to 32 bits. Callers supply a value that fits in n_bits.
pub fn VP8LPutBits(bw: &mut VP8LBitWriter, bits: u32, n_bits: u32) -> Result<(), TryReserveError> {
    assert!(n_bits <= 32);
    if n_bits == 0 {
        return Ok(());
    }
    assert!(n_bits == 32 || bits < (1 << n_bits));
    if bw.used >= 32 {
        VP8LPutBitsFlushBits(bw)?;
    }
    bw.bits |= u64::from(bits) << bw.used;
    bw.used += n_bits;
    Ok(())
}

/// Flush the remaining bits, padding the final byte with zeroes. May be called
/// repeatedly; the returned bytes remain owned by the writer.
pub fn VP8LBitWriterFinish(bw: &mut VP8LBitWriter) -> Result<&[u8], TryReserveError> {
    bw.buf.try_reserve(bw.used.div_ceil(8) as usize)?;
    while bw.used > 0 {
        bw.buf.push(bw.bits as u8);
        bw.bits >>= 8;
        bw.used = bw.used.saturating_sub(8);
    }
    Ok(&bw.buf)
}
