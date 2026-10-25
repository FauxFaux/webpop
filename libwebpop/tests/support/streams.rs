use super::Case;
use libwebpop::internal::enc::vp8l_enc::*;
use libwebpop::internal::utils::bit_writer_utils::*;

pub fn encode(case: &Case, bw: &mut VP8LBitWriter) -> LosslessStreamStats {
    let a = &case.input;
    VP8LEncodeStream(
        bw,
        &a[7..],
        a[0] as usize,
        a[1] as usize,
        a[2] as usize,
        a[3],
        a[4],
        a[5] != 0,
    )
    .unwrap()
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut remaining = case.input[6];
    while remaining > 0 {
        let bits = remaining.min(32);
        VP8LPutBits(&mut bw, 0x05ad5ad5 & (u32::MAX >> (32 - bits)), bits).unwrap();
        remaining -= bits;
    }
    let s = encode(case, &mut bw);
    let mut out = vec![
        s.lossless_features,
        s.histogram_bits,
        s.transform_bits,
        s.cross_color_transform_bits,
        s.cache_bits,
        s.palette_size as u32,
        s.lossless_size as u32,
        s.lossless_hdr_size as u32,
        s.lossless_data_size as u32,
    ];
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    out.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&v| u32::from(v)),
    );
    out
}

pub fn case(
    w: usize,
    h: usize,
    pattern: u32,
    method: u32,
    quality: u32,
    exact: bool,
    prefix: u32,
) -> Case {
    let stride = w + 3;
    let mut a = vec![
        w as u32,
        h as u32,
        stride as u32,
        method,
        quality,
        u32::from(exact),
        prefix,
    ];
    a.extend(super::analysis::pixels(w, h, stride, pattern));
    Case::new("VP8LEncodeStream", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..14 {
        cases.push(case(19, 17, pattern, 4, 70, pattern % 2 == 0, 97));
    }
    for (pattern, method, quality) in [
        (2, 0, 70),
        (20, 0, 70),
        (6, 6, 100),
        (2, 6, 100),
        (20, 6, 100),
        (30, 5, 50),
        (34, 6, 100),
    ] {
        cases.push(case(19, 17, pattern, method, quality, true, 5));
    }
    cases
}

pub fn cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for (w, h) in [(1, 1), (1, 43), (47, 1), (19, 17), (65, 67), (257, 2)] {
        for pattern in 0..36 {
            for (method, quality) in [
                (0, 70),
                (1, 0),
                (3, 49),
                (4, 49),
                (4, 50),
                (4, 70),
                (5, 49),
                (5, 50),
                (6, 99),
                (6, 100),
            ] {
                for exact in [false, true] {
                    let prefix = [0, 5, 31, 63, 97][(pattern as usize + method as usize) % 5];
                    cases.push(case(w, h, pattern, method, quality, exact, prefix));
                }
            }
        }
    }
    cases
}
