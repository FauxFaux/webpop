use super::{Case, SEED, random};
use libwebpop::{
    dsp::enc::*,
    enc::{quant_enc::*, vp8i_enc::*},
};

pub const DIMENSIONS: [(usize, usize); 9] = [
    (4, 4),
    (8, 8),
    (16, 8),
    (16, 16),
    (4, 4),
    (16, 16),
    (16, 4),
    (4, 4),
    (16, 8),
];
pub const WEIGHTS: [u16; 16] = [38, 32, 20, 9, 32, 28, 17, 7, 20, 17, 10, 4, 9, 7, 4, 2];

pub fn unpack(words: &[u32], w: usize, h: usize, padding: u8) -> Vec<u8> {
    let mut data = vec![padding; (h - 1) * BPS + w];
    for y in 0..h {
        for x in 0..w {
            data[y * BPS + x] = words[y * w + x] as u8;
        }
    }
    data
}

pub fn matrix(words: &[u32]) -> VP8Matrix {
    VP8Matrix {
        q: std::array::from_fn(|i| words[i] as u16),
        iq: std::array::from_fn(|i| words[16 + i] as u16),
        bias: words[32..48].try_into().unwrap(),
        zthresh: words[48..64].try_into().unwrap(),
        sharpen: std::array::from_fn(|i| words[64 + i] as u16),
    }
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "BIAS" => vec![BIAS(a[0])],
        "QUANTDIV" => vec![QUANTDIV(a[0], a[1], a[2]) as u32],
        "VP8DspTables" => {
            let mut out: Vec<_> = VP8DspScan
                .into_iter()
                .chain(VP8Scan)
                .chain(VP8I16ModeOffsets)
                .chain(VP8UVModeOffsets)
                .map(|v| v as u32)
                .collect();
            out.extend([QFIX, MAX_LEVEL as u32]);
            out
        }
        "VP8Quantize" => {
            let m = matrix(&a[1..81]);
            let mut input: Vec<_> = a[81..].iter().map(|&v| v as i16).collect();
            let mut levels = vec![12345; input.len()];
            let nz = if a[0] == 1 {
                Quantize2Blocks_C(
                    input.as_mut_slice().try_into().unwrap(),
                    levels.as_mut_slice().try_into().unwrap(),
                    &m,
                )
            } else {
                u32::from(QuantizeBlock_C(
                    input.as_mut_slice().try_into().unwrap(),
                    levels.as_mut_slice().try_into().unwrap(),
                    &m,
                ))
            };
            let mut out = vec![nz];
            out.extend(super::vp8_transforms::signed_words(&input));
            out.extend(super::vp8_transforms::signed_words(&levels));
            out
        }
        "VP8Metric" => {
            let op = a[0] as usize;
            let (w, h) = DIMENSIONS[op];
            let offset = if op == 4 || op == 5 { 17 } else { 1 };
            let src = unpack(&a[offset..], w, h, 0x5a);
            let reference = if op <= 5 {
                unpack(&a[offset + w * h..], w, h, 0x3c)
            } else {
                vec![]
            };
            let scalar = match op {
                0 => SSE4x4_C(&src, &reference),
                1 => SSE8x8_C(&src, &reference),
                2 => SSE16x8_C(&src, &reference),
                3 => SSE16x16_C(&src, &reference),
                4 | 5 => {
                    let weights = std::array::from_fn(|i| a[1 + i] as u16);
                    if op == 4 {
                        Disto4x4_C(&src, &reference, &weights)
                    } else {
                        Disto16x16_C(&src, &reference, &weights)
                    }
                }
                6 => {
                    let mut dc = [999; 4];
                    Mean16x4_C(&src, &mut dc);
                    return dc.to_vec();
                }
                7 | 8 => {
                    let mut dst = vec![0xa5; h * BPS];
                    if op == 7 {
                        Copy4x4_C(&src, &mut dst);
                    } else {
                        Copy16x8_C(&src, &mut dst);
                    }
                    return dst.into_iter().map(u32::from).collect();
                }
                _ => unreachable!(),
            };
            vec![scalar as u32]
        }
        "VP8SetHistogramData" | "VP8CollectHistogram" => {
            let mut histo = VP8Histogram {
                max_value: 999,
                last_non_zero: 999,
            };
            if case.function == "VP8SetHistogramData" {
                VP8SetHistogramData(&std::array::from_fn(|i| a[i] as i32), &mut histo);
            } else {
                let src = unpack(&a[2..258], 16, 16, 0x5a);
                let pred = unpack(&a[258..], 16, 16, 0xa5);
                CollectHistogram_C(&src, &pred, a[0] as usize, a[1] as usize, &mut histo);
            }
            vec![histo.max_value as u32, histo.last_non_zero as u32]
        }
        _ => unreachable!(),
    }
}

pub fn quantize_case(kind: u32, m: &VP8Matrix, input: &[i16]) -> Case {
    assert_eq!(input.len(), if kind == 1 { 32 } else { 16 });
    let mut a = vec![kind];
    a.extend(m.q.map(u32::from));
    a.extend(m.iq.map(u32::from));
    a.extend(m.bias);
    a.extend(m.zthresh);
    a.extend(m.sharpen.map(u32::from));
    a.extend(super::vp8_transforms::signed_words(input));
    Case::new("VP8Quantize", a)
}

pub fn uniform_matrix(q: u16, bias: u32, sharpen: u16) -> VP8Matrix {
    let iq = ((1u32 << QFIX) / u32::from(q)).min(65535) as u16;
    VP8Matrix {
        q: [q; 16],
        iq: [iq; 16],
        bias: [bias; 16],
        zthresh: [((1 << QFIX) - 1 - bias) / u32::from(iq); 16],
        sharpen: [sharpen; 16],
    }
}

pub fn quantization_cases() -> Vec<Case> {
    let mut cases = vec![Case::new("VP8DspTables", vec![])];
    for b in (0..=255).chain([256, 65535, 0x7fffffff, 0x80000000, u32::MAX]) {
        cases.push(Case::new("BIAS", vec![b]));
    }
    for n in [0, 1, 65535, 65536, 131071, 131072, 0x80000000, u32::MAX] {
        for iq in [0, 1, 511, 65535, 65536, u32::MAX] {
            for b in [0, 1, 65535, 131071, 0x80000000, u32::MAX] {
                cases.push(Case::new("QUANTDIV", vec![n, iq, b]));
            }
        }
    }
    for q in [1, 2, 4, 8, 16, 31, 128, 1024] {
        for b in [0, 32, 96, 128, 255] {
            for sharpen in [0, 1, 31] {
                let m = uniform_matrix(q, BIAS(b), sharpen);
                for delta in [-1i32, 0, 1] {
                    for kind in 0..3 {
                        let input: Vec<_> = (0..if kind == 1 { 32 } else { 16 })
                            .map(|i| {
                                let value = (m.zthresh[i % 16] as i32 - i32::from(sharpen) + delta)
                                    .max(0) as i16;
                                if i % 2 == 0 { value } else { -value }
                            })
                            .collect();
                        cases.push(quantize_case(kind, &m, &input));
                    }
                }
            }
        }
    }
    let m = uniform_matrix(16, BIAS(128), 0);
    for position in 0..16 {
        for value in [i16::MIN, -4095, -1, 0, 1, 4095, i16::MAX] {
            for kind in [0, 2] {
                let mut input = [0; 16];
                input[position] = value;
                cases.push(quantize_case(kind, &m, &input));
            }
        }
    }
    for mask in 0..4 {
        let mut input = [0; 32];
        if mask & 1 != 0 {
            input[0] = 16;
        }
        if mask & 2 != 0 {
            input[31] = -16;
        }
        cases.push(quantize_case(1, &m, &input));
    }
    let mut seed = SEED;
    for i in 0..1024 {
        let q = std::array::from_fn(|_| (random(&mut seed) % 16 + 1) as u16);
        let m = VP8Matrix {
            q,
            iq: std::array::from_fn(|_| random(&mut seed) as u16),
            bias: std::array::from_fn(|_| random(&mut seed)),
            zthresh: std::array::from_fn(|_| {
                if i % 2 == 0 {
                    random(&mut seed) & 65535
                } else {
                    random(&mut seed)
                }
            }),
            sharpen: std::array::from_fn(|_| random(&mut seed) as u16),
        };
        let kind = i % 3;
        let input: Vec<_> = (0..if kind == 1 { 32 } else { 16 })
            .map(|_| random(&mut seed) as i16)
            .collect();
        cases.push(quantize_case(kind, &m, &input));
    }
    for i in 0..512 {
        let mut m = VP8Matrix::default();
        for j in 0..16 {
            m.q[j] = [4, 7, 8, 16, 31, 128, 1024][random(&mut seed) as usize % 7];
            m.iq[j] = ((1u32 << QFIX) / u32::from(m.q[j])) as u16;
            m.bias[j] = BIAS([0, 96, 128, 255][random(&mut seed) as usize % 4]);
            m.zthresh[j] = ((1 << QFIX) - 1 - m.bias[j]) / u32::from(m.iq[j]);
            m.sharpen[j] = (random(&mut seed) % 32) as u16;
        }
        let kind = i % 3;
        let input: Vec<_> = (0..if kind == 1 { 32 } else { 16 })
            .map(|_| (random(&mut seed) % 32768) as i16 - 16384)
            .collect();
        cases.push(quantize_case(kind, &m, &input));
    }
    cases
}

pub fn metric_case(op: usize, a: &[u32], b: &[u32], weights: &[u16; 16]) -> Case {
    let mut words = vec![op as u32];
    if op == 4 || op == 5 {
        words.extend(weights.map(u32::from));
    }
    words.extend_from_slice(a);
    if op <= 5 {
        words.extend_from_slice(b);
    }
    Case::new("VP8Metric", words)
}

pub fn metric_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (op, (w, h)) in DIMENSIONS.into_iter().enumerate() {
        for pattern in 0..72 {
            let a: Vec<_> = (0..w * h)
                .map(|i| match pattern {
                    0 => 0,
                    1 => 255,
                    2 => {
                        if i % 2 == 0 {
                            0
                        } else {
                            255
                        }
                    }
                    3 => {
                        if i == w * h / 2 {
                            255
                        } else {
                            0
                        }
                    }
                    4 => (i & 255) as u32,
                    5 => 127,
                    _ => random(&mut seed) & 255,
                })
                .collect();
            let b: Vec<_> = (0..w * h)
                .map(|i| {
                    if pattern < 6 {
                        255 - a[i]
                    } else {
                        random(&mut seed) & 255
                    }
                })
                .collect();
            let weights = match pattern % 5 {
                0 => [0; 16],
                1 => WEIGHTS,
                2 => [65535; 16],
                _ => std::array::from_fn(|_| random(&mut seed) as u16),
            };
            cases.push(metric_case(op, &a, &b, &weights));
        }
    }
    cases
}

pub fn histogram_cases() -> Vec<Case> {
    let mut cases = vec![Case::new("VP8SetHistogramData", vec![0; 32])];
    let mut seed = SEED;
    for position in 0..32 {
        for value in [i32::MIN, -1, 1, 2, i32::MAX] {
            let mut bins = vec![0; 32];
            bins[position] = value as u32;
            cases.push(Case::new("VP8SetHistogramData", bins));
        }
    }
    for _ in 0..128 {
        cases.push(Case::new(
            "VP8SetHistogramData",
            (0..32).map(|_| random(&mut seed)).collect::<Vec<_>>(),
        ));
    }
    for start in 0..=24 {
        for end in start..=24 {
            let mut a = vec![start, end];
            a.extend((0..512).map(|_| random(&mut seed) & 255));
            cases.push(Case::new("VP8CollectHistogram", a));
        }
    }
    for delta in -255i32..=255 {
        for (start, end) in [(0, 16), (16, 24), (0, 0)] {
            let mut a = vec![start, end];
            a.extend([delta.max(0) as u32; 256]);
            a.extend([(-delta).max(0) as u32; 256]);
            cases.push(Case::new("VP8CollectHistogram", a));
        }
    }
    cases
}

pub fn cases() -> Vec<Case> {
    quantization_cases()
        .into_iter()
        .chain(metric_cases())
        .chain(histogram_cases())
        .collect()
}

pub fn fixture_cases() -> Vec<Case> {
    let mut fixtures = Vec::new();
    for cases in [quantization_cases(), metric_cases(), histogram_cases()] {
        fixtures.extend(
            cases
                .into_iter()
                .enumerate()
                .filter_map(|(i, c)| (i % 47 == 0).then_some(c)),
        );
    }
    fixtures
}
