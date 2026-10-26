//! GIF import uses image's full-canvas disposal/compositing implementation.

use std::io::Cursor;

use image::{AnimationDecoder, ImageDecoder};
use libwebpop::{OpAnimEncoder, OpAnimOptions, OpConfig, OpImage};

use crate::Error;

pub struct EncodedGif {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub frames: usize,
}

fn loop_count(mut bytes: &[u8]) -> Result<u16, Error> {
    use gif::streaming_decoder::{Block, Decoded, OutputBuffer, StreamingDecoder};

    // Observe extension events throughout the file. The high-level decoder's
    // repeat accessor only processes application extensions before frame one.
    // OutputBuffer::None skips compressed pixels without LZW decoding.
    let mut decoder = StreamingDecoder::new();
    let mut count = 1;
    let mut application = None;
    loop {
        if bytes.is_empty() {
            return Err("GIF ended before its trailer".into());
        }
        let (consumed, event) = decoder.update(bytes, &mut OutputBuffer::None)?;
        bytes = &bytes[consumed..];
        match event {
            Decoded::BlockStart(Block::Trailer) => return Ok(count),
            Decoded::SubBlock { ext, is_last } if ext == gif::AnyExtension(0xff) => {
                let data = decoder.last_ext_sub_block();
                match application {
                    None => {
                        application = Some(data == b"NETSCAPE2.0" || data == b"ANIMEXTS1.0");
                    }
                    Some(true) => {
                        if let [1, lo, hi] = *data {
                            let repeats = u16::from_le_bytes([lo, hi]);
                            // Zero is infinite. Finite GIF repeats exclude the
                            // first play; WebP counts it. Clamp like gif2webp.
                            count = if repeats == 0 {
                                0
                            } else {
                                repeats.saturating_add(1)
                            };
                        }
                        application = Some(false);
                    }
                    Some(false) => {}
                }
                if is_last {
                    application = None;
                }
            }
            _ => {}
        }
    }
}

pub fn encode_gif(bytes: &[u8], config: &OpConfig, noalpha: bool) -> Result<EncodedGif, Error> {
    // image's loop_count maps absent repetition to infinity and does not
    // convert GIF repeats to WebP total plays. Read the original metadata,
    // including extensions after the first image, without LZW decoding.
    let mut options = OpAnimOptions::default();
    options.loop_count = loop_count(bytes)?;

    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(image::Limits::default())?;
    let (width, height) = decoder.dimensions();
    let mut encoder = OpAnimEncoder::new(width, height, options)?;
    let mut frames = 0;
    // This iterator composites offsets, transparency and all disposal modes
    // into complete display canvases. Do not collect all raw frames in memory.
    for frame in decoder.into_frames() {
        let frame = frame.map_err(|e| format!("GIF frame {frames}: {e}"))?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        if denominator != 1 {
            return Err(format!("GIF frame {frames}: non-integral millisecond delay").into());
        }
        let mut rgba = frame.into_buffer();
        if noalpha {
            for pixel in rgba.pixels_mut() {
                pixel.0[3] = 255;
            }
        }
        encoder.add(
            OpImage::Rgba {
                pixels: rgba.as_raw(),
                width: rgba.width(),
                height: rgba.height(),
                stride: rgba.width() as usize * 4,
            },
            numerator,
            config,
        )?;
        frames += 1;
    }
    Ok(EncodedGif {
        data: encoder.finish()?,
        width,
        height,
        frames,
    })
}
