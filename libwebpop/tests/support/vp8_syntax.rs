use super::{Case, SEED, random};
use libwebpop::internal::{
    dsp::cost::SetResidualCoeffs_C,
    enc::{cost_enc::*, syntax_enc::*, token_enc::*, tree_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};

pub fn initialized(a: &[u32]) -> VP8Encoder {
    let mut enc = VP8Encoder::new(a[1], a[2], false).unwrap();
    enc.segment_hdr.num_segments = a[4] as usize;
    enc.segment_hdr.update_map = a[5] != 0;
    enc.filter_hdr = VP8EncFilterHeader {
        simple: a[6] != 0,
        level: a[7] as i32,
        sharpness: a[8] as i32,
        i4x4_lf_delta: a[9] as i32,
    };
    enc.base_quant = a[10] as i32;
    enc.dq_y1_dc = a[11] as i32;
    enc.dq_y2_dc = a[12] as i32;
    enc.dq_y2_ac = a[13] as i32;
    enc.dq_uv_dc = a[14] as i32;
    enc.dq_uv_ac = a[15] as i32;
    enc.proba.segments = std::array::from_fn(|i| a[16 + i] as u8);
    enc.proba.use_skip_proba = a[19] != 0;
    enc.proba.skip_proba = a[20] as u8;
    for (i, p) in enc
        .proba
        .coeffs
        .iter_mut()
        .flatten()
        .flatten()
        .flatten()
        .enumerate()
    {
        *p = match a[21] {
            0 => *p,
            1 => 0,
            2 => 255,
            seed => (i as u32 * 37 + seed) as u8,
        };
    }
    for (i, s) in enc.dqm.iter_mut().enumerate() {
        s.quant = a[22 + 2 * i] as i32;
        s.fstrength = a[23 + 2 * i] as i32;
    }
    let at = 31 + enc.preds.len();
    for (p, &v) in enc.preds.iter_mut().zip(&a[31..at]) {
        *p = v as u8;
    }
    for (mb, v) in enc.mb_info.iter_mut().zip(a[at..].chunks_exact(4)) {
        *mb = VP8MBInfo {
            r#type: v[0] as u8,
            uv_mode: v[1] as u8,
            skip: v[2] != 0,
            segment: v[3] as u8,
            alpha: 0,
        };
    }
    enc
}
fn finish(bw: &mut VP8BitWriter) -> Vec<u32> {
    let pos = VP8BitWriterPos(bw);
    let mut out = vec![pos as u32, (pos >> 32) as u32];
    VP8PutBits(bw, 0x5ad, 11).unwrap();
    out.extend(
        VP8BitWriterFinish(bw)
            .unwrap()
            .iter()
            .copied()
            .map(u32::from),
    );
    out
}
pub fn fixed_image(w: u32, h: u32, profile: u8) -> Vec<u8> {
    let mut enc = VP8Encoder::new(w, h, false).unwrap();
    enc.segment_hdr.num_segments = 1;
    enc.segment_hdr.update_map = false;
    let mut tokens = VP8TBufferInit(0);
    let mut bw = VP8BitWriterInit(0).unwrap();
    let zeros = [0; 16];
    for mb in &mut enc.mb_info {
        mb.r#type = 1;
        mb.skip = true;
        for (first, ty, count) in [(0, 1, 1), (1, 0, 16), (0, 2, 8)] {
            let mut res = VP8InitResidual(first, ty);
            SetResidualCoeffs_C(&zeros, &mut res);
            for _ in 0..count {
                VP8RecordCoeffTokens(0, &res, &mut tokens, &mut enc.proba).unwrap();
            }
        }
    }
    let probabilities: Vec<_> = enc
        .proba
        .coeffs
        .iter()
        .flatten()
        .flatten()
        .flatten()
        .copied()
        .collect();
    VP8EmitTokens(&mut tokens, &mut bw, &probabilities, true).unwrap();
    let bytes = VP8BitWriterFinish(&mut bw).unwrap();
    let mut output = Vec::new();
    VP8EncWrite(&enc, w, h, profile, bytes, &mut output).unwrap();
    output
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if case.function == "VP8FixedImage" {
        return fixed_image(a[0], a[1], a[2] as u8)
            .into_iter()
            .map(u32::from)
            .collect();
    }
    if case.function == "VP8ModeSyntax" {
        let mut bw = VP8BitWriterInit(0).unwrap();
        let mode = a[1] as u8;
        match a[0] {
            0 => PutI4Mode(&mut bw, mode, &kBModesProba[a[2] as usize][a[3] as usize]).unwrap(),
            1 => PutI16Mode(&mut bw, mode).unwrap(),
            2 => PutUVMode(&mut bw, mode).unwrap(),
            3 => PutSegment(&mut bw, mode, &std::array::from_fn(|i| a[4 + i] as u8)).unwrap(),
            _ => unreachable!(),
        }
        return finish(&mut bw);
    }
    let enc = initialized(a);
    if a[0] == 4 {
        return VP8BitWriterBuf(&GeneratePartition0(&enc).unwrap())
            .iter()
            .copied()
            .map(u32::from)
            .collect();
    }
    if a[0] == 5 {
        let tokens: Vec<_> = (0..a[30]).map(|i| (i * 73 + 19) as u8).collect();
        let mut output = Vec::new();
        VP8EncWrite(&enc, a[1], a[2], a[3] as u8, &tokens, &mut output).unwrap();
        return output.into_iter().map(u32::from).collect();
    }
    let mut bw = VP8BitWriterInit(0).unwrap();
    match a[0] {
        0 => PutSegmentHeader(&mut bw, &enc).unwrap(),
        1 => PutFilterHeader(&mut bw, &enc.filter_hdr).unwrap(),
        2 => PutQuant(&mut bw, &enc).unwrap(),
        3 => VP8CodeIntraModes(&mut bw, &enc).unwrap(),
        _ => unreachable!(),
    }
    finish(&mut bw)
}

pub fn mode_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for top in 0..10 {
        for left in 0..10 {
            for mode in 0..10 {
                cases.push(Case::new(
                    "VP8ModeSyntax",
                    vec![0, mode, top, left, 0, 0, 0],
                ));
            }
        }
    }
    for kind in 1..=2 {
        for mode in 0..4 {
            cases.push(Case::new("VP8ModeSyntax", vec![kind, mode, 0, 0, 0, 0, 0]));
        }
    }
    for p in [[0, 0, 0], [255, 255, 255], [1, 128, 254], [23, 45, 67]] {
        for mode in 0..4 {
            cases.push(Case::new(
                "VP8ModeSyntax",
                vec![3, mode, 0, 0, p[0], p[1], p[2]],
            ));
        }
    }
    cases
}
pub fn syntax_case(op: u32, w: u32, h: u32, seed: &mut u32) -> Case {
    let r = random(seed);
    let mut a = vec![
        op,
        w,
        h,
        r % 4,
        1 + r % 4,
        (r >> 4) & 1,
        (r >> 5) & 1,
        (r >> 6) & 63,
        (r >> 12) & 7,
        ((r >> 15) % 127).wrapping_sub(63),
        (r >> 22) & 127,
    ];
    a.extend((0..5).map(|_| (random(seed) % 31).wrapping_sub(15)));
    a.extend((0..3).map(|_| [0, 1, 128, 254, 255][(random(seed) % 5) as usize]));
    a.extend([(r >> 29) & 1, (r >> 16) & 255, r % 5]);
    for _ in 0..4 {
        a.extend([random(seed) % 128, random(seed) % 64]);
    }
    a.push(r % 33);
    let mw = w.div_ceil(16) as usize;
    let mh = h.div_ceil(16) as usize;
    let stride = 4 * mw + 1;
    a.extend((0..stride * (4 * mh + 1)).map(|_| random(seed) % 10));
    for y in 0..mh {
        for x in 0..mw {
            let ty = random(seed) % 2;
            if ty != 0 {
                let mode = random(seed) % 4;
                for row in 0..4 {
                    for col in 0..4 {
                        a[31 + (4 * y + 1 + row) * stride + 4 * x + 1 + col] = mode;
                    }
                }
            }
            a.extend([ty, random(seed) % 4, random(seed) % 2, random(seed) % 4]);
        }
    }
    Case::new("VP8Syntax", a)
}
pub fn syntax_cases() -> Vec<Case> {
    let mut seed = SEED;
    let mut out = Vec::new();
    for i in 0..384 {
        let (w, h) = [(1, 1), (1, 33), (33, 1), (17, 19), (33, 49), (65, 65)][i % 6];
        for op in 0..6 {
            out.push(syntax_case(op, w, h, &mut seed));
        }
    }
    // Exhaust each field's domain in isolation as well as the mixed headers.
    for (op, field, low, high) in [
        (2, 10, 0, 127),
        (1, 9, -63, 63),
        (0, 22, -127, 127),
        (0, 23, -63, 63),
    ] {
        for v in low..=high {
            let mut case = syntax_case(op, 1, 1, &mut seed);
            case.input[field] = v as u32;
            case.input[4] = 4;
            out.push(case);
        }
    }
    for field in 11..16 {
        for delta in -15i32..=15 {
            let mut case = syntax_case(2, 1, 1, &mut seed);
            case.input[field] = delta as u32;
            out.push(case);
        }
    }
    out
}
pub fn fixed_cases() -> Vec<Case> {
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
        (16383, 1),
        (1, 16383),
    ] {
        for profile in 0..4 {
            out.push(Case::new("VP8FixedImage", vec![w, h, profile]));
        }
    }
    out
}
pub fn cases() -> Vec<Case> {
    mode_cases()
        .into_iter()
        .chain(syntax_cases())
        .chain(fixed_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    mode_cases()
        .into_iter()
        .step_by(47)
        .chain(
            syntax_cases()
                .into_iter()
                .enumerate()
                .filter(|(i, c)| i % 67 < 6 && c.input[1] <= 33 && c.input[2] <= 49)
                .map(|(_, c)| c),
        )
        .chain(
            fixed_cases()
                .into_iter()
                .filter(|c| c.input[0] < 100 && c.input[1] < 100)
                .step_by(5),
        )
        .collect()
}
