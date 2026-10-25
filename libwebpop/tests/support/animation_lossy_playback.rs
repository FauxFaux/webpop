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
