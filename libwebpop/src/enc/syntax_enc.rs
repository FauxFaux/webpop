// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/syntax_enc.c. See COPYING and PATENTS.

use super::{tree_enc::*, vp8i_enc::*};
use crate::utils::bit_writer_utils::*;
use std::collections::TryReserveError;

pub const VP8_MAX_PARTITION0_SIZE: usize = 1 << 19;

#[derive(Debug, thiserror::Error)]
pub enum VP8WriteError {
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
    #[error("invalid VP8 frame dimensions or profile")]
    InvalidFrame,
    #[error("VP8 partition 0 exceeds its 19-bit length")]
    Partition0Overflow,
    #[error("WebP file exceeds the RIFF size limit")]
    FileTooBig,
}

pub fn PutSegmentHeader(bw: &mut VP8BitWriter, enc: &VP8Encoder) -> Result<(), TryReserveError> {
    let hdr = &enc.segment_hdr;
    if VP8PutBitUniform(bw, i32::from(hdr.num_segments > 1))? != 0 {
        VP8PutBitUniform(bw, i32::from(hdr.update_map))?;
        VP8PutBitUniform(bw, 1)?; // update quantizers and filter strengths
        VP8PutBitUniform(bw, 1)?; // absolute segment features
        for s in &enc.dqm {
            VP8PutSignedBits(bw, s.quant, 7)?;
        }
        for s in &enc.dqm {
            VP8PutSignedBits(bw, s.fstrength, 6)?;
        }
        if hdr.update_map {
            for p in enc.proba.segments {
                if VP8PutBitUniform(bw, i32::from(p != 255))? != 0 {
                    VP8PutBits(bw, u32::from(p), 8)?;
                }
            }
        }
    }
    Ok(())
}

pub fn PutFilterHeader(
    bw: &mut VP8BitWriter,
    hdr: &VP8EncFilterHeader,
) -> Result<(), TryReserveError> {
    VP8PutBitUniform(bw, i32::from(hdr.simple))?;
    VP8PutBits(bw, hdr.level as u32, 6)?;
    VP8PutBits(bw, hdr.sharpness as u32, 3)?;
    if VP8PutBitUniform(bw, i32::from(hdr.i4x4_lf_delta != 0))? != 0 {
        VP8PutBitUniform(bw, 1)?; // update deltas
        VP8PutBits(bw, 0, 4)?; // reference deltas unused
        VP8PutSignedBits(bw, hdr.i4x4_lf_delta, 6)?;
        VP8PutBits(bw, 0, 3)?; // other mode deltas unused
    }
    Ok(())
}

pub fn PutQuant(bw: &mut VP8BitWriter, enc: &VP8Encoder) -> Result<(), TryReserveError> {
    VP8PutBits(bw, enc.base_quant as u32, 7)?;
    for delta in [
        enc.dq_y1_dc,
        enc.dq_y2_dc,
        enc.dq_y2_ac,
        enc.dq_uv_dc,
        enc.dq_uv_ac,
    ] {
        VP8PutSignedBits(bw, delta, 4)?;
    }
    Ok(())
}

/// Default token-loop syntax uses one coefficient partition. The returned
/// writer is finished and owns its partition-0 bytes.
pub fn GeneratePartition0(enc: &VP8Encoder) -> Result<VP8BitWriter, TryReserveError> {
    let mut bw = VP8BitWriterInit(enc.mb_info.len() * 7 / 8)?;
    VP8PutBitUniform(&mut bw, 0)?; // colorspace
    VP8PutBitUniform(&mut bw, 0)?; // clamp type
    PutSegmentHeader(&mut bw, enc)?;
    PutFilterHeader(&mut bw, &enc.filter_hdr)?;
    VP8PutBits(&mut bw, 0, 2)?; // one coefficient partition
    PutQuant(&mut bw, enc)?;
    VP8PutBitUniform(&mut bw, 0)?; // no entropy refresh for subsequent frames
    VP8WriteProbas(&mut bw, &enc.proba)?;
    VP8CodeIntraModes(&mut bw, enc)?;
    VP8BitWriterFinish(&mut bw)?;
    Ok(bw)
}

pub fn PutVP8FrameHeader(
    width: u32,
    height: u32,
    profile: u8,
    size0: usize,
) -> Result<[u8; 10], VP8WriteError> {
    if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) || profile > 3 {
        return Err(VP8WriteError::InvalidFrame);
    }
    if size0 >= VP8_MAX_PARTITION0_SIZE {
        return Err(VP8WriteError::Partition0Overflow);
    }
    let bits = ((size0 as u32) << 5) | (u32::from(profile) << 1) | 16;
    let tag = bits.to_le_bytes();
    let w = (width as u16).to_le_bytes();
    let h = (height as u16).to_le_bytes();
    Ok([
        tag[0], tag[1], tag[2], 0x9d, 0x01, 0x2a, w[0], w[1], h[0], h[1],
    ])
}

/// Check lengths without allocating, including synthetic sizes near RIFF limits.
/// Pinned libwebp includes the final padding byte in the VP8 chunk length.
pub fn OpaqueWebPSizes(size0: usize, token_size: usize) -> Result<(u32, u32), VP8WriteError> {
    if size0 >= VP8_MAX_PARTITION0_SIZE {
        return Err(VP8WriteError::Partition0Overflow);
    }
    let vp8 = 10usize
        .checked_add(size0)
        .and_then(|s| s.checked_add(token_size))
        .and_then(|s| s.checked_add(s & 1))
        .ok_or(VP8WriteError::FileTooBig)?;
    let riff = vp8
        .checked_add(12)
        .filter(|&s| s <= 0xffff_fffe)
        .ok_or(VP8WriteError::FileTooBig)?;
    Ok((vp8 as u32, riff as u32))
}

pub fn AssembleOpaqueWebP(
    width: u32,
    height: u32,
    profile: u8,
    partition0: &[u8],
    tokens: &[u8],
) -> Result<Vec<u8>, VP8WriteError> {
    let header = PutVP8FrameHeader(width, height, profile, partition0.len())?;
    let (vp8, riff) = OpaqueWebPSizes(partition0.len(), tokens.len())?;
    let total = (riff as usize)
        .checked_add(8)
        .ok_or(VP8WriteError::FileTooBig)?;
    let mut out = Vec::new();
    out.try_reserve_exact(total)?;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff.to_le_bytes());
    out.extend_from_slice(b"WEBPVP8 ");
    out.extend_from_slice(&vp8.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(partition0);
    out.extend_from_slice(tokens);
    out.resize(total, 0);
    Ok(out)
}

/// Internal opaque checkpoint. Mode decisions and finalized token bytes must
/// already come from the caller; public encoding awaits the real frame loop.
pub fn VP8EncWrite(
    enc: &VP8Encoder,
    width: u32,
    height: u32,
    profile: u8,
    tokens: &[u8],
) -> Result<Vec<u8>, VP8WriteError> {
    PutVP8FrameHeader(width, height, profile, 0)?;
    if (width as usize).div_ceil(16) != enc.mb_w || (height as usize).div_ceil(16) != enc.mb_h {
        return Err(VP8WriteError::InvalidFrame);
    }
    let bw = GeneratePartition0(enc)?;
    AssembleOpaqueWebP(width, height, profile, VP8BitWriterBuf(&bw), tokens)
}
