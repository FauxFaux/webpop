//! Isolated VP8 EncodeCandidate/filter integration; no lossy optimizer yet.
use super::{Case, ImageCase, lossy};
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;

pub fn request(image: &ImageCase, options: LossyOptions, blending: bool) -> Case {
    let stride = image.width + 3;
    let mut input = vec![
        image.width,
        image.height,
        stride,
        2,
        4,
        options.quality.to_bits(),
        options.method,
        options.alpha_quality as u32,
        u32::from(options.use_sharp_yuv),
        u32::from(blending),
    ];
    for y in 0..image.height as usize {
        for pixel in image.rgba[y * image.stride..][..image.width as usize * 4].chunks_exact(4) {
            input.push(u32::from_be_bytes([pixel[3], pixel[0], pixel[1], pixel[2]]));
        }
        if y + 1 < image.height as usize {
            input.extend([0xdeadbeef; 3]);
        }
    }
    Case::new("AnimationLossyCandidate", input)
}

pub fn options(case: &Case) -> LossyOptions {
    let a = &case.input;
    LossyOptions {
        quality: f32::from_bits(a[5]),
        method: a[6],
        alpha_quality: a[7] as i32,
        use_sharp_yuv: a[8] != 0,
    }
}

pub fn rgba(case: &Case) -> Vec<u8> {
    let a = &case.input;
    let view = ArgbView::new(&a[10..], a[0] as i32, a[1] as i32, a[2] as usize).unwrap();
    (0..a[1] as usize)
        .flat_map(|y| {
            view.row(y)
                .unwrap()
                .iter()
                .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8])
        })
        .collect()
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let before = a.clone();
    let candidate = encode_lossy_candidate(
        ArgbView::new(&a[10..], a[0] as i32, a[1] as i32, a[2] as usize).unwrap(),
        FrameRectangle {
            x_offset: a[3] as i32,
            y_offset: a[4] as i32,
            width: a[0] as i32,
            height: a[1] as i32,
        },
        &options(case),
        a[9] != 0,
    )
    .unwrap();
    assert_eq!(*a, before);
    let mut result = vec![
        candidate.rect.x_offset as u32,
        candidate.rect.y_offset as u32,
        candidate.rect.width as u32,
        candidate.rect.height as u32,
        u32::from(candidate.control.blend == FrameBlend::Replace),
        candidate.control.duration_ms,
        u32::from(candidate.control.dispose == FrameDispose::Background),
    ];
    assert_eq!(candidate.frame.width(), a[0]);
    assert_eq!(candidate.frame.height(), a[1]);
    let size = candidate.encoded_size();
    let mut mux = AnimationContainer::new(a[0], a[1], Default::default()).unwrap();
    mux.push_frame(candidate.frame, Default::default()).unwrap();
    let mut file = Vec::new();
    mux.assemble(&mut file).unwrap();
    assert_eq!(file.len(), size);
    result.push(file.len() as u32);
    result.extend(file.into_iter().map(u32::from));
    result
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in 0..=6 {
        for (w, h, pattern, quality, alpha_quality, use_sharp_yuv) in [
            (1, 1, 0, 0., 100, false),
            (3, 5, 3, 98.5, 71, true),
            (17, 19, 0, 75., 100, false),
            (17, 19, 4, 30.25, 99, true),
        ] {
            for blending in [false, true] {
                cases.push(request(
                    &lossy::image(w, h, pattern),
                    LossyOptions {
                        quality,
                        method,
                        alpha_quality,
                        use_sharp_yuv,
                    },
                    blending,
                ));
            }
        }
    }
    cases
}

pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 17),
        (17, 1),
        (3, 5),
        (4, 4),
        (15, 16),
        (17, 19),
        (33, 17),
    ] {
        for pattern in 0..6 {
            let image = lossy::image(w, h, pattern);
            for method in 0..=6 {
                for (quality, alpha_quality) in [(0., 0), (75., 71), (98.5, 99), (100., 100)] {
                    for use_sharp_yuv in [false, true] {
                        for blending in [false, true] {
                            cases.push(request(
                                &image,
                                LossyOptions {
                                    quality,
                                    method,
                                    alpha_quality,
                                    use_sharp_yuv,
                                },
                                blending,
                            ));
                        }
                    }
                }
            }
        }
    }
    cases
}
