//! Standalone scalar SharpYUV conversion, independent of the WebP encoder.
//!
//! Direct port of `upstream/sharpyuv/` at
//! `1844042d1ca751e62e61df7617dba19c98588339` (API version 0.4.2).
//! Steps and strides in Rust views are measured in samples, not bytes.
//!
//! ```
//! use libwebpop::sharpyuv::*;
//!
//! let rgb = [255, 0, 0];
//! let input = SharpYuvRgb::U8(SharpYuvRgbView {
//!     r: &rgb, g: &rgb[1..], b: &rgb[2..],
//!     width: 1, height: 1, step: 3, stride: 3,
//! });
//! let matrix = SharpYuvGetConversionMatrix(SharpYuvMatrixType::kSharpYuvMatrixWebp);
//! let yuv = SharpYuvConvert(input, 8, 8, matrix)?;
//! assert!(matches!(yuv, SharpYuvOutput::U8(_)));
//! # Ok::<(), SharpYuvError>(())
//! ```
// Copyright 2022 Google Inc. All Rights Reserved.
// Use of this source code is governed by the BSD-style license in COPYING.
// An additional intellectual property rights grant is in PATENTS.

#[doc(hidden)]
pub mod sharpyuv;
pub mod sharpyuv_csp;
pub mod sharpyuv_dsp;
pub mod sharpyuv_gamma;

pub use sharpyuv::{
    SHARPYUV_VERSION, SharpYuvConvert, SharpYuvConvertWithOptions, SharpYuvGetVersion,
    SharpYuvOptions, SharpYuvOptionsInit, SharpYuvOptionsInitInternal,
};
pub use sharpyuv_csp::*;

/// Errors belonging to the conversion module, independent of encoder errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharpYuvError {
    InvalidDimensions,
    InvalidRgbBitDepth,
    InvalidYuvBitDepth,
    InvalidStorage,
    InvalidStride,
    InputTooShort,
    SizeOverflow,
    AllocationFailed,
    InvalidColorSpace,
    InvalidMatrix,
    InvalidTransferFunction,
    InvalidVersion,
}

impl std::fmt::Display for SharpYuvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDimensions => "invalid SharpYUV image dimensions",
            Self::InvalidRgbBitDepth => "RGB depth must be 8, 10, 12 or 16",
            Self::InvalidYuvBitDepth => "YUV depth must be 8, 10 or 12",
            Self::InvalidStorage => "RGB sample storage does not match its bit depth",
            Self::InvalidStride => "invalid SharpYUV pixel step or row stride",
            Self::InputTooShort => "SharpYUV input channel is too short",
            Self::SizeOverflow => "SharpYUV buffer size overflow",
            Self::AllocationFailed => "SharpYUV allocation failed",
            Self::InvalidColorSpace => "invalid SharpYUV color space",
            Self::InvalidMatrix => "SharpYUV matrix exceeds the supported arithmetic range",
            Self::InvalidTransferFunction => "invalid SharpYUV transfer function",
            Self::InvalidVersion => "incompatible SharpYUV API version",
        })
    }
}

impl std::error::Error for SharpYuvError {}

/// RGB-to-YUV coefficients and offsets in 16-bit fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharpYuvConversionMatrix {
    pub rgb_to_y: [i32; 4],
    pub rgb_to_u: [i32; 4],
    pub rgb_to_v: [i32; 4],
}

macro_rules! transfer_types {
    ($($variant:ident = $value:literal),+ $(,)?) => {
        /// Transfer functions defined by the pinned SharpYUV H.273 enum.
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub enum SharpYuvTransferFunctionType { $($variant = $value),+ }

        impl TryFrom<u32> for SharpYuvTransferFunctionType {
            type Error = SharpYuvError;
            fn try_from(value: u32) -> Result<Self, Self::Error> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(SharpYuvError::InvalidTransferFunction),
                }
            }
        }
    };
}

transfer_types! {
    kSharpYuvTransferFunctionBt709 = 1,
    kSharpYuvTransferFunctionBt470M = 4,
    kSharpYuvTransferFunctionBt470Bg = 5,
    kSharpYuvTransferFunctionBt601 = 6,
    kSharpYuvTransferFunctionSmpte240 = 7,
    kSharpYuvTransferFunctionLinear = 8,
    kSharpYuvTransferFunctionLog100 = 9,
    kSharpYuvTransferFunctionLog100_Sqrt10 = 10,
    kSharpYuvTransferFunctionIec61966 = 11,
    kSharpYuvTransferFunctionBt1361 = 12,
    kSharpYuvTransferFunctionSrgb = 13,
    kSharpYuvTransferFunctionBt2020_10Bit = 14,
    kSharpYuvTransferFunctionBt2020_12Bit = 15,
    kSharpYuvTransferFunctionSmpte2084 = 16,
    kSharpYuvTransferFunctionSmpte428 = 17,
    kSharpYuvTransferFunctionHlg = 18,
}

/// Borrowed channel starts, permitting planar or interleaved RGB layouts.
/// Each channel must reach the last sample; final-row padding is unnecessary.
#[derive(Clone, Copy, Debug)]
pub struct SharpYuvRgbView<'a, T> {
    pub r: &'a [T],
    pub g: &'a [T],
    pub b: &'a [T],
    pub width: u32,
    pub height: u32,
    pub step: usize,
    pub stride: usize,
}

impl<T> SharpYuvRgbView<'_, T> {
    pub(super) fn validate(&self) -> Result<(), SharpYuvError> {
        use SharpYuvError::*;
        if self.width == 0
            || self.height == 0
            || self.width >= i32::MAX as u32
            || self.height >= i32::MAX as u32
        {
            return Err(InvalidDimensions);
        }
        let row = (self.width as usize)
            .checked_mul(self.step)
            .ok_or(SizeOverflow)?;
        if self.step == 0 || self.stride < row {
            return Err(InvalidStride);
        }
        let len = (self.height as usize - 1)
            .checked_mul(self.stride)
            .and_then(|n| n.checked_add(row - self.step))
            .and_then(|n| n.checked_add(1))
            .ok_or(SizeOverflow)?;
        if [self.r.len(), self.g.len(), self.b.len()]
            .iter()
            .any(|&n| n < len)
        {
            return Err(InputTooShort);
        }
        Ok(())
    }
}

/// Storage matches the depth: U8 for 8-bit RGB, U16 for 10/12/16-bit RGB.
#[derive(Clone, Copy, Debug)]
pub enum SharpYuvRgb<'a> {
    U8(SharpYuvRgbView<'a, u8>),
    U16(SharpYuvRgbView<'a, u16>),
}

/// Tightly packed output; chroma dimensions are rounded up independently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharpYuvPlanes<T> {
    pub width: u32,
    pub height: u32,
    pub y: Vec<T>,
    pub u: Vec<T>,
    pub v: Vec<T>,
}

/// Logical samples: U8 for 8-bit YUV, U16 for 10/12-bit YUV.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharpYuvOutput {
    U8(SharpYuvPlanes<u8>),
    U16(SharpYuvPlanes<u16>),
}

pub(super) fn allocate<T: Default + Clone>(len: usize) -> Result<Vec<T>, SharpYuvError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| SharpYuvError::AllocationFailed)?;
    values.resize(len, T::default());
    Ok(values)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sharpyuv_allocation_capacity_overflow_returns_error() {
        assert_eq!(
            super::allocate::<u16>(usize::MAX),
            Err(super::SharpYuvError::AllocationFailed)
        );
    }
}
