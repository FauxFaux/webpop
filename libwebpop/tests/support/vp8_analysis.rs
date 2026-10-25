use super::{Case, SEED, random};
use libwebpop::internal::{
    dsp::enc::*,
    enc::{analysis_enc::*, iterator_enc::*, picture_enc::WebPYUVAPicture, vp8i_enc::*},
};

pub fn seeded(w: u32, h: u32, nb: usize) -> VP8Encoder {
    let mut enc = VP8Encoder::new(w, h, false).unwrap();
    enc.segment_hdr = VP8EncSegmentHeader {
        num_segments: nb,
        update_map: nb > 1,
        size: 37,
    };
    enc.alpha = -13;
    enc.uv_alpha = 27;
    for (i, s) in enc.dqm.iter_mut().enumerate() {
        s.alpha = -60 + i as i32;
        s.beta = 70 + i as i32;
    }
    for mb in &mut enc.mb_info {
        *mb = VP8MBInfo {
            r#type: 0,
            uv_mode: 3,
            skip: true,
            segment: 3,
            alpha: 199,
        };
    }
    for y in 0..enc.mb_h * 4 {
        for x in 0..enc.mb_w * 4 {
            enc.preds[(y + 1) * enc.preds_w + x + 1] = 7;
        }
    }
    enc
}
pub fn segments(enc: &VP8Encoder, out: &mut Vec<u32>) {
    out.extend(enc.dqm.iter().flat_map(|s| [s.alpha as u32, s.beta as u32]));
}
pub fn mbs(enc: &VP8Encoder, out: &mut Vec<u32>) {
    for mb in &enc.mb_info {
        out.extend([
            u32::from(mb.r#type),
            u32::from(mb.uv_mode),
            u32::from(mb.skip),
            u32::from(mb.segment),
            u32::from(mb.alpha),
        ]);
    }
}
pub fn stats(s: &VP8AnalysisStats, out: &mut Vec<u32>) {
    out.extend(s.alphas.iter().map(|&v| v as u32));
    out.extend([
        s.alpha_sum as u32,
        (s.alpha_sum >> 32) as u32,
        s.uv_alpha_sum as u32,
        (s.uv_alpha_sum >> 32) as u32,
    ]);
}
pub fn picture(a: &[u32]) -> WebPYUVAPicture {
    let (w, h) = (a[0] as usize, a[1] as usize);
    let uv = w.div_ceil(2) * h.div_ceil(2);
    let plane = |start: usize, n: usize| a[start..start + n].iter().map(|&v| v as u8).collect();
    WebPYUVAPicture {
        width: a[0],
        height: a[1],
        y: plane(3, w * h),
        u: plane(3 + w * h, uv),
        v: plane(3 + w * h + uv, uv),
        a: None,
    }
}
pub fn probe(a: &[u32]) -> (VP8Encoder, VP8EncIterator) {
    let mut enc = seeded(32, 32, 4);
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(&mut enc, &mut it);
    VP8IteratorSetRow(&mut it, &enc, ((a[0] >> 1) & 1) as usize);
    if a[0] & 1 != 0 {
        VP8IteratorNext(&mut it, &enc);
    }
    it.y_left = std::array::from_fn(|i| a[1 + i] as u8);
    it.uv_left = std::array::from_fn(|i| a[18 + i] as u8);
    for i in 0..16 {
        enc.y_top[16 * it.x + i] = a[43 + i] as u8;
        enc.uv_top[16 * it.x + i] = a[59 + i] as u8;
    }
    it.yuv_in = std::array::from_fn(|i| a[75 + i] as u8);
    it.yuv_p.fill(0xa5);
    (enc, it)
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8Susceptibility" => vec![
            GetAlpha(&VP8Histogram {
                max_value: a[0] as i32,
                last_non_zero: a[1] as i32,
            }) as u32,
            u32::from(FinalAlphaValue(a[2] as i32)),
        ],
        "VP8SegmentAlphas" => {
            let mut enc = seeded(16, 16, a[0] as usize);
            SetSegmentAlphas(
                &mut enc,
                &std::array::from_fn(|i| a[2 + i] as i32),
                a[1] as i32,
            );
            let mut out = Vec::new();
            segments(&enc, &mut out);
            out
        }
        "VP8AssignSegments" => {
            let alphas = std::array::from_fn(|i| a[1 + i] as i32);
            let n = alphas.iter().filter(|&&v| v != 0).count();
            let mut enc = seeded(n as u32 * 16, 16, a[0] as usize);
            for (mb, alpha) in enc
                .mb_info
                .iter_mut()
                .zip((0..256).filter(|&i| alphas[i] != 0))
            {
                mb.alpha = alpha as u8;
            }
            AssignSegments(&mut enc, &alphas);
            let mut out = Vec::new();
            segments(&enc, &mut out);
            mbs(&enc, &mut out);
            out
        }
        "VP8AnalysisProbe" => {
            let (mut enc, mut it) = probe(a);
            let mut out = vec![
                MBAnalyzeBestIntra16Mode(&mut it, &mut enc) as u32,
                MBAnalyzeBestUVMode(&mut it, &mut enc) as u32,
            ];
            let mut s = VP8AnalysisStats::default();
            MBAnalyze(&mut it, &mut enc, &mut s);
            stats(&s, &mut out);
            mbs(&enc, &mut out);
            out.extend(it.yuv_p.into_iter().map(u32::from));
            out.extend(enc.preds.into_iter().map(u32::from));
            out
        }
        "VP8AnalysisFrame" => {
            let pic = picture(a);
            let mut enc = seeded(a[0], a[1], a[2] as usize);
            let mut out = Vec::new();
            let s = if a[2] > 1 {
                DoSegmentsJob(&mut enc, &pic)
            } else {
                VP8AnalysisStats::default()
            };
            stats(&s, &mut out);
            mbs(&enc, &mut out);
            let result = VP8EncAnalyze(&mut enc, &pic);
            assert_eq!(result, s);
            out.extend([
                enc.alpha as u32,
                enc.uv_alpha as u32,
                enc.segment_hdr.num_segments as u32,
                u32::from(enc.segment_hdr.update_map),
                enc.segment_hdr.size as u32,
            ]);
            segments(&enc, &mut out);
            mbs(&enc, &mut out);
            out.extend(enc.preds.into_iter().map(u32::from));
            out
        }
        _ => unreachable!(),
    }
}

pub fn scalar_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for max in 0..=256 {
        for last in 0..=31 {
            cases.push(Case::new(
                "VP8Susceptibility",
                [max, last, if max > 1 { 510 * last / max } else { 0 }],
            ));
        }
    }
    for raw in [-1048576, -1, 0, 1, 254, 255, 256, 7905, 1048576] {
        cases.push(Case::new("VP8Susceptibility", [2, 31, raw as u32]));
    }
    cases
}
pub fn alpha_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for nb in 1..=4 {
        for value in 0..=255 {
            for mid in [value, value + 1] {
                cases.push(Case::new(
                    "VP8SegmentAlphas",
                    [nb, mid, value, value, value, value],
                ));
            }
        }
    }
    for i in 0..1024 {
        let nb = i % 4 + 1;
        let centers: [u32; 4] = std::array::from_fn(|_| random(&mut seed) & 255);
        let min = *centers[..nb].iter().min().unwrap();
        let max = (*centers[..nb].iter().max().unwrap()).max(min + 1);
        cases.push(Case::new(
            "VP8SegmentAlphas",
            [
                nb as u32,
                min + (random(&mut seed) % (max - min + 1)),
                centers[0],
                centers[1],
                centers[2],
                centers[3],
            ],
        ));
    }
    cases
}
pub fn assign_case(nb: u32, hist: [u32; 256]) -> Case {
    Case::new(
        "VP8AssignSegments",
        [nb].into_iter().chain(hist).collect::<Vec<_>>(),
    )
}
pub fn tied_centers_case() -> Case {
    let mut hist = [0; 256];
    hist[0] = 1;
    hist[2] = 7;
    assign_case(4, hist)
}
pub fn assign_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for nb in 1..=4 {
        for alpha in 0..256 {
            for count in [1, 1048576] {
                let mut hist = [0; 256];
                hist[alpha] = count;
                cases.push(assign_case(nb, hist));
            }
        }
        for gap in 1..=255 {
            let mut hist = [0; 256];
            hist[0] = 1;
            hist[gap] = if gap & 1 == 0 { 7 } else { 1 };
            cases.push(assign_case(nb, hist));
        }
        cases.push(assign_case(nb, [1; 256]));
        cases.push(assign_case(nb, [4096; 256]));
    }
    let mut seed = SEED;
    for i in 0..768 {
        let mut hist = std::array::from_fn(|_| {
            let v = random(&mut seed);
            if v & 7 < (i % 8) { 0 } else { (v >> 8) % 512 }
        });
        if hist.iter().all(|&v| v == 0) {
            hist[0] = 1;
        }
        cases.push(assign_case(i % 4 + 1, hist));
    }
    cases
}
pub fn probe_case(flags: u32, pattern: u32) -> Case {
    let mut seed = SEED ^ flags ^ pattern;
    let byte = |i: u32, seed: &mut u32| match pattern {
        0..=255 => pattern,
        256 => i.wrapping_mul(31).wrapping_add(17) & 255,
        257 => 255 * (i & 1),
        _ => random(seed) & 255,
    };
    let mut input = vec![flags];
    input.extend((0..586).map(|i| byte(i, &mut seed)));
    Case::new("VP8AnalysisProbe", input)
}
pub fn probe_cases() -> Vec<Case> {
    (0..4)
        .flat_map(|flags| (0..322).map(move |p| probe_case(flags, p)))
        .collect()
}
pub fn frame_case(w: u32, h: u32, nb: u32, pattern: u32) -> Case {
    let mut seed = SEED ^ w ^ (h << 16) ^ pattern;
    let mut input = vec![w, h, nb];
    for ch in 0..3 {
        let (pw, ph) = if ch == 0 {
            (w, h)
        } else {
            (w.div_ceil(2), h.div_ceil(2))
        };
        for y in 0..ph {
            for x in 0..pw {
                let v = match pattern {
                    0 => 0,
                    1 => 128,
                    2 => (x * 7 + y * 13 + ch * 73) & 255,
                    3 => 255 * ((x + y) & 1),
                    4 => random(&mut seed) & 255,
                    5 => {
                        if x < pw / 2 {
                            23
                        } else {
                            231
                        }
                    }
                    6 => {
                        if x < pw / 2 {
                            37
                        } else {
                            random(&mut seed) & 255
                        }
                    }
                    7 => ((x / 16) * 31 + (y / 16) * 97 + ch * 37) & 255,
                    8 => {
                        if x == pw / 2 && y == ph / 2 {
                            255
                        } else {
                            127
                        }
                    }
                    _ => 255 * u32::from(x == pw - 1 || y == ph - 1),
                };
                input.push(v);
            }
        }
    }
    Case::new("VP8AnalysisFrame", input)
}
pub fn frame_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 17),
        (17, 1),
        (3, 5),
        (7, 9),
        (8, 8),
        (15, 17),
        (16, 16),
        (17, 15),
        (17, 19),
        (31, 33),
        (32, 32),
        (33, 31),
        (65, 49),
    ] {
        for nb in 1..=4 {
            for pattern in 0..10 {
                cases.push(frame_case(w, h, nb, pattern));
            }
        }
    }
    for (w, h) in [(16383, 1), (1, 16383)] {
        for pattern in [2, 4, 6] {
            cases.push(frame_case(w, h, 4, pattern));
        }
    }
    let rgba = super::public_images::real_image();
    let pic = libwebpop::internal::enc::picture_csp_enc::import_rgba_to_yuva(
        &rgba.rgba,
        rgba.width,
        rgba.height,
        rgba.stride,
    )
    .unwrap();
    let mut input = vec![pic.width, pic.height, 4];
    input.extend(pic.y.into_iter().chain(pic.u).chain(pic.v).map(u32::from));
    cases.push(Case::new("VP8AnalysisFrame", input));
    cases
}
pub fn cases() -> Vec<Case> {
    scalar_cases()
        .into_iter()
        .chain(alpha_cases())
        .chain(assign_cases())
        .chain(probe_cases())
        .chain(frame_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = scalar_cases().into_iter().step_by(431).collect();
    cases.extend(alpha_cases().into_iter().step_by(233));
    cases.extend(assign_cases().into_iter().step_by(173));
    cases.push(assign_case(4, [1; 256]));
    cases.push(tied_centers_case());
    for flags in 0..4 {
        for p in [0, 128, 256, 259] {
            cases.push(probe_case(flags, p));
        }
    }
    for (w, h, nb, p) in [
        (1, 1, 4, 0),
        (1, 17, 4, 2),
        (17, 1, 4, 6),
        (17, 19, 4, 6),
        (31, 33, 4, 7),
        (17, 19, 1, 4),
        (32, 32, 2, 4),
    ] {
        cases.push(frame_case(w, h, nb, p));
    }
    cases
}
