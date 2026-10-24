// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/muxedit.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

use super::muxinternal::{ChunkEmit, SizeWithPadding, checked_file_size, put_le24};
use super::*;

impl AnimationContainer {
    pub fn new(
        width: u32,
        height: u32,
        options: AnimationContainerOptions,
    ) -> Result<Self, MuxError> {
        if !(1..=1 << 24).contains(&width)
            || !(1..=1 << 24).contains(&height)
            || u64::from(width) * u64::from(height) >= 1 << 32
        {
            return Err(MuxError::InvalidCanvas);
        }
        for bytes in [&options.icc_profile, &options.exif, &options.xmp]
            .into_iter()
            .flatten()
        {
            SizeWithPadding(bytes.len() as u64)?;
        }
        Ok(Self {
            width,
            height,
            options,
            frames: Vec::new(),
        })
    }

    /// Validate before changing the builder. No raw source pixels are retained.
    pub fn push_frame(
        &mut self,
        frame: EncodedFrame,
        options: ContainerFrameOptions,
    ) -> Result<(), MuxError> {
        if options.x_offset & 1 != 0
            || options.y_offset & 1 != 0
            || options.x_offset >= 1 << 24
            || options.y_offset >= 1 << 24
            || u64::from(options.x_offset) + u64::from(frame.width) > u64::from(self.width)
            || u64::from(options.y_offset) + u64::from(frame.height) > u64::from(self.height)
        {
            return Err(MuxError::InvalidRectangle);
        }
        if options.duration_ms >= 1 << 24 {
            return Err(MuxError::InvalidDuration);
        }
        SizeWithPadding(16 + frame.payload.disk_size()?)?;
        self.frames.try_reserve(1)?;
        self.frames.push((frame, options));
        Ok(())
    }

    /// Assemble without consuming the builder. Repeated assembly is permitted.
    /// Success replaces `output`, reusing its capacity. Errors preserve its
    /// contents. Frame timing is supplied directly, without timestamp inference.
    pub fn assemble(&self, output: &mut Vec<u8>) -> Result<(), MuxError> {
        WebPMuxAssemble(self, output)
    }
}

fn MuxCleanup(mux: &AnimationContainer) -> bool {
    mux.frames.len() == 1
        && mux.frames[0].0.width == mux.width
        && mux.frames[0].0.height == mux.height
}

fn CreateVP8XChunk(mux: &AnimationContainer, animated: bool) -> u32 {
    let mut flags = u32::from(mux.options.icc_profile.is_some()) * 0x20
        | u32::from(mux.options.exif.is_some()) * 0x08
        | u32::from(mux.options.xmp.is_some()) * 0x04
        | u32::from(animated) * 0x02;
    // A bare lossless still signals alpha only in VP8L. It gets an ALPHA
    // feature bit when metadata or animation requires a VP8X chunk.
    if mux.frames.iter().any(|(f, _)| f.payload.has_alpha_chunk()) {
        flags |= 0x10;
    }
    if flags != 0 && mux.frames.iter().any(|(f, _)| f.has_alpha) {
        flags |= 0x10;
    }
    flags
}

fn WebPMuxAssemble(mux: &AnimationContainer, output: &mut Vec<u8>) -> Result<(), MuxError> {
    if mux.frames.is_empty() {
        return Err(MuxError::NoFrames);
    }
    let animated = !MuxCleanup(mux);
    let flags = CreateVP8XChunk(mux, animated);
    let mut size = 12u64 + if flags != 0 { 18 } else { 0 } + if animated { 14 } else { 0 };
    for bytes in [
        &mux.options.icc_profile,
        &mux.options.exif,
        &mux.options.xmp,
    ]
    .into_iter()
    .flatten()
    {
        size = size
            .checked_add(SizeWithPadding(bytes.len() as u64)?)
            .ok_or(MuxError::SizeOverflow)?;
    }
    for (frame, _) in &mux.frames {
        let image_size = frame.payload.disk_size()?;
        let frame_size = if animated {
            SizeWithPadding(16 + image_size)?
        } else {
            image_size
        };
        size = size.checked_add(frame_size).ok_or(MuxError::SizeOverflow)?;
    }
    let total = checked_file_size(size)?;
    output.try_reserve_exact(total.saturating_sub(output.len()))?;
    output.clear();
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&((size - 8) as u32).to_le_bytes());
    output.extend_from_slice(b"WEBP");
    if flags != 0 {
        output.extend_from_slice(b"VP8X\x0a\0\0\0");
        output.extend_from_slice(&flags.to_le_bytes());
        put_le24(output, mux.width - 1);
        put_le24(output, mux.height - 1);
    }
    if let Some(icc) = &mux.options.icc_profile {
        ChunkEmit(b"ICCP", icc, output);
    }
    if animated {
        let bg = mux.options.background;
        let [lo, hi] = mux.options.loop_count.to_le_bytes();
        ChunkEmit(
            b"ANIM",
            &[bg.blue, bg.green, bg.red, bg.alpha, lo, hi],
            output,
        );
    }
    for (frame, control) in &mux.frames {
        if animated {
            output.extend_from_slice(b"ANMF");
            let image_size = frame
                .payload
                .disk_size()
                .expect("sizes validated before writing");
            output.extend_from_slice(&((16 + image_size) as u32).to_le_bytes());
            put_le24(output, control.x_offset / 2);
            put_le24(output, control.y_offset / 2);
            put_le24(output, frame.width - 1);
            put_le24(output, frame.height - 1);
            put_le24(output, control.duration_ms);
            output.push(
                u8::from(control.dispose == FrameDispose::Background)
                    | (u8::from(control.blend == FrameBlend::Replace) << 1),
            );
        }
        frame.payload.emit(output);
    }
    if let Some(exif) = &mux.options.exif {
        ChunkEmit(b"EXIF", exif, output);
    }
    if let Some(xmp) = &mux.options.xmp {
        ChunkEmit(b"XMP ", xmp, output);
    }
    debug_assert_eq!(output.len(), total);
    Ok(())
}
