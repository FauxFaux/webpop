// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/bit_writer_utils.{c,h}.
// See COPYING and PATENTS.

use std::collections::TryReserveError;

/// VP8 boolean arithmetic coder. Initialize with `VP8BitWriterInit` before use.
/// Numeric fields retain the upstream logical state; buffer ownership is Rust's.
/// Allocation failures propagate immediately, so callers must stop on error.
#[derive(Debug, Default)]
pub struct VP8BitWriter {
    pub range: i32,
    pub value: i32,
    pub run: usize,
    pub nb_bits: i32,
    buf: Vec<u8>,
}

pub fn VP8BitWriterInit(expected_size: usize) -> Result<VP8BitWriter, TryReserveError> {
    let mut bw = VP8BitWriter {
        range: 254,
        nb_bits: -8,
        ..VP8BitWriter::default()
    };
    bw.buf.try_reserve_exact(expected_size)?;
    Ok(bw)
}

#[inline]
pub fn VP8BitWriterPos(bw: &VP8BitWriter) -> u64 {
    (bw.buf.len() as u64 + bw.run as u64) * 8 + (8 + bw.nb_bits) as u64
}

#[inline]
pub fn VP8BitWriterBuf(bw: &VP8BitWriter) -> &[u8] {
    &bw.buf
}

#[inline]
pub fn VP8BitWriterSize(bw: &VP8BitWriter) -> usize {
    bw.buf.len()
}

// The upstream lookup tables normalize range-1 to 127..=254.
const fn norm_tables() -> ([u8; 128], [u8; 128]) {
    let mut norm = [0; 128];
    let mut range = [0; 128];
    let mut i = 0;
    while i < 128 {
        let shift = (i as u32 + 1).leading_zeros() - 24;
        norm[i] = shift as u8;
        range[i] = (((i as u32 + 1) << shift) - 1) as u8;
        i += 1;
    }
    (norm, range)
}
const NORM: ([u8; 128], [u8; 128]) = norm_tables();

fn Flush(bw: &mut VP8BitWriter) -> Result<(), TryReserveError> {
    assert!(bw.nb_bits >= 0);
    let shift = 8 + bw.nb_bits;
    let bits = bw.value >> shift;
    bw.value -= bits << shift;
    bw.nb_bits -= 8;
    if bits & 0xff == 0xff {
        bw.run += 1;
    } else {
        // Reserve before mutating the preceding byte or releasing the run.
        bw.buf.try_reserve(bw.run + 1)?;
        let carry = bits & 0x100 != 0;
        if carry && let Some(last) = bw.buf.last_mut() {
            *last = last.wrapping_add(1);
        }
        bw.buf
            .resize(bw.buf.len() + bw.run, if carry { 0 } else { 0xff });
        bw.run = 0;
        bw.buf.push(bits as u8);
    }
    Ok(())
}

/// Encode zero or nonzero `bit` using an eight-bit probability of zero.
/// The return value preserves the supplied bit, as in the C API.
pub fn VP8PutBit(bw: &mut VP8BitWriter, bit: i32, prob: u8) -> Result<i32, TryReserveError> {
    let split = (bw.range * i32::from(prob)) >> 8;
    if bit != 0 {
        bw.value += split + 1;
        bw.range -= split + 1;
    } else {
        bw.range = split;
    }
    if bw.range < 127 {
        let index = bw.range as usize;
        let shift = NORM.0[index];
        bw.range = i32::from(NORM.1[index]);
        bw.value <<= shift;
        bw.nb_bits += i32::from(shift);
        if bw.nb_bits > 0 {
            Flush(bw)?;
        }
    }
    Ok(bit)
}

pub fn VP8PutBitUniform(bw: &mut VP8BitWriter, bit: i32) -> Result<i32, TryReserveError> {
    let split = bw.range >> 1;
    if bit != 0 {
        bw.value += split + 1;
        bw.range -= split + 1;
    } else {
        bw.range = split;
    }
    if bw.range < 127 {
        bw.range = i32::from(NORM.1[bw.range as usize]);
        bw.value <<= 1;
        bw.nb_bits += 1;
        if bw.nb_bits > 0 {
            Flush(bw)?;
        }
    }
    Ok(bit)
}

/// Encode the low `nb_bits` bits, most significant bit first (1..=31).
pub fn VP8PutBits(bw: &mut VP8BitWriter, value: u32, nb_bits: u32) -> Result<(), TryReserveError> {
    assert!((1..32).contains(&nb_bits));
    for shift in (0..nb_bits).rev() {
        VP8PutBitUniform(bw, (value & (1 << shift)) as i32)?;
    }
    Ok(())
}

/// Emit a nonzero flag, then magnitude and sign for a value fitting `nb_bits`
/// magnitude bits (0..=30). A zero value emits only its flag.
pub fn VP8PutSignedBits(
    bw: &mut VP8BitWriter,
    value: i32,
    nb_bits: u32,
) -> Result<(), TryReserveError> {
    assert!(nb_bits < 31);
    assert!(value.unsigned_abs() < (1 << nb_bits));
    if VP8PutBitUniform(bw, i32::from(value != 0))? != 0 {
        VP8PutBits(
            bw,
            (value.unsigned_abs() << 1) | u32::from(value < 0),
            nb_bits + 1,
        )?;
    }
    Ok(())
}

/// Terminate the coder and pad its final byte. Call once per coded stream.
pub fn VP8BitWriterFinish(bw: &mut VP8BitWriter) -> Result<&[u8], TryReserveError> {
    VP8PutBits(bw, 0, (9 - bw.nb_bits) as u32)?;
    bw.nb_bits = 0;
    Flush(bw)?;
    Ok(&bw.buf)
}

/// Append bytes at a flushed boundary. Returns false without changing state
/// when arithmetic-coded bits are pending.
pub fn VP8BitWriterAppend(bw: &mut VP8BitWriter, data: &[u8]) -> Result<bool, TryReserveError> {
    if bw.nb_bits != -8 {
        return Ok(false);
    }
    bw.buf.try_reserve(data.len())?;
    bw.buf.extend_from_slice(data);
    Ok(true)
}

pub fn VP8BitWriterWipeOut(bw: &mut VP8BitWriter) {
    *bw = VP8BitWriter::default();
}

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

#[inline]
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
#[inline]
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
