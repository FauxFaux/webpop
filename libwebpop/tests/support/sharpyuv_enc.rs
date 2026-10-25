use super::{Case, ImageCase, lossy};
use libwebpop::internal::{LossyOptions, encode_lossy_rgba};

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let import = case.function == "YuvaSharpImport";
    let mut storage = vec![0x5a];
    storage.extend(a[if import { 4 } else { 7 }..].iter().map(|&v| v as u8));
    let before = storage.clone();
    let out = if import {
        use libwebpop::internal::enc::{
            picture_csp_enc::*, picture_tools_enc::WebPCleanupTransparentArea,
        };
        let mut pic = import_rgba_to_yuva_sharp(&storage[1..], a[0], a[1], a[2] as usize).unwrap();
        if a[3] != 0 {
            WebPCleanupTransparentArea(&mut pic);
        }
        let mut out = vec![
            u32::from(has_yuva_transparency(&pic)),
            u32::from(pic.a.is_some()),
        ];
        out.extend(
            pic.y
                .iter()
                .chain(&pic.u)
                .chain(&pic.v)
                .chain(pic.a.iter().flatten())
                .map(|&v| u32::from(v)),
        );
        out
    } else {
        let mut encoded = Vec::new();
        encode_lossy_rgba(
            &storage[1..],
            a[0],
            a[1],
            a[2] as usize,
            &LossyOptions {
                quality: f32::from_bits(a[3]),
                method: 4,
                alpha_quality: a[4] as i32,
                use_sharp_yuv: a[5] != 0 || a[6] & 4 != 0,
            },
            &mut encoded,
        )
        .unwrap();
        encoded.into_iter().map(u32::from).collect()
    };
    assert_eq!(before, storage);
    out
}

pub fn request(image: &ImageCase, quality: f32, alpha_quality: i32, sharp: bool, pre: u32) -> Case {
    let mut input = vec![
        image.width,
        image.height,
        image.stride as u32,
        quality.to_bits(),
        alpha_quality as u32,
        u32::from(sharp),
        pre,
    ];
    input.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new("WebPEncodeLossyOptions", input)
}

pub fn import_request(image: &ImageCase, cleanup: bool) -> Case {
    let mut input = vec![
        image.width,
        image.height,
        image.stride as u32,
        u32::from(cleanup),
    ];
    input.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new("YuvaSharpImport", input)
}

pub fn public_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 17),
        (3, 5),
        (4, 3),
        (4, 4),
        (5, 7),
        (16, 17),
        (17, 19),
    ] {
        for pattern in 0..6 {
            let image = lossy::image(w, h, pattern);
            for q in [0., 75., 98.5, 100.] {
                for aq in [0, 30, 100] {
                    for (sharp, pre) in [(false, 0), (true, 0), (false, 4), (true, 4)] {
                        cases.push(request(&image, q, aq, sharp, pre));
                    }
                }
            }
        }
    }
    let mut real = super::public_images::real_image();
    for alpha in [false, true] {
        if alpha {
            for (i, p) in real.rgba.chunks_exact_mut(4).enumerate() {
                p[3] = (i * 17) as u8;
            }
        }
        for q in [30., 75., 98.5] {
            for aq in [0, 100] {
                cases.push(request(&real, q, aq, true, 0));
            }
        }
    }
    for (w, h) in [(16383, 1), (1, 16383)] {
        cases.push(request(&lossy::image(w, h, 4), 75., 100, true, 0));
    }
    cases
}

pub fn import_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for w in [1, 3, 4, 5, 8, 17] {
        for h in [1, 3, 4, 5, 9, 16] {
            for pattern in 0..6 {
                for cleanup in [false, true] {
                    cases.push(import_request(&lossy::image(w, h, pattern), cleanup));
                }
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(3, 5), (4, 3), (4, 4), (5, 7), (17, 19)] {
        for pattern in [0, 2, 4] {
            let image = lossy::image(w, h, pattern);
            cases.push(import_request(&image, false));
            cases.push(import_request(&image, true));
            cases.push(request(&image, 75., 100, true, 0));
            cases.push(request(&image, 98.5, 30, false, 4));
        }
    }
    cases
}

pub fn check_decoded(case: &Case, output: &[u32]) {
    let mut equivalent = case.clone();
    equivalent.function = "WebPEncodeRGBAWithAlphaQuality".into();
    equivalent.input.drain(5..7);
    lossy::check_decoded(&equivalent, output);
}
