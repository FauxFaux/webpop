// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/muxinternal.c and muxi.h at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

use super::{EncodedFrame, MuxError};
use crate::WebPEncodingError;
use crate::enc::syntax_enc::{AssembleWebP, PutVP8FrameHeader, WebPSizes};
use crate::enc::vp8l_enc::WriteImageData;

pub(super) const MAX_CHUNK_PAYLOAD: u64 = u32::MAX as u64 - 9;
pub(super) const MAX_FILE_SIZE: u64 = u32::MAX as u64 - 1;

/// Borrowed codec output at the still-writer boundary. No RIFF parsing or
/// fixed-header stripping is used to prepare animation frames.
pub(crate) enum EncodedImage<'a> {
    Vp8l {
        data: &'a [u8],
    }, // Finalized bits, excluding the 0x2f signature.
    Vp8 {
        width: u32,
        height: u32,
        profile: u8,
        partition0: &'a [u8],
        residuals: &'a [u8],
        alpha: Option<&'a [u8]>,
    },
}

impl EncodedImage<'_> {
    pub(crate) fn write_still(self, output: &mut Vec<u8>) -> Result<(), WebPEncodingError> {
        match self {
            Self::Vp8l { data } => Ok(WriteImageData(data, output)?),
            Self::Vp8 {
                width,
                height,
                profile,
                partition0,
                residuals,
                alpha,
            } => Ok(AssembleWebP(
                width, height, profile, partition0, residuals, alpha, output,
            )?),
        }
    }

    pub(crate) fn into_frame(
        self,
        width: u32,
        height: u32,
    ) -> Result<EncodedFrame, WebPEncodingError> {
        let copy = |data: &[u8]| -> Result<Vec<u8>, std::collections::TryReserveError> {
            let mut result = Vec::new();
            result.try_reserve_exact(data.len())?;
            result.extend_from_slice(data);
            Ok(result)
        };
        let (payload, has_alpha) = match self {
            Self::Vp8l { data } => {
                let size = data
                    .len()
                    .checked_add(1)
                    .ok_or(crate::enc::vp8l_enc::ImageWriteError::SizeOverflow)?;
                // Reuse the still writer's checked size calculation.
                crate::enc::vp8l_enc::WriteRiffHeader(data.len())?;
                let mut image = Vec::new();
                image.try_reserve_exact(size)?;
                image.push(0x2f);
                image.extend_from_slice(data);
                (FramePayload::Vp8l(image), data[3] & 0x10 != 0)
            }
            Self::Vp8 {
                width,
                height,
                profile,
                partition0,
                residuals,
                alpha,
            } => {
                let header = PutVP8FrameHeader(width, height, profile, partition0.len())?;
                let (size, _) =
                    WebPSizes(partition0.len(), residuals.len(), alpha.map(<[u8]>::len))?;
                let mut image = Vec::new();
                image.try_reserve_exact(size as usize)?;
                image.extend_from_slice(&header);
                image.extend_from_slice(partition0);
                image.extend_from_slice(residuals);
                // Pinned VP8 encoders include the final zero byte in the chunk
                // length; preserve it when the payload goes through WebPMux.
                image.resize(size as usize, 0);
                let alpha = alpha.map(copy).transpose()?;
                let has_alpha = alpha.is_some();
                (FramePayload::Vp8 { image, alpha }, has_alpha)
            }
        };
        Ok(EncodedFrame {
            width,
            height,
            has_alpha,
            payload,
        })
    }
}

#[derive(Debug)]
pub(super) enum FramePayload {
    Vp8l(Vec<u8>),
    Vp8 {
        image: Vec<u8>,
        alpha: Option<Vec<u8>>,
    },
}

impl FramePayload {
    pub(super) fn disk_size(&self) -> Result<u64, MuxError> {
        match self {
            Self::Vp8l(image) => SizeWithPadding(image.len() as u64),
            Self::Vp8 { image, alpha } => Ok(SizeWithPadding(image.len() as u64)?
                + alpha
                    .as_ref()
                    .map_or(Ok(0), |a| SizeWithPadding(a.len() as u64))?),
        }
    }

    pub(super) fn emit(&self, output: &mut Vec<u8>) {
        match self {
            Self::Vp8l(image) => ChunkEmit(b"VP8L", image, output),
            Self::Vp8 { image, alpha } => {
                if let Some(alpha) = alpha {
                    ChunkEmit(b"ALPH", alpha, output);
                }
                ChunkEmit(b"VP8 ", image, output);
            }
        }
    }

    pub(super) fn has_alpha_chunk(&self) -> bool {
        matches!(self, Self::Vp8 { alpha: Some(_), .. })
    }
}

/// Checked counterpart of muxi.h's inline SizeWithPadding. Wide arithmetic
/// permits synthetic boundary tests on both 32- and 64-bit hosts.
#[inline]
pub(super) fn SizeWithPadding(size: u64) -> Result<u64, MuxError> {
    if size > MAX_CHUNK_PAYLOAD {
        return Err(MuxError::SizeOverflow);
    }
    Ok(8 + size + (size & 1))
}

pub(super) fn checked_file_size(size: u64) -> Result<usize, MuxError> {
    if size > MAX_FILE_SIZE {
        return Err(MuxError::SizeOverflow);
    }
    usize::try_from(size).map_err(|_| MuxError::SizeOverflow)
}

/// Call only after validating all sizes and reserving the complete output.
pub(super) fn ChunkEmit(tag: &[u8; 4], data: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(tag);
    output.extend_from_slice(&(data.len() as u32).to_le_bytes());
    output.extend_from_slice(data);
    if data.len() & 1 != 0 {
        output.push(0);
    }
}

pub(super) fn put_le24(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes()[..3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_mux_synthetic_size_boundaries() {
        assert_eq!(SizeWithPadding(0).unwrap(), 8);
        assert_eq!(SizeWithPadding(1).unwrap(), 10);
        assert_eq!(SizeWithPadding(MAX_CHUNK_PAYLOAD).unwrap(), MAX_FILE_SIZE);
        for size in [MAX_CHUNK_PAYLOAD + 1, u32::MAX as u64, u64::MAX] {
            assert!(matches!(SizeWithPadding(size), Err(MuxError::SizeOverflow)));
        }
        assert_eq!(
            checked_file_size(MAX_FILE_SIZE).unwrap() as u64,
            MAX_FILE_SIZE
        );
        assert!(checked_file_size(MAX_FILE_SIZE + 1).is_err());
        assert!(checked_file_size(u64::MAX).is_err());
        // Nested ANMF includes its 16-byte frame header and all child chunk
        // headers/padding. Test its boundary independently of real buffers.
        let child = SizeWithPadding(MAX_CHUNK_PAYLOAD - 24).unwrap();
        assert_eq!(SizeWithPadding(16 + child).unwrap(), MAX_FILE_SIZE);
        assert!(SizeWithPadding(18 + child).is_err());
        assert!(checked_file_size(12 + MAX_FILE_SIZE).is_err());
    }
}
