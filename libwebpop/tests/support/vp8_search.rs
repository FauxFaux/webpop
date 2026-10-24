use super::{Case, SEED, random, vp8_reconstruct as reconstruction};
use libwebpop::{
    dsp::quant::*,
    enc::{cost_enc::*, quant_enc::*, vp8i_enc::*},
};

pub fn initialized(a: &[u32]) -> (VP8Encoder, VP8EncIterator) {
    let (mut enc, mut it) = reconstruction::initialized(a);
    enc.max_i4_header_bits = a[635] as i32;
    enc.preds = a[639..720].iter().map(|&v| v as u8).collect();
    it.top_nz = std::array::from_fn(|i| ((i + 1) % 2) as u8);
    it.left_nz = std::array::from_fn(|i| (i % 2) as u8);
    if a[636] != 0 && a[636] < 4 {
        for (i, p) in enc
            .proba
            .coeffs
            .as_flattened_mut()
            .as_flattened_mut()
            .as_flattened_mut()
            .iter_mut()
            .enumerate()
        {
            *p = match a[636] {
                1 => 0,
                2 => 255,
                _ => (37 * i + 17) as u8,
            };
        }
    }
    if a[636] == 4 {
        for m in &mut enc.dqm {
            m.lambda_i16 = 0;
            m.lambda_i4 = 0;
            m.lambda_uv = 0;
            m.lambda_mode = 1;
            m.tlambda = 0;
        }
    }
    enc.proba.dirty = true;
    VP8CalculateLevelCosts(&mut enc.proba);
    it.yuv_out.fill(0x5a);
    it.yuv_out2.fill(0xc3);
    (enc, it)
}
pub fn run(
    a: &[u32],
    enc: &mut VP8Encoder,
    it: &mut VP8EncIterator,
    rd: &mut VP8ModeScore,
) -> bool {
    rd.score = (u64::from(a[637]) | (u64::from(a[638]) << 32)) as i64;
    VP8MakeLuma16Preds(it, enc);
    VP8MakeChroma8Preds(it, enc);
    match a[634] {
        0 => {
            PickBestIntra16(it, enc, rd);
            false
        }
        1 => {
            PickBestIntra16(it, enc, rd);
            rd.modes_i4 = std::array::from_fn(|i| (i % 10) as u8);
            PickBestIntra4(it, enc, rd)
        }
        2 => VP8Decimate(it, enc, rd, VP8RDLevel::RD_OPT_BASIC),
        3 => {
            PickBestUV(it, enc, rd);
            false
        }
        4 => PickBestIntra4(it, enc, rd),
        _ => unreachable!(),
    }
}
pub fn logical_score(
    a: &[u32],
    enc: &VP8Encoder,
    it: &VP8EncIterator,
    rd: &VP8ModeScore,
) -> VP8ModeScore {
    let stage = a[634];
    let mut score = VP8ModeScore::default();
    CopyScore(&mut score, rd);
    score.mode_i16 = rd.mode_i16;
    score.y_dc_levels = rd.y_dc_levels;
    score.y_ac_levels = rd.y_ac_levels;
    if stage == 1 || stage == 4 || (stage == 2 && enc.mb_info[it.y * enc.mb_w + it.x].r#type == 0) {
        score.modes_i4 = rd.modes_i4;
    }
    if stage == 2 || stage == 3 {
        score.mode_uv = rd.mode_uv;
        score.uv_levels = rd.uv_levels;
        if enc.top_derr.is_some() {
            score.derr = rd.derr;
        }
    }
    score
}
pub fn state(
    a: &[u32],
    enc: &VP8Encoder,
    it: &VP8EncIterator,
    rd: &VP8ModeScore,
    result: bool,
) -> Vec<u32> {
    let mut out = vec![u32::from(result)];
    reconstruction::score(&logical_score(a, enc, it, rd), &mut out);
    out.extend(it.yuv_out.map(u32::from));
    out.extend(it.yuv_out2.map(u32::from));
    out.extend(it.yuv_p.map(u32::from));
    let mut dummy = [[[11, 12], [13, 14]], [[15, 16], [17, 18]]];
    dummy[it.x] = std::array::from_fn(|ch| std::array::from_fn(|k| a[8 + 2 * ch + k] as i32 as i8));
    reconstruction::errors(it, enc.top_derr.as_deref().unwrap_or(&dummy), &mut out);
    out.extend(it.top_nz.map(u32::from));
    out.extend(it.left_nz.map(u32::from));
    out.extend([it.i4 as u32, it.i4_top as u32]);
    out.extend(it.i4_boundary.map(u32::from));
    super::vp8_analysis::mbs(enc, &mut out);
    out.extend(enc.preds.iter().map(|&v| u32::from(v)));
    out.extend(enc.nz.iter().copied());
    out.extend(enc.dqm.iter().map(|s| s.max_edge as u32));
    out
}
pub fn scores(a: &[u32]) -> (VP8ModeScore, VP8ModeScore) {
    let mut dst = reconstruction::seed_score();
    let mut src = dst.clone();
    let read = |i: usize| (u64::from(a[i]) | (u64::from(a[i + 1]) << 32)) as i64;
    [dst.D, dst.SD, dst.H, dst.R, dst.score] = std::array::from_fn(|i| read(2 + 2 * i));
    dst.nz = a[12];
    [src.D, src.SD, src.H, src.R, src.score] = std::array::from_fn(|i| read(13 + 2 * i));
    src.nz = a[23];
    src.y_dc_levels[0] = 99;
    src.mode_uv = 3;
    src.derr[0][0] = 77;
    (dst, src)
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8SearchFrame" => frame_evaluate(a),
        "VP8ModeSearch" => {
            let (mut enc, mut it) = initialized(a);
            let mut rd = reconstruction::seed_score();
            let result = run(a, &mut enc, &mut it, &mut rd);
            state(a, &enc, &it, &rd, result)
        }
        "VP8Flat" => {
            let src: Vec<_> = a[2..514].iter().map(|&v| v as u8).collect();
            let levels: Vec<[i16; 16]> = a[514..]
                .chunks_exact(16)
                .map(|c| std::array::from_fn(|i| c[i] as i16))
                .collect();
            vec![
                u32::from(IsFlat(&levels, a[0] as i32)),
                u32::from(IsFlatSource16(&src)),
            ]
        }
        "VP8ModeCost" => {
            let mut enc = VP8Encoder::new(32, 32, false).unwrap();
            enc.preds = a[2..83].iter().map(|&v| v as u8).collect();
            let it = VP8EncIterator {
                x: (a[1] & 1) as usize,
                y: ((a[1] >> 1) & 1) as usize,
                i4: a[0] as usize,
                ..Default::default()
            };
            let modes = std::array::from_fn(|i| a[83 + i] as u8);
            GetCostModeI4(&it, &enc, &modes).map(u32::from).to_vec()
        }
        "VP8ScoreOps" => {
            let (mut dst, src) = scores(a);
            match a[0] {
                0 => InitScore(&mut dst),
                1 => CopyScore(&mut dst, &src),
                2 => AddScore(&mut dst, &src),
                3 => SetRDScore(a[1] as i32, &mut dst),
                _ => unreachable!(),
            };
            let mut out = Vec::new();
            reconstruction::score(&dst, &mut out);
            out
        }
        _ => unreachable!(),
    }
}
pub fn search_case(stage: u32, q: u32, flags: u32, pattern: u32) -> Case {
    let mut a = reconstruction::request(0, q, flags, 0, pattern).input;
    a[2] = (q + pattern) % 4;
    if pattern == 8 {
        for i in 16..33 {
            a[i] = ((i - 16) * 17 % 256) as u32;
        }
        a[58..90].fill(128);
        a[122..634].fill(130);
    }
    if pattern == 9 {
        a[122..634].fill(128);
        a[33] = 60;
        a[49] = 100;
        for y in 0..8 {
            a[34 + y] = 70 + 4 * y as u32;
            a[50 + y] = 120 + 3 * y as u32;
        }
        for col in 0..2 {
            for x in 0..8 {
                a[90 + 16 * col + x] = 80 + 3 * x as u32;
                a[98 + 16 * col + x] = 110 + 2 * x as u32;
            }
        }
        for y in 0..8 {
            for x in 0..8 {
                a[122 + 16 + y * 32 + x] = 90 + 3 * x as u32 + 4 * y as u32;
                a[122 + 24 + y * 32 + x] = 130 + 2 * x as u32 + 3 * y as u32;
            }
        }
    }
    a.extend([
        stage,
        65536,
        (q + pattern) % 4,
        MAX_COST as u32,
        (MAX_COST >> 32) as u32,
    ]);
    a.extend((0..81).map(|i| {
        if pattern == 0 {
            0
        } else {
            (i * 37 + pattern) % 10
        }
    }));
    Case::new("VP8ModeSearch", a)
}
pub fn search_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in 0..128 {
        for stage in 0..5 {
            for pattern in 0..10 {
                cases.push(search_case(stage, q, q % 8, pattern));
            }
        }
    }
    for flags in 0..8 {
        for budget in [0, 1, 211, 1000, 4000, 65536] {
            for pattern in 0..8 {
                for stage in [1, 2, 4] {
                    let mut case = search_case(stage, 37 + flags, flags, pattern);
                    case.input[635] = budget;
                    cases.push(case);
                }
            }
        }
    }
    cases.extend(boundary_cases());
    cases.extend(tie_cases());
    cases
}
pub fn boundary_cases() -> Vec<Case> {
    // Pinned C baseline: all-DC I4 costs 640 header units and scores 2382600.
    [
        (639, MAX_COST),
        (640, MAX_COST),
        (65536, 2382599),
        (65536, 2382600),
        (65536, 2382601),
    ]
    .into_iter()
    .map(|(budget, score)| {
        let mut case = search_case(4, 63, 7, 0);
        case.input[635] = budget;
        case.input[637] = score as u32;
        case.input[638] = (score >> 32) as u32;
        case
    })
    .collect()
}
pub fn flat_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for count in [1, 8, 16] {
        for position in 0..16 * count {
            for thresh in [0, 2, 3, 240] {
                let mut a = vec![thresh, count];
                a.extend([128; 512]);
                a.extend(vec![0; 16 * count as usize]);
                a[514 + position as usize] = (-2047i32) as u32;
                cases.push(Case::new("VP8Flat", a));
            }
        }
    }
    for position in 0..512 {
        let mut a = vec![0, 1];
        a.extend([128; 512]);
        a[2 + position] = 129;
        a.extend([0; 16]);
        cases.push(Case::new("VP8Flat", a));
    }
    cases
}
pub fn score_cases() -> Vec<Case> {
    let mut seed = SEED;
    let mut cases = Vec::new();
    for kind in 0..4 {
        for k in 0..512 {
            let mut a = vec![kind, if k % 2 == 0 { 1 } else { 1000000 }];
            for _ in 0..2 {
                for i in 0..5 {
                    let v = match k {
                        0 => 0,
                        1 => 1i64 << 40,
                        2 => -(1i64 << 40),
                        _ => i64::from(random(&mut seed) as i32) * (i + 1),
                    };
                    a.extend([v as u32, (v >> 32) as u32]);
                }
                a.push(random(&mut seed));
            }
            cases.push(Case::new("VP8ScoreOps", a));
        }
    }
    cases
}
pub fn cost_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for flags in 0..4 {
        for i4 in 0..16 {
            for top in 0..10 {
                for left in 0..10 {
                    let mut a = vec![i4, flags];
                    a.extend((0..81).map(|i| (i * 7) % 10));
                    a.extend((0..16).map(|i| (i * 3) % 10));
                    let base = 10 + 4 * (flags & 1) + 36 * ((flags >> 1) & 1);
                    let (x, y) = (i4 % 4, i4 / 4);
                    if x == 0 {
                        a[(2 + base + 9 * y - 1) as usize] = left;
                    } else {
                        a[(83 + i4 - 1) as usize] = left;
                    }
                    if y == 0 {
                        a[(2 + base - 9 + x) as usize] = top;
                    } else {
                        a[(83 + i4 - 4) as usize] = top;
                    }
                    cases.push(Case::new("VP8ModeCost", a));
                }
            }
        }
    }
    cases
}
pub fn cases() -> Vec<Case> {
    let mut c = search_cases();
    c.extend(flat_cases());
    c.extend(score_cases());
    c.extend(cost_cases());
    c.extend(frame_cases());
    c
}
pub fn fixture_cases() -> Vec<Case> {
    let mut c = Vec::new();
    c.extend(flat_cases().into_iter().step_by(301));
    c.extend(score_cases().into_iter().step_by(173));
    c.extend(cost_cases().into_iter().step_by(613));
    for stage in 0..5 {
        for flags in [0, 7] {
            c.push(search_case(stage, 75, flags, 6));
        }
    }
    c.push(search_case(2, 3, 7, 9));
    for budget in [0, 1, 211, 1000, 65536] {
        let mut case = search_case(4, 63, 7, 2);
        case.input[635] = budget;
        c.push(case);
    }
    c.extend(frame_cases().into_iter().step_by(65).take(4));
    c.extend(boundary_cases());
    c.extend(tie_cases());
    c
}

pub fn record_residuals(it: &mut VP8EncIterator, enc: &mut VP8Encoder, rd: &VP8ModeScore) {
    use libwebpop::{dsp::cost::SetResidualCoeffs_C, enc::iterator_enc::*};
    VP8IteratorNzToBytes(it, enc);
    let i16 = enc.mb_info[it.y * enc.mb_w + it.x].r#type == 1;
    if i16 {
        let mut res = VP8InitResidual(0, 1);
        SetResidualCoeffs_C(&rd.y_dc_levels, &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[8] + it.left_nz[8]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[8] = nz;
        it.left_nz[8] = nz;
    }
    for n in 0..16 {
        let (x, y) = (n % 4, n / 4);
        let mut res = VP8InitResidual(usize::from(i16), if i16 { 0 } else { 3 });
        SetResidualCoeffs_C(&rd.y_ac_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    for n in 0..8 {
        let (x, y) = (4 + 2 * (n / 4) + n % 2, 4 + 2 * (n / 4) + (n % 4) / 2);
        let mut res = VP8InitResidual(0, 2);
        SetResidualCoeffs_C(&rd.uv_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    VP8IteratorBytesToNz(it, enc);
}
pub fn frame_evaluate(a: &[u32]) -> Vec<u32> {
    frame_evaluate_with_method(a, 4)
}
pub fn frame_evaluate_with_method(a: &[u32], method: i32) -> Vec<u32> {
    use libwebpop::enc::{analysis_enc::VP8EncAnalyze, iterator_enc::*};
    let quality = f32::from_bits(a[3]);
    let mut enc = super::vp8_analysis::seeded(a[0], a[1], a[2] as usize);
    enc.max_i4_header_bits = a[4] as i32;
    enc.config.method = method;
    enc.config.quality = quality;
    if quality <= 98. {
        enc.top_derr = Some(vec![[[0; 2]; 2]; enc.mb_w]);
    }
    let mut planes = a[..3].to_vec();
    planes.extend_from_slice(&a[5..]);
    let pic = super::vp8_analysis::picture(&planes);
    VP8EncAnalyze(&mut enc, &pic);
    VP8SetSegmentParams(&mut enc, quality);
    VP8CalculateLevelCosts(&mut enc.proba);
    if method == 4 {
        enc.preds.fill(0);
    }
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(&mut enc, &mut it);
    it.yuv_p.fill(0xa5);
    it.yuv_out.fill(0x5a);
    it.yuv_out2.fill(0xc3);
    let mut out = vec![enc.segment_hdr.num_segments as u32, enc.base_quant as u32];
    out.extend(enc.dqm.iter().map(|s| s.quant as u32));
    let mut normalize = [0; 720];
    normalize[634] = 2;
    loop {
        VP8IteratorImport(&mut it, &enc, &pic, false);
        let mut rd = VP8ModeScore::default();
        out.push(u32::from(VP8Decimate(
            &mut it,
            &mut enc,
            &mut rd,
            if method == 4 {
                VP8RDLevel::RD_OPT_BASIC
            } else {
                VP8RDLevel::RD_OPT_NONE
            },
        )));
        if method == 4 {
            reconstruction::score(&logical_score(&normalize, &enc, &it, &rd), &mut out);
        } else {
            reconstruction::score(&rd, &mut out);
        }
        out.extend(it.yuv_out.map(u32::from));
        let mb = enc.mb_info[it.y * enc.mb_w + it.x];
        out.extend([
            u32::from(mb.r#type),
            u32::from(mb.uv_mode),
            u32::from(mb.skip),
            u32::from(mb.segment),
            u32::from(mb.alpha),
        ]);
        record_residuals(&mut it, &mut enc, &rd);
        out.extend(it.top_nz.map(u32::from));
        out.extend(it.left_nz.map(u32::from));
        out.push(enc.nz[it.x + 1]);
        out.extend(it.left_derr.as_flattened().iter().map(|&v| v as i32 as u32));
        out.extend(
            enc.top_derr
                .as_ref()
                .map_or([[0; 2]; 2], |top| top[it.x])
                .as_flattened()
                .iter()
                .map(|&v| v as i32 as u32),
        );
        VP8IteratorSaveBoundary(&mut it, &mut enc);
        out.extend(it.y_left.map(u32::from));
        out.extend(
            it.uv_left[..9]
                .iter()
                .chain(&it.uv_left[16..])
                .map(|&v| u32::from(v)),
        );
        out.extend(
            it.y_top(&enc)[..16]
                .iter()
                .chain(&it.uv_top(&enc)[..16])
                .map(|&v| u32::from(v)),
        );
        if !VP8IteratorNext(&mut it, &enc) {
            break;
        }
    }
    out.extend(super::vp8_cost::stats_words(&enc.proba));
    out.extend(enc.preds.iter().map(|&v| u32::from(v)));
    out.extend(enc.nz.iter().copied());
    if let Some(top) = enc.top_derr {
        out.extend(top.iter().flatten().flatten().map(|&v| v as i32 as u32));
    }
    out.extend(enc.dqm.iter().map(|s| s.max_edge as u32));
    out
}
pub fn frame_cases() -> Vec<Case> {
    let frames = super::vp8_analysis::frame_cases();
    frames
        .into_iter()
        .enumerate()
        .filter(|(i, _)| (*i < 560 && i % 17 == 0) || *i == 566)
        .flat_map(|(_, frame)| {
            [0f32, 30., 75., 98., 98.5, 100.]
                .into_iter()
                .map(move |quality| {
                    let mut a = frame.input[..3].to_vec();
                    a.extend([quality.to_bits(), 65536]);
                    a.extend_from_slice(&frame.input[3..]);
                    Case::new("VP8SearchFrame", a)
                })
        })
        .collect()
}

pub fn tie_cases() -> Vec<Case> {
    [0, 2, 3, 4]
        .into_iter()
        .map(|stage| {
            let mut case = search_case(stage, 0, 3, 0);
            case.input[636] = 4;
            case
        })
        .collect()
}
