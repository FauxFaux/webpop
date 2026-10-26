//! Unsupported APIs exposed for testing and integration.
//!
//! All items in this module may change or disappear in any release, including
//! 0.1.x patch releases. The supported encoding APIs and their compatibility
//! guarantees are documented at the crate root.

// Explicit paths preserve the source layout matching the upstream port.
#[path = "dsp/mod.rs"]
pub mod dsp;
#[path = "enc/mod.rs"]
pub mod enc;
#[path = "mux/mod.rs"]
pub mod mux;
#[path = "sharpyuv/mod.rs"]
pub mod sharpyuv;
#[path = "utils/mod.rs"]
pub mod utils;
#[path = "webp/mod.rs"]
pub mod webp;

pub use crate::encode::{
    LosslessEncodeError, LosslessOptions, LossyOptions, WebPEncodeRGBAWithAlphaQuality,
    WebPEncodeRGBAWithNearLossless, encode_lossless_rgba, encode_lossy_rgba,
    encode_near_lossless_rgba,
};
pub use mux::{
    AnimationContainer, AnimationContainerOptions, AnimationEncoder, AnimationEncodingPolicy,
    AnimationError, AnimationFrameCodec, AnimationFrameOptions, AnimationKeyframePolicy,
    AnimationOptimizerOptions, AnimationOptions, BackgroundColor, ContainerFrameOptions,
    EncodedFrame, FrameBlend, FrameDispose, MuxError,
};
