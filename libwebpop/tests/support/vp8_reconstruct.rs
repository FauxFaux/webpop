use super::{Case, SEED, random};
use libwebpop::enc::{iterator_enc::*, quant_enc::*, vp8i_enc::*};

pub const SCORE_WORDS: usize = 435;
pub const PIXELS: usize = 1 + SCORE_WORDS;
pub fn seed_score() -> VP8ModeScore {
    VP8ModeScore {
        D: -123,
        SD: 456,
        H: 789,
        R: -987,
        score: (1i64 << 40) + 3,
        nz: 0x13579bdf,
        mode_i16: 3,
        mode_uv: 2,
        y_dc_levels: [-1234; 16],
        y_ac_levels: [[234; 16]; 16],
        uv_levels: [[-345; 16]; 8],
        modes_i4: std::array::from_fn(|i| (i % 10) as u8),
        derr: [[-7, -6, -5], [-4, -3, -2]],
    }
}
pub fn score(rd: &VP8ModeScore, out: &mut Vec<u32>) {
    for v in [rd.D, rd.SD, rd.H, rd.R, rd.score] {
        out.extend([v as u32, (v >> 32) as u32]);
    }
    out.extend(rd.y_dc_levels.map(|v| v as i32 as u32));
    out.extend(
        rd.y_ac_levels
            .as_flattened()
            .iter()
            .map(|&v| v as i32 as u32),
    );
    out.extend(rd.uv_levels.as_flattened().iter().map(|&v| v as i32 as u32));
    out.push(rd.mode_i16 as u32);
    out.extend(rd.modes_i4.map(u32::from));
    out.extend([rd.mode_uv as u32, rd.nz]);
    out.extend(rd.derr.as_flattened().iter().map(|&v| v as i32 as u32));
}
pub fn errors(it: &VP8EncIterator, top: &[DError], out: &mut Vec<u32>) {
    out.extend(top.iter().flatten().flatten().map(|&v| v as i32 as u32));
    out.extend(it.left_derr.as_flattened().iter().map(|&v| v as i32 as u32));
}
pub fn uv_matrix(q: u32) -> VP8Matrix {
    let mut m = VP8Matrix::default();
    m.q[0] = q as u16;
    m.q[1] = 4;
    ExpandMatrix(&mut m, 2);
    m
}
pub fn dc_state(a: &[u32]) -> (VP8Encoder, VP8EncIterator, [[i16; 16]; 8]) {
    let mut enc = VP8Encoder::new(32, 16, true).unwrap();
    let mut it = VP8EncIterator {
        x: 1,
        ..Default::default()
    };
    enc.top_derr.as_mut().unwrap()[0] = [[11, 12], [13, 14]];
    enc.top_derr.as_mut().unwrap()[1] =
        std::array::from_fn(|ch| std::array::from_fn(|k| a[1 + 2 * ch + k] as i32 as i8));
    it.left_derr =
        std::array::from_fn(|ch| std::array::from_fn(|k| a[5 + 2 * ch + k] as i32 as i8));
    let tmp = std::array::from_fn(|i| std::array::from_fn(|j| a[9 + 16 * i + j] as i16));
    (enc, it, tmp)
}
pub fn initialized(a: &[u32]) -> (VP8Encoder, VP8EncIterator) {
    initialized_with_method(a, 4)
}
pub fn initialized_with_method(a: &[u32], method: i32) -> (VP8Encoder, VP8EncIterator) {
    let mut enc = VP8Encoder::new(32, 32, a[3] & 4 != 0).unwrap();
    enc.dq_uv_dc = a[6] as i32;
    enc.dq_uv_ac = a[7] as i32;
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(&mut enc, &mut it);
    VP8IteratorSetRow(&mut it, &enc, ((a[3] >> 1) & 1) as usize);
    if a[3] & 1 != 0 {
        VP8IteratorNext(&mut it, &enc);
    }
    for mb in &mut enc.mb_info {
        mb.segment = a[2] as u8;
        mb.alpha = 199;
    }
    for (i, s) in enc.dqm.iter_mut().enumerate() {
        s.quant = ((a[1] + 17 * i as u32) % 128) as i32;
    }
    enc.config.method = method;
    SetupMatrices(&mut enc);
    for (i, s) in enc.dqm.iter_mut().enumerate() {
        s.max_edge = 1 + i as i32;
    }
    it.y_left = std::array::from_fn(|i| a[16 + i] as u8);
    it.uv_left = std::array::from_fn(|i| a[33 + i] as u8);
    for i in 0..32 {
        enc.y_top[i] = a[58 + i] as u8;
        enc.uv_top[i] = a[90 + i] as u8;
    }
    it.yuv_in = std::array::from_fn(|i| a[122 + i] as u8);
    it.yuv_p.fill(0xa5);
    enc.nz.copy_from_slice(&[0x123456, 0xabcdef, 0x765432]);
    it.top_nz = std::array::from_fn(|i| 2 + i as u8);
    it.left_nz = std::array::from_fn(|i| 3 + i as u8);
    it.left_derr =
        std::array::from_fn(|ch| std::array::from_fn(|k| a[12 + 2 * ch + k] as i32 as i8));
    if let Some(top) = &mut enc.top_derr {
        for (x, t) in top.iter_mut().enumerate() {
            *t = std::array::from_fn(|ch| std::array::from_fn(|k| (11 + 4 * x + 2 * ch + k) as i8));
        }
        top[it.x] =
            std::array::from_fn(|ch| std::array::from_fn(|k| a[8 + 2 * ch + k] as i32 as i8));
    }
    (enc, it)
}
pub fn run(
    a: &[u32],
    enc: &mut VP8Encoder,
    it: &mut VP8EncIterator,
    rd: &mut VP8ModeScore,
    dst: &mut [u8],
) -> u32 {
    run_with_observer(a, enc, it, rd, dst, &mut |_, _| {})
}
pub fn run_with_observer(
    a: &[u32],
    enc: &mut VP8Encoder,
    it: &mut VP8EncIterator,
    rd: &mut VP8ModeScore,
    dst: &mut [u8],
    observe: &mut impl FnMut(&VP8Encoder, VP8Reconstruction),
) -> u32 {
    match a[0] {
        0 => {
            VP8MakeLuma16Preds(it, enc);
            let nz = ReconstructIntra16WithObserver(it, enc, rd, dst, a[4] as usize, observe);
            StoreMaxDelta(&mut enc.dqm[a[2] as usize], &rd.y_dc_levels);
            nz
        }
        2 => {
            VP8MakeChroma8Preds(it, enc);
            ReconstructUVWithObserver(it, enc, rd, &mut dst[U_OFF_ENC..], a[4] as usize, observe)
        }
        1 => {
            VP8IteratorStartI4(it, enc);
            for _ in 0..a[5] {
                VP8IteratorRotateI4(it, dst);
            }
            MakeIntra4Preds(it);
            let offset = VP8Scan[it.i4];
            u32::from(ReconstructIntra4WithObserver(
                it,
                enc,
                &mut rd.y_ac_levels[it.i4],
                &it.yuv_in[offset..],
                &mut dst[offset..],
                a[4] as usize,
                observe,
            ))
        }
        3 => {
            VP8IteratorStartI4(it, enc);
            let mut nz = 0;
            for i in 0..16 {
                MakeIntra4Preds(it);
                let offset = VP8Scan[i];
                nz |= u32::from(ReconstructIntra4WithObserver(
                    it,
                    enc,
                    &mut rd.y_ac_levels[i],
                    &it.yuv_in[offset..],
                    &mut dst[offset..],
                    (a[4] as usize + i) % 10,
                    observe,
                )) << i;
                VP8IteratorRotateI4(it, dst);
            }
            nz
        }
        _ => unreachable!(),
    }
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8QuantizeSingle" => {
            let mut v = a[1] as i16;
            let e = QuantizeSingle(&mut v, &uv_matrix(a[0]));
            vec![v as i32 as u32, e as u32]
        }
        "VP8DCError" => {
            let (mut enc, mut it, mut tmp) = dc_state(a);
            let mut rd = seed_score();
            CorrectDCValues(&it, &enc, &uv_matrix(a[0]), &mut tmp, &mut rd);
            let mut out: Vec<_> = tmp
                .as_flattened()
                .iter()
                .map(|&v| v as i32 as u32)
                .collect();
            score(&rd, &mut out);
            errors(&it, enc.top_derr.as_ref().unwrap(), &mut out);
            StoreDiffusionErrors(&mut it, &mut enc, &rd);
            errors(&it, enc.top_derr.as_ref().unwrap(), &mut out);
            out
        }
        "VP8Reconstruct" => evaluate_with_method(a, 4),
        _ => unreachable!(),
    }
}
pub fn scalar_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in 4i32..=132 {
        for v in (-2 * q..=2 * q).chain([-32768, -32767, -8193, -4096, 4096, 8193, 32766, 32767]) {
            cases.push(Case::new("VP8QuantizeSingle", vec![q as u32, v as u32]));
        }
    }
    for v in -32768i32..=32767 {
        cases.push(Case::new("VP8QuantizeSingle", vec![132, v as u32]));
    }
    cases
}
pub fn dc_cases() -> Vec<Case> {
    let mut seed = SEED;
    let mut cases = Vec::new();
    for q in 4..=132 {
        for pattern in 0..8 {
            let mut a = vec![q];
            a.extend((0..8).map(|i| {
                (match pattern {
                    0 => 0,
                    1 => 127,
                    2 => -128,
                    3 => {
                        if i % 2 == 0 {
                            -128
                        } else {
                            127
                        }
                    }
                    _ => random(&mut seed) as i8 as i32,
                }) as u32
            }));
            a.extend((0..128).map(|i| {
                (match pattern {
                    0 => 0,
                    1 => 32767,
                    2 => -32768,
                    3 => {
                        if i % 2 == 0 {
                            -2048
                        } else {
                            2040
                        }
                    }
                    4 => (i / 16) as i32 * 71 - 250,
                    _ => random(&mut seed) as i16 as i32,
                }) as u32
            }));
            cases.push(Case::new("VP8DCError", a));
        }
    }
    cases
}
pub fn request(kind: u32, q: u32, flags: u32, mode: u32, pattern: u32) -> Case {
    let mut seed = SEED ^ q ^ (flags << 8) ^ (mode << 16) ^ (kind << 24) ^ pattern;
    let mut a = vec![
        kind,
        q,
        (q + mode) % 4,
        flags,
        mode,
        (q + mode) % 16,
        (-4i32) as u32,
        6,
    ];
    a.extend((0..8).map(|i| {
        (if pattern == 0 {
            0
        } else if pattern == 1 {
            (i as i32 * 37 - 128).clamp(-128, 127)
        } else {
            random(&mut seed) as i8 as i32
        }) as u32
    }));
    a.extend((0..618).map(|i| match pattern {
        0 => 128,
        1 => {
            if i % 2 == 0 {
                0
            } else {
                255
            }
        }
        2 => (i * 37 + i / 32 * 71) % 256,
        3 => {
            if i < 106 {
                0
            } else {
                255
            }
        }
        4 => {
            if i < 106 {
                255
            } else {
                0
            }
        }
        5 => {
            if i == 110 {
                255
            } else {
                0
            }
        }
        _ => random(&mut seed) % 256,
    }));
    Case::new("VP8Reconstruct", a)
}
pub fn reconstruction_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in 0..128 {
        for kind in 0..3 {
            for mode in 0..if kind == 1 { 10 } else { 4 } {
                for pattern in [0, 6] {
                    cases.push(request(kind, q, q % 8, mode, pattern));
                }
            }
        }
        for pattern in [0, 6] {
            cases.push(request(3, q, q % 8, q % 10, pattern));
        }
    }
    for flags in 0..8 {
        for kind in 0..3 {
            for mode in 0..if kind == 1 { 10 } else { 4 } {
                for pattern in 0..8 {
                    cases.push(request(
                        kind,
                        (flags * 17 + pattern * 13) % 128,
                        flags,
                        mode,
                        pattern,
                    ));
                }
            }
        }
    }
    cases
}
pub fn cases() -> Vec<Case> {
    let mut cases = scalar_cases();
    cases.extend(dc_cases());
    cases.extend(reconstruction_cases());
    cases
}
pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in [4, 17, 132] {
        for v in [-32768i32, -267, -1, 0, 1, 264, 32767] {
            cases.push(Case::new("VP8QuantizeSingle", vec![q, v as u32]));
        }
    }
    cases.extend(dc_cases().into_iter().step_by(147));
    for kind in 0..4 {
        for flags in [0, 3, 4, 7] {
            cases.push(request(
                kind,
                (31 * flags) % 128,
                flags,
                if kind == 1 { 9 } else { 3 },
                6,
            ));
        }
    }
    cases
}

pub fn evaluate_with_method(a: &[u32], method: i32) -> Vec<u32> {
    let (mut enc, mut it) = initialized_with_method(a, method);
    let mut rd = seed_score();
    let mut dst = [0xa5; 512];
    let mut calls = Vec::new();
    if method >= 5 {
        it.do_trellis = true;
        libwebpop::enc::cost_enc::VP8CalculateLevelCosts(&mut enc.proba);
    }
    let mask = run_with_observer(a, &mut enc, &mut it, &mut rd, &mut dst, &mut |_, r| {
        calls.extend([
            u32::from(r.kind),
            u32::from(r.trellis),
            r.block as u32,
            r.mode as u32,
        ]);
    });
    let mut out = vec![mask];
    score(&rd, &mut out);
    out.extend(dst.map(u32::from));
    out.extend(it.yuv_p.map(u32::from));
    // C retains its unused local storage even without a top_derr pointer.
    let mut dummy = [[[11, 12], [13, 14]], [[15, 16], [17, 18]]];
    dummy[it.x] = std::array::from_fn(|ch| std::array::from_fn(|k| a[8 + 2 * ch + k] as i32 as i8));
    errors(&it, enc.top_derr.as_deref().unwrap_or(&dummy), &mut out);
    if a[0] == 2 && enc.top_derr.is_some() {
        StoreDiffusionErrors(&mut it, &mut enc, &rd);
    }
    errors(&it, enc.top_derr.as_deref().unwrap_or(&dummy), &mut out);
    out.extend(it.top_nz.map(u32::from));
    out.extend(it.left_nz.map(u32::from));
    out.extend([it.i4 as u32, it.i4_top as u32]);
    out.extend(it.i4_boundary.map(u32::from));
    super::vp8_analysis::mbs(&enc, &mut out);
    out.extend(enc.preds.iter().map(|&v| u32::from(v)));
    out.extend(enc.nz);
    out.extend(enc.dqm.iter().map(|s| s.max_edge as u32));
    if method >= 5 {
        out.push(calls.len() as u32 / 4);
        out.extend(calls);
    }
    out
}
