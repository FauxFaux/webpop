use super::{Case, SEED, random, vp8_cost as cost};
use libwebpop::internal::enc::{
    cost_enc::*, iterator_enc::*, picture_enc::WebPYUVAPicture, vp8i_enc::*,
};

pub const SNAPSHOT: usize = 79;
pub const PACKET: usize = 558;
pub const I4_TRACE: usize = 679;

pub fn snapshot(it: &VP8EncIterator, out: &mut Vec<u32>) {
    out.extend([
        it.x as u32,
        it.y as u32,
        it.count_down as u32,
        it.count_down0 as u32,
        u32::from(VP8IteratorIsDone(it)),
    ]);
    out.extend(
        it.y_left
            .iter()
            .chain(&it.uv_left[..9])
            .chain(&it.uv_left[16..])
            .copied()
            .map(u32::from),
    );
    out.extend(it.top_nz.into_iter().chain(it.left_nz).map(u32::from));
    out.extend(it.left_derr.into_iter().flatten().map(|v| v as i32 as u32));
    out.push(u32::from(it.do_trellis));
    out.extend(it.bit_count.into_iter().flatten().map(|v| v as u32));
    out.extend([
        it.luma_bits as u32,
        (it.luma_bits >> 32) as u32,
        it.uv_bits as u32,
        (it.uv_bits >> 32) as u32,
    ]);
}
fn tops(it: &VP8EncIterator, enc: &VP8Encoder, out: &mut Vec<u32>) {
    out.extend(
        it.y_top(enc)[..16]
            .iter()
            .chain(&it.uv_top(enc)[..16])
            .copied()
            .map(u32::from),
    );
}
fn i4(it: &VP8EncIterator, out: &mut Vec<u32>) {
    out.extend([it.i4 as u32, it.i4_top as u32]);
    out.extend(it.i4_boundary.into_iter().map(u32::from));
}
pub fn scan_picture(a: &[u32]) -> WebPYUVAPicture {
    let (w, h) = (a[0] as usize, a[1] as usize);
    let uv = w.div_ceil(2) * h.div_ceil(2);
    let plane = |from: usize, n: usize| a[from..from + n].iter().map(|&v| v as u8).collect();
    WebPYUVAPicture {
        width: a[0],
        height: a[1],
        y: plane(7, w * h),
        u: plane(7 + w * h, uv),
        v: plane(7 + w * h + uv, uv),
        a: None,
    }
}
pub fn initialized(a: &[u32]) -> (VP8Encoder, VP8EncIterator) {
    let mut enc = VP8Encoder::new(a[0], a[1], a[3] != 0).unwrap();
    for mb in &mut enc.mb_info {
        mb.alpha = 199;
    }
    if let Some(derr) = &mut enc.top_derr {
        derr.fill([[0x5a; 2]; 2]);
    }
    let mut it = VP8EncIterator {
        top_nz: [3; 9],
        left_nz: [3; 9],
        left_derr: [[7; 2]; 2],
        bit_count: [[0xbeef; 3]; 4],
        do_trellis: true,
        luma_bits: 0x12345678abcdef01,
        uv_bits: 0xfedcba9876543210,
        ..Default::default()
    };
    VP8IteratorInit(&mut enc, &mut it);
    it.yuv_in.fill(0xa5);
    (enc, it)
}
pub fn scan_evaluate(a: &[u32]) -> Vec<u32> {
    let pic = scan_picture(a);
    let (mut enc, mut it) = initialized(a);
    let mut out = Vec::new();
    snapshot(&it, &mut out);
    VP8IteratorSetRow(&mut it, &enc, a[4] as usize);
    VP8IteratorSetCountDown(&mut it, a[5] as i32);
    snapshot(&it, &mut out);
    let start = 7 + pic.y.len() + pic.u.len() + pic.v.len();
    for p in a[start..].chunks_exact(PACKET) {
        VP8IteratorImport(&mut it, &enc, &pic, a[2] != 0);
        snapshot(&it, &mut out);
        out.extend(it.yuv_in.into_iter().map(u32::from));
        tops(&it, &enc, &mut out);
        it.yuv_out = std::array::from_fn(|i| p[i] as u8);
        if a[6] != 0 && a[2] == 0 {
            VP8IteratorStartI4(&mut it, &enc);
            i4(&it, &mut out);
            let reconstructed = it.yuv_out;
            for _ in 0..16 {
                out.push(u32::from(VP8IteratorRotateI4(&mut it, &reconstructed)));
                i4(&it, &mut out);
            }
        }
        let modes = std::array::from_fn(|i| p[513 + i] as u8);
        if p[512] != 0 {
            VP8SetIntra16Mode(&it, &mut enc, modes[0]);
        } else {
            VP8SetIntra4Mode(&it, &mut enc, &modes);
        }
        VP8SetIntraUVMode(&it, &mut enc, p[529] as u8);
        VP8SetSkip(&it, &mut enc, p[530] != 0);
        VP8SetSegment(&it, &mut enc, p[531] as u8);
        it.top_nz = std::array::from_fn(|i| p[532 + i] as u8);
        it.left_nz = std::array::from_fn(|i| p[541 + i] as u8);
        if let Some(derr) = &mut enc.top_derr {
            it.left_derr =
                std::array::from_fn(|i| std::array::from_fn(|j| p[550 + 2 * i + j] as i32 as i8));
            derr[it.x] =
                std::array::from_fn(|i| std::array::from_fn(|j| p[554 + 2 * i + j] as i32 as i8));
        }
        VP8IteratorBytesToNz(&it, &mut enc);
        if a[2] == 0 {
            VP8IteratorSaveBoundary(&mut it, &mut enc);
        }
        snapshot(&it, &mut out);
        tops(&it, &enc, &mut out);
        out.push(enc.nz[it.x + 1]);
        out.push(u32::from(VP8IteratorNext(&mut it, &enc)));
        snapshot(&it, &mut out);
    }
    out.extend(enc.preds.into_iter().map(u32::from));
    for mb in enc.mb_info {
        out.extend([
            u32::from(mb.r#type),
            u32::from(mb.uv_mode),
            u32::from(mb.skip),
            u32::from(mb.segment),
            u32::from(mb.alpha),
        ]);
    }
    out.extend(enc.nz);
    out.extend(enc.y_top.into_iter().chain(enc.uv_top).map(u32::from));
    if let Some(derr) = enc.top_derr {
        out.extend(
            derr.into_iter()
                .flatten()
                .flatten()
                .map(|v| v as i32 as u32),
        );
    }
    out
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if case.function == "VP8IteratorScan" {
        return scan_evaluate(a);
    }
    let mut enc = VP8Encoder::new(16, 16, false).unwrap();
    let mut it = VP8EncIterator::default();
    if case.function == "VP8IteratorNz" {
        enc.nz = vec![a[2], a[1]];
        it.top_nz = std::array::from_fn(|i| a[4 + i] as u8);
        it.left_nz = std::array::from_fn(|i| a[13 + i] as u8);
        it.left_nz[8] = a[3] as u8;
        if a[0] == 0 {
            VP8IteratorNzToBytes(&mut it, &enc);
        }
        VP8IteratorBytesToNz(&it, &mut enc);
        it.top_nz
            .into_iter()
            .chain(it.left_nz)
            .map(u32::from)
            .chain([enc.nz[1], enc.nz[0]])
            .collect()
    } else {
        enc.nz = vec![a[1], a[0]];
        it.i4 = a[3] as usize;
        it.top_nz = std::array::from_fn(|i| a[4 + i] as u8);
        it.left_nz = std::array::from_fn(|i| a[13 + i] as u8);
        it.left_nz[8] = a[2] as u8;
        enc.proba = cost::read_probas(&a[22..1078]);
        enc.proba.dirty = true;
        VP8CalculateLevelCosts(&mut enc.proba);
        let rd = VP8ModeScore {
            y_dc_levels: std::array::from_fn(|i| a[1078 + i] as i32 as i16),
            y_ac_levels: std::array::from_fn(|b| {
                std::array::from_fn(|i| a[1094 + 16 * b + i] as i32 as i16)
            }),
            uv_levels: std::array::from_fn(|b| {
                std::array::from_fn(|i| a[1350 + 16 * b + i] as i32 as i16)
            }),
            ..Default::default()
        };
        let levels = std::array::from_fn(|i| a[1478 + i] as i32 as i16);
        let mut out = vec![VP8GetCostLuma4(&it, &enc, &levels) as u32];
        out.extend(it.top_nz.into_iter().chain(it.left_nz).map(u32::from));
        out.push(VP8GetCostLuma16(&mut it, &enc, &rd) as u32);
        out.extend(it.top_nz.into_iter().chain(it.left_nz).map(u32::from));
        out.push(VP8GetCostUV(&mut it, &enc, &rd) as u32);
        out.extend(it.top_nz.into_iter().chain(it.left_nz).map(u32::from));
        out.extend(enc.nz);
        out
    }
}

pub fn nz_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for i in 0..1100 {
        let (top, left) = if i < 32 {
            (1 << i, 0)
        } else if i < 64 {
            (0, 1 << (i - 32))
        } else {
            (random(&mut seed), random(&mut seed))
        };
        let mut input = vec![0, top, left, i & 1];
        input.extend([1; 18]);
        cases.push(Case::new("VP8IteratorNz", input));
    }
    // All independent outgoing edge bits, plus deliberately inconsistent
    // shared corners: packing must use the top array at those corners.
    for bits in 0..16384u32 {
        let mut input = vec![1, 0xdeadbeef, 0x31415926, bits & 1];
        input.extend((0..9).map(|i| (bits >> i) & 1));
        input.extend((0..9).map(|i| match i {0=>bits>>9,1=>bits>>10,2=>bits>>11,4=>bits>>12,6=>bits>>13,_=>!(bits>>i)}&1));
        cases.push(Case::new("VP8IteratorNz", input));
    }
    cases
}

pub fn scan_case(
    w: u32,
    h: u32,
    source: bool,
    diffusion: bool,
    start: u32,
    count: u32,
    trace: bool,
    pattern: u32,
) -> Case {
    let mut input = vec![
        w,
        h,
        u32::from(source),
        u32::from(diffusion),
        start,
        count,
        u32::from(trace),
    ];
    let mut seed = SEED ^ w ^ (h << 16) ^ (pattern << 8);
    let pixels = w * h + 2 * w.div_ceil(2) * h.div_ceil(2);
    input.extend((0..pixels).map(|i| {
        if pattern == 0 {
            i.wrapping_mul(31).wrapping_add(17) & 255
        } else {
            random(&mut seed) & 255
        }
    }));
    for block in 0..count {
        input.extend((0..512).map(|i| {
            if pattern == 0 {
                (255u32.wrapping_sub(i * 7 + block * 13)) & 255
            } else {
                random(&mut seed) & 255
            }
        }));
        let intra16 = (block + pattern) & 1;
        input.push(intra16);
        input.extend((0..16).map(|_| random(&mut seed) % if intra16 != 0 { 4 } else { 10 }));
        input.extend([block % 4, block & 1, (block + 1) % 4]);
        input.extend((0..18).map(|_| random(&mut seed) & 1));
        input.extend((0..8).map(|_| (random(&mut seed) as u8 as i8) as i32 as u32));
    }
    Case::new("VP8IteratorScan", input)
}

pub fn scan_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1u32, 1u32),
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
        (48, 49),
        (65, 35),
    ] {
        let (mw, mh) = (w.div_ceil(16), h.div_ceil(16));
        for source in [false, true] {
            for diffusion in [false, true] {
                for trace in [false, true] {
                    for pattern in 0..2 {
                        cases.push(scan_case(
                            w,
                            h,
                            source,
                            diffusion,
                            0,
                            mw * mh,
                            trace,
                            pattern,
                        ));
                    }
                }
            }
        }
        if mh > 1 {
            cases.push(scan_case(w, h, false, true, 1, mw * (mh - 1), true, 1));
            cases.push(scan_case(w, h, true, false, 1, mw.min(2), false, 0));
        }
        cases.push(scan_case(w, h, false, true, mh, 0, false, 0));
    }
    for (w, h) in [(16383u32, 1u32), (1, 16383)] {
        for source in [false, true] {
            cases.push(scan_case(
                w,
                h,
                source,
                true,
                0,
                w.div_ceil(16) * h.div_ceil(16),
                false,
                1,
            ));
        }
    }
    cases
}

pub fn cost_cases() -> Vec<Case> {
    let mut seed = SEED;
    let mut cases = Vec::new();
    for probs in cost::probability_sets() {
        for i4 in 0..16 {
            for pattern in 0..3 {
                let mut input = vec![
                    random(&mut seed),
                    random(&mut seed),
                    random(&mut seed) & 1,
                    i4,
                ];
                input.extend((0..18).map(|_| random(&mut seed) & 1));
                input.extend_from_slice(&probs);
                for block in 0..26 {
                    for i in 0..16 {
                        let v = if pattern == 0 || (block > 0 && block < 17 && i == 0) {
                            0
                        } else if pattern == 1 {
                            if i == (block % 15) + 1 {
                                [1, -1, 9, -10, 67, -68, 2047, -2047][block % 8]
                            } else {
                                0
                            }
                        } else {
                            let v = random(&mut seed);
                            if v & 3 == 0 {
                                0
                            } else {
                                (v % 4095) as i32 - 2047
                            }
                        };
                        input.push(v as u32);
                    }
                }
                cases.push(Case::new("VP8IteratorCosts", input));
            }
        }
    }
    cases
}

pub fn cases() -> Vec<Case> {
    nz_cases()
        .into_iter()
        .chain(scan_cases())
        .chain(cost_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = nz_cases().into_iter().step_by(311).collect();
    for (w, h, source, diffusion, start, count, trace, pattern) in [
        (1, 1, false, false, 0, 1, true, 0),
        (1, 17, true, true, 0, 2, false, 1),
        (17, 1, false, true, 0, 2, true, 1),
        (17, 19, false, true, 0, 4, true, 0),
        (17, 19, true, false, 0, 4, false, 1),
        (33, 17, false, false, 1, 3, true, 1),
        (16, 16, true, true, 0, 1, false, 0),
        (3, 5, false, true, 1, 0, false, 1),
    ] {
        cases.push(scan_case(
            w, h, source, diffusion, start, count, trace, pattern,
        ));
    }
    cases.extend(cost_cases().into_iter().step_by(61));
    cases
}
