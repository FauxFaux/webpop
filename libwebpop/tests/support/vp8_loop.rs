use super::{Case, SEED, random, vp8_tokens::hash};
use libwebpop::internal::{
    enc::{
        analysis_enc::*, filter_enc::*, frame_enc::*, picture_csp_enc::import_rgba_to_yuva,
        picture_enc::WebPYUVAPicture, picture_tools_enc::WebPCleanupTransparentArea,
        syntax_enc::VP8EncWrite, vp8i_enc::*,
    },
    utils::bit_writer_utils::*,
};
pub const STATE_SIZE: usize = 2150;
fn wide(out: &mut Vec<u32>, v: u64) {
    out.extend([v as u32, (v >> 32) as u32]);
}
pub fn state(enc: &VP8Encoder, out: &mut Vec<u32>) {
    out.extend([
        enc.segment_hdr.num_segments as u32,
        u32::from(enc.segment_hdr.update_map),
        enc.segment_hdr.size as u32,
        enc.max_i4_header_bits as u32,
        enc.base_quant as u32,
        enc.dq_y1_dc as u32,
        enc.dq_y2_dc as u32,
        enc.dq_y2_ac as u32,
        enc.dq_uv_dc as u32,
        enc.dq_uv_ac as u32,
        u32::from(enc.filter_hdr.simple),
        enc.filter_hdr.level as u32,
        enc.filter_hdr.sharpness as u32,
        enc.filter_hdr.i4x4_lf_delta as u32,
        u32::from(enc.proba.dirty),
        u32::from(enc.proba.use_skip_proba),
        u32::from(enc.proba.skip_proba),
        enc.proba.nb_skip as u32,
    ]);
    out.extend(enc.proba.segments.map(u32::from));
    out.extend(enc.dqm.iter().flat_map(|s| {
        [
            s.quant as u32,
            s.fstrength as u32,
            s.max_edge as u32,
            u32::from(s.y2.q[1]),
        ]
    }));
    out.extend(super::vp8_cost::probability_words(&enc.proba.coeffs));
    out.extend(super::vp8_cost::stats_words(&enc.proba));
    out.push(hash(super::vp8_cost::costs_words(&enc.proba)));
}
pub fn picture(a: &[u32]) -> WebPYUVAPicture {
    if a[6] != 0 {
        let rgba: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let mut pic = import_rgba_to_yuva(&rgba, a[0], a[1], a[8] as usize).unwrap();
        WebPCleanupTransparentArea(&mut pic);
        pic
    } else {
        let mut planes = vec![a[0], a[1], a[3]];
        planes.extend_from_slice(&a[9..]);
        super::vp8_analysis::picture(&planes)
    }
}
pub fn encode(a: &[u32]) -> (Vec<u32>, Vec<u8>) {
    encode_method(a, 4, false)
}

pub fn encode_method(a: &[u32], method: i32, method_trace: bool) -> (Vec<u32>, Vec<u8>) {
    let quality = f32::from_bits(a[2]);
    let pic = picture(a);
    let mut enc = VP8Encoder::new_with_method(a[0], a[1], quality <= 98., method).unwrap();
    enc.config.quality = quality;
    enc.segment_hdr.num_segments = a[3] as usize;
    enc.segment_hdr.update_map = a[3] > 1;
    enc.max_i4_header_bits = a[4] as i32;
    VP8EncAnalyze(&mut enc, &pic);
    let mut out = Vec::new();
    let mut calls = Vec::new();
    let bw = if !enc.use_tokens {
        VP8EncLoopWithObserver(&mut enc, &pic, quality, u64::from(a[5]), |enc, event| {
            if a[7] == 0 {
                return;
            }
            match event {
                VP8DirectLoopEvent::ScanStart { pass, .. } => {
                    out.extend([0, pass as u32]);
                    state(enc, &mut out);
                }
                VP8DirectLoopEvent::Macroblock {
                    pass,
                    index,
                    score,
                    iterator,
                    ..
                } => {
                    macroblock_words(enc, &mut out, pass, index, score, iterator, true);
                }
                VP8DirectLoopEvent::StatEnd { .. } => {}
            }
        })
        .unwrap()
    } else if a[7] == 0 && u64::from(a[5]) == PARTITION0_SIZE_LIMIT {
        VP8EncTokenLoop(&mut enc, &pic, quality).unwrap()
    } else {
        VP8EncTokenLoopWithObserver(&mut enc, &pic, quality, u64::from(a[5]), |enc, event| {
            if a[7] == 0 {
                return;
            }
            match event {
                VP8TokenLoopEvent::Reconstruct {
                    reconstruction: r, ..
                } => {
                    if method_trace && method >= 5 {
                        calls.extend([
                            u32::from(r.kind),
                            u32::from(r.trellis),
                            r.block as u32,
                            r.mode as u32,
                        ]);
                    }
                }
                VP8TokenLoopEvent::PassStart { pass } => {
                    out.extend([0, pass as u32]);
                    state(enc, &mut out);
                }
                VP8TokenLoopEvent::Refresh { pass, index } => {
                    out.extend([1, pass as u32, index as u32]);
                    state(enc, &mut out);
                }
                VP8TokenLoopEvent::Macroblock {
                    pass,
                    index,
                    score: rd,
                    iterator: it,
                    ..
                } => {
                    if method_trace && method >= 5 {
                        out.extend([6, pass as u32, index as u32, calls.len() as u32 / 4]);
                        out.append(&mut calls);
                    }
                    macroblock_words(enc, &mut out, pass, index, rd, it, method_trace);
                }
                VP8TokenLoopEvent::PassEnd {
                    pass,
                    header_cost,
                    distortion,
                    retry,
                } => {
                    if method_trace {
                        return;
                    }
                    out.extend([3, pass as u32]);
                    wide(&mut out, header_cost);
                    wide(&mut out, distortion);
                    out.push(u32::from(retry));
                }
                VP8TokenLoopEvent::Emit { tokens } => {
                    out.push(4);
                    state(enc, &mut out);
                    out.extend([tokens.len() as u32, hash(tokens.tokens().map(u32::from))]);
                    if enc.mb_info.len() <= 64 {
                        out.extend(tokens.tokens().map(u32::from));
                    }
                }
            }
        })
        .unwrap()
    };
    let tokens = VP8BitWriterBuf(&bw);
    if a[7] != 0 {
        out.push(5);
        state(&enc, &mut out);
        out.push(enc.preds.len() as u32);
        out.extend(enc.preds.iter().map(|&v| u32::from(v)));
        super::vp8_analysis::mbs(&enc, &mut out);
        out.push(tokens.len() as u32);
        out.extend(tokens.iter().map(|&v| u32::from(v)));
    }
    let mut file = Vec::new();
    VP8EncWrite(&enc, a[0], a[1], 0, tokens, &mut file).unwrap();
    if a[7] != 0 {
        out.push(file.len() as u32);
    }
    out.extend(file.iter().copied().map(u32::from));
    (out, file)
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8LoopFrame" => encode(a).0,
        "VP8FrameSegments" => {
            let mut enc = VP8Encoder::new(16, 16, false).unwrap();
            enc.mb_info.resize(a[1] as usize, VP8MBInfo::default());
            enc.mb_w = a[1] as usize;
            enc.segment_hdr.num_segments = a[0] as usize;
            enc.proba.segments = std::array::from_fn(|i| a[2 + i] as u8);
            for (mb, &v) in enc.mb_info.iter_mut().zip(&a[5..]) {
                mb.segment = v as u8;
            }
            SetSegmentProbas(&mut enc);
            let mut out = vec![
                u32::from(enc.segment_hdr.update_map),
                enc.segment_hdr.size as u32,
            ];
            out.extend(enc.proba.segments.map(u32::from));
            out.extend(enc.mb_info.iter().map(|mb| u32::from(mb.segment)));
            out
        }
        "VP8AdjustFilter" => {
            let mut enc = VP8Encoder::new(1, 1, false).unwrap();
            enc.config.filter_strength = a[0] as i32;
            enc.filter_hdr.sharpness = a[1] as i32;
            enc.filter_hdr.level = a[2] as i32;
            for (i, s) in enc.dqm.iter_mut().enumerate() {
                s.max_edge = a[3 + 3 * i] as i32;
                s.y2.q[1] = a[4 + 3 * i] as u16;
                s.fstrength = a[5 + 3 * i] as i32;
            }
            VP8AdjustFilterStrength(&mut enc);
            let mut out = vec![enc.filter_hdr.level as u32];
            out.extend(enc.dqm.iter().map(|s| s.fstrength as u32));
            out
        }
        _ => unreachable!(),
    }
}
pub fn frame_case(w: u32, h: u32, q: f32, segments: u32, pattern: u32, trace: bool) -> Case {
    let mut a = vec![
        w,
        h,
        q.to_bits(),
        segments,
        65536,
        PARTITION0_SIZE_LIMIT as u32,
        0,
        u32::from(trace),
        w * 4,
    ];
    let mut seed = SEED ^ pattern;
    for (plane, pw, ph) in [
        (0, w, h),
        (1, w.div_ceil(2), h.div_ceil(2)),
        (2, w.div_ceil(2), h.div_ceil(2)),
    ] {
        for y in 0..ph {
            for x in 0..pw {
                let r = random(&mut seed);
                a.push(match pattern {
                    0 => 128,
                    1 => (x * 9 + y * 13 + plane * 67) & 255,
                    2 => r & 255,
                    3 => {
                        if (x / 8 + y / 8) % 2 == 0 {
                            16 + plane * 7
                        } else {
                            230 - plane * 7
                        }
                    }
                    4 => {
                        if x < pw / 2 {
                            128
                        } else {
                            r & 255
                        }
                    }
                    _ => {
                        if x == pw / 2 && y == ph / 2 {
                            255
                        } else {
                            0
                        }
                    }
                });
            }
        }
    }
    Case::new("VP8LoopFrame", a)
}
pub fn frame_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 17),
        (17, 1),
        (3, 5),
        (15, 15),
        (16, 16),
        (17, 19),
        (31, 33),
        (65, 49),
    ] {
        for q in [0f32, 30., 75., 98., 98.5, 100.] {
            for p in 0..6 {
                out.push(frame_case(w, h, q, 4, p, true));
            }
        }
    }
    for count in [
        95, 96, 97, 98, 193, 194, 195, 767, 768, 775, 776, 783, 784, 792, 800,
    ] {
        for p in [0, 2, 4] {
            out.push(frame_case(count * 16, 1, 75., 4, p, true));
        }
    }
    for (w, h) in [(16383, 1), (1, 16383), (193, 129)] {
        for q in [0f32, 75., 98.5, 100.] {
            out.push(frame_case(w, h, q, 4, 4, true));
        }
    }
    for segments in 1..=4 {
        for limit in [0, 1, 8192, 65536, PARTITION0_SIZE_LIMIT as u32] {
            let mut c = frame_case(17, 19, 75., segments, 4, true);
            c.input[5] = limit;
            out.push(c);
        }
    }
    for budget in [0, 1, 639, 640, 65536] {
        let mut c = frame_case(33, 17, 98.5, 4, 1, true);
        c.input[4] = budget;
        c.input[5] = 0;
        out.push(c);
    }
    // A retry after a periodic fit must retain those probabilities while
    // clearing statistics, tokens, contexts and diffusion state.
    let mut c = frame_case(97 * 16, 1, 75., 4, 4, true);
    c.input[4] = 1;
    c.input[5] = 0;
    out.push(c);
    out.extend(retry_boundary_cases());
    out
}

pub fn retry_boundary_cases() -> Vec<Case> {
    // First-pass size from pinned C for this exact image/quality: 20,912
    // header units. Preserve strict > comparison at the retry boundary.
    [20911, 20912, 20913]
        .into_iter()
        .map(|limit| {
            let mut c = frame_case(17, 19, 75., 4, 4, true);
            c.input[5] = limit;
            c
        })
        .collect()
}
pub fn rgba_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (17, 19), (33, 17)] {
        for p in 0..6 {
            let mut image = super::public_images::image(w, h, p);
            for y in 0..h {
                for x in 0..w {
                    image.rgba[y * image.stride + 4 * x + 3] = 255;
                }
            }
            for q in [0f32, 1., 30., 50., 75., 98., 98.5, 99., 100.] {
                out.push(rgba_case(&image, q));
            }
        }
    }
    let real = super::public_images::real_image();
    for q in [30f32, 75., 98.5, 100.] {
        out.push(rgba_case(&real, q));
    }
    out
}
pub fn rgba_case(image: &super::ImageCase, q: f32) -> Case {
    let mut a = vec![
        image.width,
        image.height,
        q.to_bits(),
        4,
        65536,
        PARTITION0_SIZE_LIMIT as u32,
        1,
        0,
        image.stride as u32,
    ];
    a.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new("VP8LoopFrame", a)
}
pub fn leaf_cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut seed = SEED;
    for nb in 1..=4 {
        for count in [1, 2, 3, 17, 255, 1024] {
            for pattern in 0..8 {
                let mut a = vec![nb, count, 7, 19, 231];
                a.extend((0..count).map(|i| {
                    if pattern < 4 {
                        pattern
                    } else if pattern == 4 {
                        i % 4
                    } else {
                        random(&mut seed) % 4
                    }
                }));
                out.push(Case::new("VP8FrameSegments", a));
            }
        }
    }
    for sharpness in 0..8 {
        for edge in [0, 1, 2, 7, 15, 63, 64, 255, 2047] {
            for q in [8, 16, 64, 128, 440, 65535] {
                for strength in [0, 60] {
                    let mut a = vec![strength, sharpness, 17];
                    for s in 0..4 {
                        a.extend([edge, q, (s * 17) % 64]);
                    }
                    out.push(Case::new("VP8AdjustFilter", a));
                }
            }
        }
    }
    let mut rare = vec![4, 1024, 7, 19, 231];
    rare.extend([0; 1023]);
    rare.push(1);
    out.push(Case::new("VP8FrameSegments", rare));
    out
}
pub fn cases() -> Vec<Case> {
    leaf_cases()
        .into_iter()
        .chain(frame_cases())
        .chain(rgba_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut out: Vec<_> = leaf_cases()
        .into_iter()
        .step_by(113)
        .chain(
            frame_cases()
                .into_iter()
                .filter(|c| c.input[0] <= 17 && c.input[1] <= 19)
                .step_by(31),
        )
        .chain(
            rgba_cases()
                .into_iter()
                .filter(|c| c.input[0] <= 17)
                .step_by(67),
        )
        .collect();
    out.extend(retry_boundary_cases());
    for count in [96, 97] {
        out.push(frame_case(count * 16, 1, 75., 4, 0, true));
    }
    for count in [1, 97] {
        let mut c = frame_case(count * 16, 1, 75., 4, 4, true);
        c.input[4] = 1;
        c.input[5] = 0;
        out.push(c);
    }
    out
}

fn macroblock_words(
    enc: &VP8Encoder,
    out: &mut Vec<u32>,
    pass: usize,
    index: usize,
    rd: &VP8ModeScore,
    it: &VP8EncIterator,
    selected_only: bool,
) {
    out.extend([2, pass as u32, index as u32]);
    for v in [rd.D, rd.SD, rd.H, rd.R, rd.score] {
        wide(out, v as u64);
    }
    out.push(rd.nz);
    out.push(hash(
        rd.y_dc_levels
            .iter()
            .take(if selected_only && enc.mb_info[index].r#type == 0 {
                0
            } else {
                16
            })
            .chain(rd.y_ac_levels.iter().flatten())
            .chain(rd.uv_levels.iter().flatten())
            .map(|&v| v as i32 as u32),
    ));
    out.push(hash(
        (0..16)
            .flat_map(|y| (0..16).map(move |x| u32::from(it.yuv_out[y * 32 + x])))
            .chain((0..8).flat_map(|y| (16..32).map(move |x| u32::from(it.yuv_out[y * 32 + x])))),
    ));
    let mb = enc.mb_info[index];
    out.extend([
        u32::from(mb.r#type),
        u32::from(mb.uv_mode),
        u32::from(mb.skip),
        u32::from(mb.segment),
        u32::from(mb.alpha),
    ]);
    if selected_only && enc.config.method < 3 && enc.proba.use_skip_proba && mb.skip {
        out.extend([0; 17]);
        out.push(u32::from(it.left_nz[8]));
    } else {
        out.extend(it.top_nz.map(u32::from));
        out.extend(it.left_nz.map(u32::from));
    }
    out.push(enc.nz[it.x + 1]);
    out.extend(it.left_derr.iter().flatten().map(|&v| v as i32 as u32));
    out.extend(
        enc.top_derr
            .as_ref()
            .map_or([[0; 2]; 2], |top| top[it.x])
            .iter()
            .flatten()
            .map(|&v| v as i32 as u32),
    );
}
