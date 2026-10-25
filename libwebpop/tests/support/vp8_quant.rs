use super::{Case, SEED, random};
use libwebpop::internal::enc::{
    config_enc::VP8EncoderConfig, filter_enc::*, quant_enc::*, vp8i_enc::*,
};

pub fn matrix(m: &VP8Matrix, out: &mut Vec<u32>) {
    out.extend(m.q.map(u32::from));
    out.extend(m.iq.map(u32::from));
    out.extend(m.bias);
    out.extend(m.zthresh);
    out.extend(m.sharpen.map(u32::from));
}
pub fn seed_matrix(seed: u16) -> VP8Matrix {
    VP8Matrix {
        q: std::array::from_fn(|i| 4 + seed + i as u16),
        iq: std::array::from_fn(|i| 500 + seed + i as u16),
        bias: std::array::from_fn(|i| 70000 + u32::from(seed) + i as u32),
        zthresh: std::array::from_fn(|i| 200 + u32::from(seed) + i as u32),
        sharpen: std::array::from_fn(|i| 40 + seed + i as u16),
    }
}
pub fn seeded(a: &[u32]) -> VP8Encoder {
    let mut enc = VP8Encoder::new(128, 16, false).unwrap();
    enc.config = VP8EncoderConfig {
        quality: 75.0,
        method: a[2] as i32,
        sns_strength: a[3] as i32,
        filter_strength: a[4] as i32,
        filter_sharpness: a[5] as i32,
        filter_type: a[6] as i32,
    };
    enc.segment_hdr = VP8EncSegmentHeader {
        num_segments: a[1] as usize,
        update_map: true,
        size: 37,
    };
    enc.filter_hdr = VP8EncFilterHeader {
        simple: true,
        level: 19,
        sharpness: a[7] as i32,
        i4x4_lf_delta: -3,
    };
    enc.uv_alpha = a[8] as i32;
    enc.base_quant = 91;
    enc.dq_y1_dc = a[10] as i32;
    enc.dq_y2_dc = a[11] as i32;
    enc.dq_y2_ac = a[12] as i32;
    enc.dq_uv_dc = a[13] as i32;
    enc.dq_uv_ac = a[14] as i32;
    enc.dqm = std::array::from_fn(|i| VP8SegmentInfo {
        y1: seed_matrix(30 * i as u16),
        y2: seed_matrix(30 * i as u16 + 10),
        uv: seed_matrix(30 * i as u16 + 20),
        alpha: a[15 + 4 * i] as i32,
        beta: a[16 + 4 * i] as i32,
        quant: a[17 + 4 * i] as i32,
        fstrength: a[18 + 4 * i] as i32,
        max_edge: 101 + i as i32,
        min_disto: 201 + i as i32,
        lambda_i16: 301 + i as i32,
        lambda_i4: 401 + i as i32,
        lambda_uv: 501 + i as i32,
        lambda_mode: 601 + i as i32,
        tlambda: 701 + i as i32,
        lambda_trellis_i16: 801 + i as i32,
        lambda_trellis_i4: 901 + i as i32,
        lambda_trellis_uv: 1001 + i as i32,
        i4_penalty: (1i64 << 35) + i as i64,
    });
    for (i, mb) in enc.mb_info.iter_mut().enumerate() {
        *mb = VP8MBInfo {
            r#type: (i % 2) as u8,
            uv_mode: (i % 4) as u8,
            skip: (i / 2) % 2 != 0,
            segment: (i % a[1] as usize) as u8,
            alpha: 200 + i as u8,
        };
    }
    enc
}
pub fn run(a: &[u32], enc: &mut VP8Encoder) {
    match a[0] {
        0 => SetupMatrices(enc),
        1 => SetupFilterStrength(enc),
        2 => SimplifySegments(enc),
        3 => VP8SetSegmentParams(enc, f32::from_bits(a[9])),
        4 => {
            VP8SetSegmentParams(enc, f32::from_bits(a[9]));
            VP8SetSegmentParams(enc, f32::from_bits(a[31]));
        }
        _ => unreachable!(),
    }
}
pub fn state(enc: &VP8Encoder) -> Vec<u32> {
    let mut out = vec![
        enc.segment_hdr.num_segments as u32,
        enc.segment_hdr.update_map as u32,
        enc.segment_hdr.size as u32,
        enc.base_quant as u32,
        enc.dq_y1_dc as u32,
        enc.dq_y2_dc as u32,
        enc.dq_y2_ac as u32,
        enc.dq_uv_dc as u32,
        enc.dq_uv_ac as u32,
        enc.filter_hdr.simple as u32,
        enc.filter_hdr.level as u32,
        enc.filter_hdr.sharpness as u32,
        enc.filter_hdr.i4x4_lf_delta as u32,
    ];
    for s in &enc.dqm {
        matrix(&s.y1, &mut out);
        matrix(&s.y2, &mut out);
        matrix(&s.uv, &mut out);
        out.extend(
            [
                s.alpha,
                s.beta,
                s.quant,
                s.fstrength,
                s.max_edge,
                s.min_disto,
                s.lambda_i16,
                s.lambda_i4,
                s.lambda_uv,
                s.lambda_mode,
                s.tlambda,
                s.lambda_trellis_i16,
                s.lambda_trellis_i4,
                s.lambda_trellis_uv,
            ]
            .map(|v| v as u32),
        );
        out.extend([s.i4_penalty as u32, (s.i4_penalty >> 32) as u32]);
    }
    super::vp8_analysis::mbs(enc, &mut out);
    out
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8AnalysisQuant" => {
            let mut enc = super::vp8_analysis::seeded(a[0], a[1], a[2] as usize);
            let mut planes = a[..3].to_vec();
            planes.extend_from_slice(&a[4..]);
            let pic = super::vp8_analysis::picture(&planes);
            libwebpop::internal::enc::analysis_enc::VP8EncAnalyze(&mut enc, &pic);
            VP8SetSegmentParams(&mut enc, f32::from_bits(a[3]));
            let mut out = state(&enc);
            out.extend([enc.alpha as u32, enc.uv_alpha as u32]);
            out.extend(enc.preds.iter().map(|&v| u32::from(v)));
            out
        }
        "VP8FilterStrengthFromDelta" => {
            vec![VP8FilterStrengthFromDelta(a[0] as i32, a[1] as i32) as u32]
        }
        "VP8ExpandMatrix" => {
            let mut m = seed_matrix(9);
            m.q[0] = a[1] as u16;
            m.q[1] = a[2] as u16;
            let mut out = vec![ExpandMatrix(&mut m, a[0] as usize) as u32];
            matrix(&m, &mut out);
            out
        }
        "VP8QuantSetup" => {
            let mut enc = seeded(a);
            run(a, &mut enc);
            state(&enc)
        }
        _ => unreachable!(),
    }
}
pub fn setup(op: u32, nb: u32, quality: f32) -> Vec<u32> {
    let mut a = vec![
        op,
        nb,
        4,
        50,
        60,
        0,
        1,
        0,
        64,
        quality.to_bits(),
        0,
        0,
        0,
        0,
        0,
    ];
    for i in 0..4 {
        a.extend([
            (-127 + 84 * i) as u32,
            (85 * i) as u32,
            (31 * i) as u32,
            (15 * i) as u32,
        ]);
    }
    a.push(quality.to_bits());
    a
}
pub fn matrix_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for t in 0..3 {
        for q in 4..=440 {
            cases.push(Case::new("VP8ExpandMatrix", vec![t, 4, q]));
            cases.push(Case::new("VP8ExpandMatrix", vec![t, q, 440]));
        }
    }
    let mut seed = SEED;
    for _ in 0..768 {
        cases.push(Case::new(
            "VP8ExpandMatrix",
            vec![
                random(&mut seed) % 3,
                4 + random(&mut seed) % 437,
                4 + random(&mut seed) % 437,
            ],
        ));
    }
    cases
}
pub fn setup_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    let mut push = |a| cases.push(Case::new("VP8QuantSetup", a));
    for q in 0..128 {
        for k in 0..12 {
            let mut a = setup(0, 1 + k % 4, 75.);
            a[2] = [0, 3, 4, 6][k as usize % 4];
            a[3] = [0, 50, 100][k as usize / 4];
            for (j, d) in a[10..15].iter_mut().enumerate() {
                *d = [-15i32, -4, 0, 6, 15][(j + k as usize) % 5] as u32;
            }
            for i in 0..4 {
                a[17 + 4 * i] = (q + 31 * i as u32) % 128;
            }
            push(a);
        }
        for k in 0..16 {
            let mut a = setup(1, 1 + k % 4, 75.);
            a[4] = [0, 1, 50, 60, 100][k as usize % 5];
            a[5] = (k + 3) % 8;
            a[6] = k % 2;
            a[7] = k % 8;
            for i in 0..4 {
                a[16 + 4 * i] = [0, 1, 127, 255][(i + k as usize) % 4];
                a[17 + 4 * i] = (q + 31 * i as u32) % 128;
            }
            push(a);
        }
    }
    // Exhaust equivalence patterns over four (quant, strength) pairs.
    for nb in 1..=4 {
        for mut code in 0..4u32.pow(nb) {
            let mut a = setup(2, nb, 75.);
            for i in 0..nb as usize {
                let symbol = code % 4;
                code /= 4;
                a[17 + 4 * i] = 20 + symbol / 2;
                a[18 + 4 * i] = 10 + symbol % 2;
            }
            push(a);
        }
    }
    for q in 0..=400 {
        for sns in [0, 50, 100] {
            let mut a = setup(3, 1 + q % 4, q as f32 / 4.);
            a[3] = sns;
            a[4] = q % 101;
            a[5] = q % 8;
            a[7] = (q + 3) % 8;
            a[8] = q % 256;
            push(a);
        }
    }
    for alpha in -127i32..=127 {
        for quality in [1., 25., 74.999, 75., 75.001, 99.9] {
            let mut a = setup(3, 4, quality);
            a[3] = 100;
            for i in 0..4 {
                a[15 + 4 * i] = alpha as u32;
                a[16 + 4 * i] = 0;
            }
            push(a);
        }
    }
    for uv in (0..256).chain([256, 510, 1024, 7905]) {
        for sns in [0, 24, 25, 49, 50, 74, 75, 99, 100] {
            let mut a = setup(3, 4, 68.125);
            a[3] = sns;
            a[8] = uv;
            push(a);
        }
    }
    for quality in [0f32, f32::from_bits(1), 1., 50., 75., 100.] {
        for bits in [
            quality.to_bits().saturating_sub(1),
            quality.to_bits(),
            quality.to_bits() + 1,
        ] {
            let q = f32::from_bits(bits);
            if q > 100. {
                continue;
            }
            let mut a = setup(3, 4, q);
            a[3] = 100;
            push(a);
        }
    }
    // Invert the quality curve at each integer-quantizer boundary, then test
    // adjacent representable f32 values on both sides of the transition.
    for quant in 0..127 {
        let compression = 1. - f64::from(quant) / 127.;
        let linear = compression * compression * compression;
        let quality = (if linear < 0.5 {
            150. * linear
        } else {
            50. * (linear + 1.)
        }) as f32;
        for bits in [
            quality.to_bits() - 1,
            quality.to_bits(),
            quality.to_bits() + 1,
        ] {
            let quality = f32::from_bits(bits);
            if quality > 100. {
                continue;
            }
            let mut a = setup(3, 1, quality);
            a[3] = 0;
            a[15] = 0;
            push(a);
        }
    }
    for _ in 0..128 {
        let mut a = setup(4, 4, (random(&mut seed) % 10001) as f32 / 100.);
        a[31] = ((random(&mut seed) % 10001) as f32 / 100.).to_bits();
        a[3] = random(&mut seed) % 101;
        a[4] = random(&mut seed) % 101;
        a[5] = random(&mut seed) % 8;
        a[7] = random(&mut seed) % 8;
        for i in 0..4 {
            a[15 + 4 * i] = (random(&mut seed) % 255).wrapping_sub(127);
            a[16 + 4 * i] = random(&mut seed) % 256;
        }
        push(a);
    }
    // A first trial can permanently merge segments before a later trial.
    let mut a = setup(4, 4, 100.);
    a[4] = 0;
    a[31] = 10f32.to_bits();
    push(a);
    cases
}
pub fn filter_cases() -> Vec<Case> {
    (0..8)
        .flat_map(|s| {
            (0..=128)
                .chain([i32::MAX as u32])
                .map(move |d| Case::new("VP8FilterStrengthFromDelta", vec![s, d]))
        })
        .collect()
}
pub fn cases() -> Vec<Case> {
    let mut cases = filter_cases();
    cases.extend(matrix_cases());
    cases.extend(setup_cases());
    cases.extend(analysis_cases());
    cases
}
pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = filter_cases().into_iter().step_by(41).collect();
    cases.extend(matrix_cases().into_iter().step_by(173));
    for op in 0..=4 {
        for q in [0., 74.999, 75., 100.] {
            let mut a = setup(op, 4, q);
            a[5] = 7;
            a[7] = 3;
            a[31] = 27.375f32.to_bits();
            cases.push(Case::new("VP8QuantSetup", a));
        }
    }
    for nb in 1..=4 {
        let mut a = setup(2, nb, 75.);
        for i in 0..4 {
            a[17 + 4 * i] = 20 + (i % 2) as u32;
            a[18 + 4 * i] = 10;
        }
        cases.push(Case::new("VP8QuantSetup", a));
    }
    cases.extend(analysis_cases().into_iter().step_by(47).take(4));
    cases
}

pub fn analysis_cases() -> Vec<Case> {
    super::vp8_analysis::frame_cases()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % 17 == 0 || *i >= 560)
        .map(|(_, frame)| frame)
        .flat_map(|frame| {
            [0f32, 30., 75., 98.5, 100.].into_iter().map(move |q| {
                let mut a = frame.input[..3].to_vec();
                a.push(q.to_bits());
                a.extend_from_slice(&frame.input[3..]);
                Case::new("VP8AnalysisQuant", a)
            })
        })
        .collect()
}
