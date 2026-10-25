use super::{Case, SEED, random, vp8_cost as cost, vp8_quant};
use libwebpop::{
    dsp::enc::{FTransform_C, kZigzag},
    enc::{quant_enc::*, vp8i_enc::*},
};

pub const MATRIX: usize = 4;
pub const PROBS: usize = 84;
pub const COEFFS: usize = 348;
pub const LEVELS: usize = 364;

pub fn score_case(lambda: u32, rate: i64, distortion: i64) -> Case {
    Case::new(
        "VP8TrellisScore",
        vec![
            lambda,
            rate as u32,
            (rate as u64 >> 32) as u32,
            distortion as u32,
            (distortion as u64 >> 32) as u32,
        ],
    )
}

pub fn block_case(
    ty: usize,
    ctx: usize,
    lambda: i32,
    m: &VP8Matrix,
    coeffs: [i16; 16],
    probs: &[u32],
) -> Case {
    let mut input = vec![ty as u32, ctx as u32, lambda as u32, 1];
    vp8_quant::matrix(m, &mut input);
    input.extend_from_slice(&probs[264 * ty..264 * (ty + 1)]);
    input.extend(coeffs.map(|v| v as i32 as u32));
    input.extend((0..16).map(|i| (if i % 2 == 0 { -1234i32 } else { 2345 }) as u32));
    Case::new("VP8TrellisBlock", input)
}

pub fn production_matrix(q: u32, ty: usize) -> (VP8Matrix, i32) {
    let mut enc = VP8Encoder::new_with_method(16, 16, false, 6).unwrap();
    for m in &mut enc.dqm {
        m.quant = q as i32;
    }
    SetupMatrices(&mut enc);
    let s = &enc.dqm[0];
    match ty {
        0 => (s.y1.clone(), s.lambda_trellis_i16),
        1 => (s.y2.clone(), s.lambda_trellis_i16),
        2 => (s.uv.clone(), s.lambda_trellis_uv),
        3 => (s.y1.clone(), s.lambda_trellis_i4),
        _ => unreachable!(),
    }
}

pub fn coefficients(m: &VP8Matrix, ty: usize, pattern: u32, seed: u32) -> [i16; 16] {
    let mut state = SEED ^ seed;
    let first = usize::from(ty == 0);
    if pattern == 5 {
        let mut src = [0u8; 100];
        let mut reference = [0u8; 100];
        for y in 0..4 {
            for x in 0..4 {
                src[32 * y + x] = random(&mut state) as u8;
                reference[32 * y + x] = random(&mut state) as u8;
            }
        }
        let mut coeffs = [0; 16];
        FTransform_C(&src, &reference, &mut coeffs);
        return coeffs;
    }
    let mut coeffs = [0; 16];
    for (n, &j) in kZigzag.iter().enumerate() {
        let q = i32::from(m.q[j]);
        let r = random(&mut state);
        let magnitude = match pattern {
            0 => 0,
            1 => {
                if n == first {
                    q / 2 + (seed % 3) as i32 - 1
                } else {
                    0
                }
            }
            2 => (r % 8161) as i32 - 4080,
            3 => q * (n as i32 % 5) + q / 2 + (seed % 3) as i32 - 1,
            4 => {
                if n == 15 {
                    2040
                } else {
                    0
                }
            }
            6 => {
                if n < first + 3 {
                    (r % (4 * q as u32)) as i32
                } else {
                    0
                }
            }
            7 => 8191 - i32::from(m.sharpen[j]),
            _ => unreachable!(),
        };
        coeffs[j] = (if r & 1 == 0 { magnitude } else { -magnitude }) as i16;
    }
    if ty == 0 {
        coeffs[0] = i16::MIN;
    }
    coeffs
}

pub fn tie_cases() -> Vec<Case> {
    let mut m = VP8Matrix {
        q: [4; 16],
        iq: [32768; 16],
        ..Default::default()
    };
    m.bias.fill(u32::MAX);
    m.zthresh.fill(u32::MAX); // ignored by trellis
    let probs = vec![128; 1056];
    let mut cases = Vec::new();
    for ty in [0, 3] {
        for values in [[2, 0, 0], [2, 8, 0], [8, 2, 0], [8, 8, 2]] {
            let mut coeffs = [0; 16];
            let first = usize::from(ty == 0);
            for (n, v) in values.into_iter().enumerate() {
                coeffs[kZigzag[first + n]] = v;
            }
            cases.push(block_case(ty, 0, 0, &m, coeffs, &probs));
        }
    }
    cases
}

pub fn score_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for lambda in [0, 1, 16, 257, 65536, 1000000] {
        for rate in [-(1i64 << 40), -1000, -1, 0, 1, 1000, 1i64 << 40] {
            for distortion in [-(1i64 << 40), -1000, -1, 0, 1, 1000, 1i64 << 40] {
                cases.push(score_case(lambda, rate, distortion));
            }
        }
    }
    cases
}

pub fn block_cases() -> Vec<Case> {
    let sets = cost::probability_sets();
    let mut cases = tie_cases();
    for q in 0..128 {
        for ty in 0..4 {
            let (m, lambda) = production_matrix(q, ty);
            for ctx in 0..3 {
                for pattern in 0..8 {
                    cases.push(block_case(
                        ty,
                        ctx,
                        lambda,
                        &m,
                        coefficients(&m, ty, pattern, q),
                        &sets[(q as usize + ty + ctx + pattern as usize) % sets.len()],
                    ));
                }
            }
        }
    }
    for probs in &sets {
        for ty in [0, 3] {
            let (m, _) = production_matrix(32, ty);
            for ctx in 0..3 {
                for lambda in [0, 1, 16, 128, 4096, 1000000] {
                    for pattern in 0..8 {
                        cases.push(block_case(
                            ty,
                            ctx,
                            lambda,
                            &m,
                            coefficients(&m, ty, pattern, 17),
                            probs,
                        ));
                    }
                }
            }
        }
    }
    cases.extend(varied_matrix_cases());
    cases.extend(pruning_cases());
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = score_cases().into_iter().step_by(37).collect();
    cases.extend(tie_cases());
    cases.extend(block_cases().into_iter().skip(8).step_by(397));
    cases.extend(pruning_cases().into_iter().step_by(7));
    cases
}

pub fn initialized(a: &[u32]) -> (VP8Encoder, VP8Matrix, [i16; 16], [i16; 16]) {
    let mut enc = VP8Encoder::new(16, 16, false).unwrap();
    for (dst, &v) in enc.proba.coeffs[a[0] as usize]
        .iter_mut()
        .flatten()
        .flatten()
        .zip(&a[PROBS..COEFFS])
    {
        *dst = v as u8;
    }
    enc.proba.dirty = true;
    libwebpop::enc::cost_enc::VP8CalculateLevelCosts(&mut enc.proba);
    let m = VP8Matrix {
        q: std::array::from_fn(|i| a[MATRIX + i] as u16),
        iq: std::array::from_fn(|i| a[MATRIX + 16 + i] as u16),
        bias: std::array::from_fn(|i| a[MATRIX + 32 + i]),
        zthresh: std::array::from_fn(|i| a[MATRIX + 48 + i]),
        sharpen: std::array::from_fn(|i| a[MATRIX + 64 + i] as u16),
    };
    (
        enc,
        m,
        std::array::from_fn(|i| a[COEFFS + i] as i16),
        std::array::from_fn(|i| a[LEVELS + i] as i16),
    )
}

pub fn read_wide(a: &[u32]) -> i64 {
    (u64::from(a[0]) | (u64::from(a[1]) << 32)) as i64
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if case.function == "VP8TrellisScore" {
        let score = RDScoreTrellis(a[0] as i32, read_wide(&a[1..]), read_wide(&a[3..]));
        return vec![score as u32, (score as u64 >> 32) as u32];
    }
    assert_eq!(case.function, "VP8TrellisBlock");
    let (enc, m, mut input, mut output) = initialized(a);
    let mut trace = Vec::new();
    let nz = TrellisQuantizeBlockWithObserver(
        &enc,
        &mut input,
        &mut output,
        a[1] as usize,
        a[0] as usize,
        &m,
        a[2] as i32,
        |level, band, ctx, cost| {
            if a[3] != 0 {
                trace.extend([level as u32, band as u32, ctx as u32, cost as u32]);
            }
        },
    );
    let mut out = vec![u32::from(nz)];
    out.extend(input.map(|v| v as i32 as u32));
    out.extend(output.map(|v| v as i32 as u32));
    out.push(trace.len() as u32 / 4);
    out.extend(trace);
    out
}

pub fn varied_matrix_cases() -> Vec<Case> {
    let sets = cost::probability_sets();
    let mut seed = SEED;
    let mut cases = Vec::new();
    for index in 0..128 {
        let mut m = VP8Matrix::default();
        for i in 0..16 {
            m.q[i] = (4 + random(&mut seed) % 437) as u16;
            m.iq[i] = (131072 / u32::from(m.q[i])) as u16;
            m.sharpen[i] = (random(&mut seed) % 513) as u16;
            m.bias[i] = random(&mut seed);
            m.zthresh[i] = random(&mut seed);
        }
        for ty in 0..4 {
            for ctx in 0..3 {
                let mut coeffs = [0; 16];
                for n in 0..16 {
                    let r = random(&mut seed);
                    coeffs[kZigzag[n]] = if index % 2 == 0 && n > 3 {
                        0
                    } else {
                        ((r % 15359) as i32 - 7679) as i16
                    };
                }
                if ty == 0 {
                    coeffs[0] = if index % 2 == 0 { i16::MIN } else { i16::MAX };
                }
                let lambda = [0, 1, 16, 128, 4096, 1000000][index % 6];
                let mut case = block_case(ty, ctx, lambda, &m, coeffs, &sets[index % sets.len()]);
                // Also exercise the unobserved C route; Rust's normal no-op
                // entry point is used independently in the invariant checks.
                if index % 11 == 0 {
                    case.input[3] = 0;
                }
                cases.push(case);
            }
        }
    }
    cases
}

pub fn pruning_cases() -> Vec<Case> {
    let probs = vec![128; 1056];
    let mut cases = Vec::new();
    for ty in [0, 3] {
        for position in [3, 7, 14, 15] {
            for sign in [-1, 1] {
                for value in [1, 2, 3] {
                    let mut m = VP8Matrix {
                        q: [4; 16],
                        iq: [32768; 16],
                        ..Default::default()
                    };
                    m.sharpen[kZigzag[position]] = 16;
                    let mut coeffs = [0; 16];
                    coeffs[kZigzag[usize::from(ty == 0)]] = 8;
                    coeffs[kZigzag[position]] = sign * value;
                    cases.push(block_case(ty, 0, 1, &m, coeffs, &probs));
                }
            }
        }
    }
    cases
}
