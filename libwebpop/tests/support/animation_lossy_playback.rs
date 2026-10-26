//! Test-only rendering from independently decoded VP8 rectangles. The carryover
//! optimizer's original-word canvases are deliberately not used for playback.
use libwebpop::internal::{ContainerFrameOptions, FrameBlend, FrameDispose};

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub control: ContainerFrameOptions,
    pub still: Vec<u8>,
}

// Match the pinned decoder's straight-alpha integer convention, including its
// reciprocal truncation. Replacement/opaque/transparent cases avoid rounding.
fn blend(source: &[u8], target: &mut [u8]) {
    if source[3] == 0 {
        return;
    }
    if source[3] == 255 {
        target.copy_from_slice(source);
        return;
    }
    let src_a = u64::from(source[3]);
    let dst_a = (u64::from(target[3]) * (256 - src_a)) / 256;
    let alpha = src_a + dst_a;
    let scale = (1u64 << 24) / alpha;
    for i in 0..3 {
        target[i] =
            (((u64::from(source[i]) * src_a + u64::from(target[i]) * dst_a) * scale) >> 24) as u8;
    }
    target[3] = alpha as u8;
}

pub fn render(width: u32, height: u32, frames: &[Frame]) -> Vec<u32> {
    let decoded: Vec<_> = frames
        .iter()
        .map(|f| {
            image::load_from_memory_with_format(&f.still, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8()
        })
        .collect();
    render_decoded(
        width,
        height,
        frames,
        &decoded
            .iter()
            .map(|p| p.as_raw().as_slice())
            .collect::<Vec<_>>(),
    )
}

fn render_decoded(width: u32, height: u32, frames: &[Frame], decoded: &[&[u8]]) -> Vec<u32> {
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    let mut result = vec![width, height, frames.len() as u32, 0];
    let mut previous_key = false;
    let full = |f: &Frame| f.width == width && f.height == height;
    let contains = |f: &Frame, x: u32, y: u32| {
        let c = &f.control;
        x >= c.x_offset && x < c.x_offset + f.width && y >= c.y_offset && y < c.y_offset + f.height
    };
    let mut timestamp = 0;
    for (index, f) in frames.iter().enumerate() {
        assert_eq!(decoded[index].len(), (f.width * f.height * 4) as usize);
        let previous = index.checked_sub(1).map(|i| &frames[i]);
        let background = previous.is_some_and(|p| p.control.dispose == FrameDispose::Background);
        let key = index == 0
            || ((!f.has_alpha || f.control.blend == FrameBlend::Replace) && full(f))
            || (background && (previous_key || full(previous.unwrap())));
        if key {
            canvas.fill(0);
        } else if background {
            let p = previous.unwrap();
            for y in p.control.y_offset..p.control.y_offset + p.height {
                for x in p.control.x_offset..p.control.x_offset + p.width {
                    canvas[((y * width + x) * 4) as usize..][..4].fill(0);
                }
            }
        }
        for dy in 0..f.height {
            for dx in 0..f.width {
                let (x, y) = (f.control.x_offset + dx, f.control.y_offset + dy);
                let source = &decoded[index][((dy * f.width + dx) * 4) as usize..][..4];
                let target = &mut canvas[((y * width + x) * 4) as usize..][..4];
                if key
                    || f.control.blend == FrameBlend::Replace
                    || (background && contains(previous.unwrap(), x, y))
                {
                    target.copy_from_slice(source);
                } else {
                    blend(source, target);
                }
            }
        }
        timestamp += f.control.duration_ms;
        result.push(timestamp);
        for p in canvas.chunks_exact(4) {
            result.extend(if p[3] == 0 {
                [0; 4]
            } else {
                [p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32]
            });
        }
        previous_key = key;
    }
    result
}

pub fn le24(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}
pub fn chunks(mut bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert!(bytes.len() >= 8 + size + (size & 1));
        if size & 1 != 0 {
            assert_eq!(bytes[8 + size], 0);
        }
        result.push((&bytes[..4], &bytes[8..8 + size]));
        bytes = &bytes[8 + size + (size & 1)..];
    }
    result
}

// Parse the container independently, decode each still, and composite with the
// established exact straight-alpha compositor. No original-word cache is used.
pub fn playback(case: &super::Case, bytes: &[u8]) -> Option<(super::Case, Vec<u32>)> {
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    let top = chunks(&bytes[12..]);
    let (w, h) = (case.input[0], case.input[1]);
    let mut frames = Vec::new();
    for (_, data) in top.iter().filter(|c| c.0 == b"ANMF") {
        assert_eq!(data[15] & !3, 0);
        let (fw, fh) = (le24(&data[6..]) + 1, le24(&data[9..]) + 1);
        let nested = chunks(&data[16..]);
        let has_alpha = nested
            .iter()
            .any(|(tag, d)| *tag == b"ALPH" || (*tag == b"VP8L" && d[4] & 0x10 != 0));
        let mut payload = Vec::new();
        if nested[0].0 == b"ALPH" {
            payload.extend_from_slice(b"VP8X\x0a\0\0\0\x10\0\0\0");
            payload.extend_from_slice(&(fw - 1).to_le_bytes()[..3]);
            payload.extend_from_slice(&(fh - 1).to_le_bytes()[..3]);
        }
        payload.extend_from_slice(&data[16..]);
        let mut still = b"RIFF".to_vec();
        still.extend_from_slice(&(4 + payload.len() as u32).to_le_bytes());
        still.extend_from_slice(b"WEBP");
        still.extend(payload);
        let control = ContainerFrameOptions {
            x_offset: le24(data) * 2,
            y_offset: le24(&data[3..]) * 2,
            duration_ms: le24(&data[12..]),
            dispose: if data[15] & 1 != 0 {
                FrameDispose::Background
            } else {
                FrameDispose::None
            },
            blend: if data[15] & 2 != 0 {
                FrameBlend::Replace
            } else {
                FrameBlend::Blend
            },
        };
        assert!(control.x_offset + fw <= w && control.y_offset + fh <= h);
        frames.push(Frame {
            width: fw,
            height: fh,
            has_alpha,
            control,
            still,
        });
    }
    if frames.is_empty() {
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (w, h));
        return None;
    }
    let total: u32 = super::animation_codec_cache::frames(case)
        .iter()
        .map(|f| f.duration)
        .sum();
    assert_eq!(
        frames.iter().map(|f| f.control.duration_ms).sum::<u32>(),
        total
    );
    let mut observations = render(w, h, &frames);
    observations[3] = if case.input[5] != 0 { 65535 } else { 0 };
    // Walk piecewise intervals, never expand the potentially huge timeline.
    let mut input_start = 0;
    for f in super::animation_codec_cache::frames(case) {
        let input_end = input_start + f.duration;
        let mut output_start = 0;
        let mut covered = 0;
        for encoded in &frames {
            let output_end = output_start + encoded.control.duration_ms;
            covered += input_end
                .min(output_end)
                .saturating_sub(input_start.max(output_start));
            output_start = output_end;
        }
        assert_eq!(covered, f.duration);
        input_start = input_end;
    }
    let mut request = vec![bytes.len() as u32];
    request.extend(bytes.iter().map(|&v| u32::from(v)));
    Some((super::Case::new("AnimationDecode", request), observations))
}
