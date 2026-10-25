use super::{Case, SEED, random};
use libwebpop::*;

#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub mode: u32,
    pub stride: usize,
    pub control: ContainerFrameOptions,
    pub rgba: Vec<u8>,
    pub still: Vec<u8>,
}

fn lossless_options() -> LosslessOptions {
    LosslessOptions::default()
}
fn lossy_options(mode: u32) -> LossyOptions {
    LossyOptions {
        quality: if mode == 2 { 42.5 } else { 75.0 },
        method: 4,
        alpha_quality: if mode == 2 { 50 } else { 100 },
        use_sharp_yuv: mode == 4,
    }
}

impl Frame {
    pub fn options(&self) -> AnimationFrameOptions {
        match self.mode {
            0 => AnimationFrameOptions::Lossless(lossless_options()),
            3 => AnimationFrameOptions::NearLossless {
                strength: 60,
                options: lossless_options(),
            },
            mode => AnimationFrameOptions::Lossy(lossy_options(mode)),
        }
    }

    pub fn encoded(&self) -> EncodedFrame {
        match self.mode {
            0 => EncodedFrame::encode_lossless_rgba(
                &self.rgba,
                self.width,
                self.height,
                self.stride,
                &lossless_options(),
            ),
            3 => EncodedFrame::encode_near_lossless_rgba(
                &self.rgba,
                self.width,
                self.height,
                self.stride,
                60,
                &lossless_options(),
            ),
            mode => EncodedFrame::encode_lossy_rgba(
                &self.rgba,
                self.width,
                self.height,
                self.stride,
                &lossy_options(mode),
            ),
        }
        .unwrap()
    }
}

pub fn frame(width: u32, height: u32, mode: u32, alpha: bool, seed: u32) -> Frame {
    let stride = width as usize * 4 + 3;
    let mut rgba = vec![0xab; (height as usize - 1) * stride + width as usize * 4];
    let mut state = seed;
    for y in 0..height as usize {
        for x in 0..width as usize {
            let v = random(&mut state);
            rgba[y * stride + x * 4..][..4].copy_from_slice(&[
                v as u8,
                (v >> 8) as u8,
                (v >> 16) as u8,
                if alpha {
                    [0, 1, 127, 254, 255][(x + y) % 5]
                } else {
                    255
                },
            ]);
        }
    }
    let mut still = Vec::new();
    match mode {
        0 => encode_lossless_rgba(
            &rgba,
            width,
            height,
            stride,
            &lossless_options(),
            &mut still,
        ),
        3 => encode_near_lossless_rgba(
            &rgba,
            width,
            height,
            stride,
            60,
            &lossless_options(),
            &mut still,
        ),
        mode => encode_lossy_rgba(
            &rgba,
            width,
            height,
            stride,
            &lossy_options(mode),
            &mut still,
        ),
    }
    .unwrap();
    Frame {
        width,
        height,
        mode,
        stride,
        control: ContainerFrameOptions::default(),
        rgba,
        still,
    }
}

pub fn request(
    width: u32,
    height: u32,
    options: AnimationContainerOptions,
    frames: &[Frame],
) -> Case {
    let bg = options.background;
    let mut a = vec![
        width,
        height,
        u32::from_le_bytes([bg.blue, bg.green, bg.red, bg.alpha]),
        u32::from(options.loop_count),
        0,
    ];
    for (i, meta) in [options.icc_profile, options.exif, options.xmp]
        .into_iter()
        .enumerate()
    {
        if let Some(meta) = meta {
            a[4] |= 1 << i;
            a.push(meta.len() as u32);
            a.extend(meta.into_iter().map(u32::from));
        }
    }
    a.push(frames.len() as u32);
    for f in frames {
        let c = f.control;
        a.extend([
            f.width,
            f.height,
            f.mode,
            f.stride as u32,
            c.x_offset,
            c.y_offset,
            c.duration_ms,
            u32::from(c.dispose == FrameDispose::Background)
                | (u32::from(c.blend == FrameBlend::Replace) << 1),
            f.rgba.len() as u32,
            f.still.len() as u32,
        ]);
        a.extend(f.rgba.iter().copied().map(u32::from));
        a.extend(f.still.iter().copied().map(u32::from));
    }
    Case::new("AnimationMux", a)
}

pub fn parse(case: &Case) -> (u32, u32, AnimationContainerOptions, Vec<Frame>) {
    let a = &case.input;
    let [blue, green, red, alpha] = a[2].to_le_bytes();
    let mut at = 5;
    let mut metadata: [Option<Vec<u8>>; 3] = Default::default();
    for (i, meta) in metadata.iter_mut().enumerate() {
        if a[4] & (1 << i) != 0 {
            let size = a[at] as usize;
            at += 1;
            *meta = Some(a[at..at + size].iter().map(|&v| v as u8).collect());
            at += size;
        }
    }
    let count = a[at];
    at += 1;
    let mut frames = Vec::new();
    for _ in 0..count {
        let f = &a[at..at + 10];
        at += 10;
        let pixels = f[8] as usize;
        let size = f[9] as usize;
        let rgba = a[at..at + pixels].iter().map(|&v| v as u8).collect();
        at += pixels;
        let still = a[at..at + size].iter().map(|&v| v as u8).collect();
        at += size;
        frames.push(Frame {
            width: f[0],
            height: f[1],
            mode: f[2],
            stride: f[3] as usize,
            rgba,
            still,
            control: ContainerFrameOptions {
                x_offset: f[4],
                y_offset: f[5],
                duration_ms: f[6],
                blend: if f[7] & 2 != 0 {
                    FrameBlend::Replace
                } else {
                    FrameBlend::Blend
                },
                dispose: if f[7] & 1 != 0 {
                    FrameDispose::Background
                } else {
                    FrameDispose::None
                },
            },
        });
    }
    assert_eq!(at, a.len());
    let [icc_profile, exif, xmp] = metadata;
    (
        a[0],
        a[1],
        AnimationContainerOptions {
            loop_count: a[3] as u16,
            background: BackgroundColor {
                red,
                green,
                blue,
                alpha,
            },
            icc_profile,
            exif,
            xmp,
        },
        frames,
    )
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let (w, h, options, frames) = parse(case);
    let mut mux = AnimationContainer::new(w, h, options).unwrap();
    for f in frames {
        mux.push_frame(f.encoded(), f.control).unwrap();
    }
    let mut output = vec![0xab; 3];
    mux.assemble(&mut output).unwrap();
    output.into_iter().map(u32::from).collect()
}

fn options(mask: u32) -> AnimationContainerOptions {
    AnimationContainerOptions {
        loop_count: if mask & 1 == 0 { 0 } else { 65535 },
        background: BackgroundColor {
            red: 0x12,
            green: 0x34,
            blue: 0x56,
            alpha: 0x78,
        },
        icc_profile: (mask & 1 != 0).then(|| vec![0, 0xff, 0x80]),
        exif: (mask & 2 != 0).then(|| vec![1, 2, 3, 4]),
        xmp: (mask & 4 != 0).then(|| vec![]),
    }
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // Every metadata combination, VP8L implicit alpha, VP8/ALPH, near-lossless,
    // SharpYUV, full-canvas cleanup and retained single subrectangle.
    for mode in 0..5 {
        for alpha in [false, true] {
            for mask in [0, 7] {
                let mut f = frame(3, 5, mode, alpha, SEED);
                f.control.duration_ms = 0x00ff_ffff;
                cases.push(request(3, 5, options(mask), &[f]));
            }
        }
    }
    for mask in 0..8 {
        let mut a = frame(1, 1, 0, true, SEED);
        a.control = ContainerFrameOptions {
            x_offset: 2,
            y_offset: 4,
            duration_ms: 0,
            blend: FrameBlend::Blend,
            dispose: FrameDispose::Background,
        };
        let mut b = frame(3, 1, 2, true, SEED + 1);
        b.control.duration_ms = 0x00ff_ffff;
        cases.push(request(5, 5, options(mask), &[a, b]));
    }
    let mut a = frame(1, 1, 0, true, SEED);
    a.control.x_offset = 2;
    cases.push(request(4, 2, options(0), &[a]));
    // Max axes and offsets without allocating a full-size raw canvas.
    for (w, h, x, y) in [
        (1 << 24, 1, (1 << 24) - 2, 0),
        (1, 1 << 24, 0, (1 << 24) - 2),
        (65535, 65537, 65534, 65536),
    ] {
        let mut a = frame(1, 1, 0, false, SEED);
        a.control.x_offset = x;
        a.control.y_offset = y;
        cases.push(request(w, h, options(7), &[a]));
    }
    // A longer sequence, repeats and zero durations remain separate frames.
    let frames: Vec<_> = (0..32)
        .map(|i| {
            let mut f = frame(1, 1, 0, true, SEED);
            f.control.duration_ms = i % 3;
            f
        })
        .collect();
    cases.push(request(1, 1, options(0), &frames));
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for i in 0..160u32 {
        let mode = i % 5;
        let mut a = frame(1 + i % 7, 1 + i / 7 % 6, mode, i & 1 != 0, SEED + i);
        a.control = ContainerFrameOptions {
            x_offset: 2 * (i % 3),
            y_offset: 2 * (i / 3 % 3),
            duration_ms: i * 137,
            blend: if i & 1 != 0 {
                FrameBlend::Blend
            } else {
                FrameBlend::Replace
            },
            dispose: if i & 2 != 0 {
                FrameDispose::Background
            } else {
                FrameDispose::None
            },
        };
        let mut b = frame(5, 7, (mode + 1) % 5, i & 2 != 0, SEED + i + 1);
        b.control.duration_ms = if i & 4 == 0 { 0 } else { 0x00ff_ffff };
        cases.push(request(16, 16, options(i % 8), &[a, b]));
    }
    cases
}

pub fn single_frame_case() -> Case {
    let mut f = frame(1, 1, 0, false, SEED);
    f.control.x_offset = 2;
    f.control.y_offset = 2;
    f.control.duration_ms = 100;
    let mut case = request(4, 4, options(0), &[f]);
    case.function = "AnimationSingleFrame".into();
    case
}
