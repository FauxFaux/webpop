//! Checked raw RGBA sequences and output containers for encoded rectangles.
//! Both preserve frame order and durations. Pixel/rectangle optimizer leaves
//! are in [`anim_encode`]; optimized sequence encoding is not yet available.

pub mod anim_encode;
mod muxedit;
pub(crate) mod muxinternal;
mod sequence;

pub use sequence::{
    AnimationEncoder, AnimationEncodingPolicy, AnimationError, AnimationFrameOptions,
    AnimationOptions,
};

use crate::{LosslessOptions, LossyOptions, WebPEncodingError};
use muxinternal::FramePayload;
use std::collections::TryReserveError;

#[derive(Debug, thiserror::Error)]
pub enum MuxError {
    #[error("canvas axes must be in 1..=2^24 and area below 2^32")]
    InvalidCanvas,
    #[error("frame origins must be even, below 2^24, and contained in the canvas")]
    InvalidRectangle,
    #[error("frame duration must fit 24 bits")]
    InvalidDuration,
    #[error("an animation container needs at least one frame")]
    NoFrames,
    #[error("container size exceeds WebP or host limits")]
    SizeOverflow,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
}

/// Explicit background channels. Serialized as BGRA in ANIM. Viewers may treat
/// this as a hint; it does not specify the optimizer's internal canvas clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Default for BackgroundColor {
    fn default() -> Self {
        Self {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 255,
        }
    }
}

/// Caller-supplied bytes; no metadata is extracted or interpreted. `None`
/// omits a chunk; `Some(Vec::new())` writes a present, empty chunk.
#[derive(Debug, Default)]
pub struct AnimationContainerOptions {
    /// Zero means infinite looping.
    pub loop_count: u16,
    pub background: BackgroundColor,
    pub icc_profile: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameBlend {
    /// Replace every pixel inside the frame rectangle.
    #[default]
    Replace,
    /// Alpha-composite over the current canvas.
    Blend,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameDispose {
    #[default]
    None,
    /// Clear this rectangle before rendering the next frame.
    Background,
}

/// Controls for an explicitly encoded rectangle. Unlike WebPMuxPushFrame,
/// odd origins are rejected rather than rounded down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerFrameOptions {
    pub x_offset: u32,
    pub y_offset: u32,
    /// Exact stored duration, including zero; no browser delay is invented.
    pub duration_ms: u32,
    pub blend: FrameBlend,
    pub dispose: FrameDispose,
}

/// Opaque codec payload prepared by checked Rust encoding. This API cannot
/// parse or accept an arbitrary WebP container. Source pixels are borrowed only
/// during encoding. Codec settings and fidelity match the corresponding still
/// functions, including near-lossless's intermediate-alpha caveat.
#[derive(Debug)]
pub struct EncodedFrame {
    width: u32,
    height: u32,
    has_alpha: bool,
    payload: FramePayload,
}

impl EncodedFrame {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn has_alpha(&self) -> bool {
        self.has_alpha
    }

    /// Encode lossless RGBA with the still API's strided input contract.
    pub fn encode_lossless_rgba(
        rgba: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        options: &LosslessOptions,
    ) -> Result<Self, WebPEncodingError> {
        Self::encode_near_lossless_rgba(rgba, width, height, stride, 100, options)
    }

    /// Encode near-lossless RGBA with independent strength and effort.
    pub fn encode_near_lossless_rgba(
        rgba: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        near_lossless: i32,
        options: &LosslessOptions,
    ) -> Result<Self, WebPEncodingError> {
        crate::encode::encode_near_lossless_rgba_with(
            rgba,
            width,
            height,
            stride,
            near_lossless,
            options,
            |image| image.into_frame(width, height),
        )
    }

    /// Encode lossy RGBA with method 0–6, including optional ALPH and SharpYUV.
    pub fn encode_lossy_rgba(
        rgba: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        options: &LossyOptions,
    ) -> Result<Self, WebPEncodingError> {
        crate::encode::encode_lossy_rgba_with(rgba, width, height, stride, options, |image| {
            image.into_frame(width, height)
        })
    }
}

/// Output-only builder for prepared encoded rectangles. It owns encoded frames
/// and metadata, without allocating a raw canvas. Canvas axes may exceed the
/// codecs' 16383-pixel frame limit, subject to WebP's canvas area bound.
///
/// One full-canvas frame becomes a still image, retaining metadata but losing
/// playback duration/loop settings, as in pinned WebPMux. A lone subrectangle
/// remains ANMF; no decoding or full-canvas re-encoding is performed.
///
/// ```
/// use libwebpop::{AnimationContainer, AnimationContainerOptions,
///     ContainerFrameOptions, EncodedFrame, LosslessOptions};
/// let mut animation = AnimationContainer::new(1, 1, AnimationContainerOptions::default())?;
/// for rgba in [[255, 0, 0, 255], [0, 0, 255, 255]] {
///     let frame = EncodedFrame::encode_lossless_rgba(&rgba, 1, 1, 4, &LosslessOptions::default())?;
///     animation.push_frame(frame, ContainerFrameOptions { duration_ms: 100, ..Default::default() })?;
/// }
/// let mut webp = Vec::new();
/// animation.assemble(&mut webp)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct AnimationContainer {
    width: u32,
    height: u32,
    options: AnimationContainerOptions,
    frames: Vec<(EncodedFrame, ContainerFrameOptions)>,
}
