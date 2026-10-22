use super::{Case, SEED, random};
use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
use libwebpop::utils::bit_writer_utils::*;

pub fn pixels(case: &Case) -> &[u32] {
    &case.input[8 + 2 * case.input[7] as usize..]
}

pub fn encode(case: &Case, bw: &mut VP8LBitWriter, initial: usize) -> (u32, usize, usize) {
    let a = &case.input;
    let configs: Vec<_> = a[8..8 + 2 * a[7] as usize]
        .chunks_exact(2)
        .map(|p| CrunchSubConfig {
            lz77: p[0],
            do_no_cache: p[1] != 0,
        })
        .collect();
    let mut chain = VP8LHashChainInit((a[0] * a[1]) as usize).unwrap();
    let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
    let mut cache = a[4];
    let (header, data) = EncodeImageInternal(
        bw,
        pixels(case),
        &mut chain,
        &mut refs,
        a[0] as usize,
        a[1] as usize,
        a[2],
        a[3] != 0,
        &configs,
        &mut cache,
        a[5],
        initial,
    )
    .unwrap();
    (cache, header, data)
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut remaining = case.input[6];
    while remaining > 0 {
        let bits = remaining.min(32);
        let mask = u32::MAX >> (32 - bits);
        VP8LPutBits(&mut bw, 0x05ad5ad5 & mask, bits).unwrap();
        remaining -= bits;
    }
    let initial = VP8LBitWriterNumBytes(&bw);
    let (cache, header, data) = encode(case, &mut bw, initial);
    assert_eq!(header + data, VP8LBitWriterNumBytes(&bw) - initial);
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    let mut out = vec![cache, header as u32, data as u32];
    out.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&v| u32::from(v)),
    );
    out
}

pub fn case(
    w: u32,
    h: u32,
    pattern: u32,
    quality: u32,
    low: bool,
    config: u32,
    prefix: u32,
) -> Case {
    let bits = if w < 100 { 2 } else { 4 };
    let mut a = vec![
        w,
        h,
        quality,
        u32::from(low),
        if config == 4 { 4 } else { 0 },
        bits,
        prefix,
    ];
    match config {
        0 => a.extend([1, 3, 0]),
        1 => a.extend([1, 7, 1]),
        2 => a.extend([2, 1, 1, 4, 1]),
        3 => a.extend([2, 2, 0, 1, 0]),
        _ => a.extend([1, 1, 1]),
    }
    let mut seed = SEED;
    for y in 0..h {
        for x in 0..w {
            let r = random(&mut seed);
            let region = (x / 16 + y / 16 * w.div_ceil(16)) % 5;
            let p = match pattern {
                0 => 0xff123456,
                1 => r,
                2 => [0xff123456, 0x80123456, 0x00123456, 0xffffffff][r as usize % 4],
                3 => 0xff000000 | ((x * 3) << 16) | ((y * 7) << 8) | ((x + y) & 255),
                4 => (region * 0x30303030) | (r % 4 * 0x10101),
                5 => ((x / 7 + y / 5) % 3) * 0x55555555,
                _ => {
                    if (x / 16 + y / 16) % 2 == 0 {
                        r
                    } else {
                        0xff000000 | (r % 2 * 0x10101)
                    }
                }
            };
            a.push(p);
        }
    }
    Case::new("EncodeImageInternal", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for pattern in 0..7 {
        out.push(case(17, 19, pattern, 70, false, pattern % 5, 63));
    }
    for (pattern, quality, config) in [(2, 100, 2), (6, 100, 1), (4, 70, 3)] {
        out.push(case(65, 67, pattern, quality, false, config, 31));
    }
    out.push(case(1, 1, 0, 0, true, 0, 7));
    out
}

pub fn cases() -> Vec<Case> {
    let mut out = fixture_cases();
    for (w, h) in [(1, 1), (1, 47), (43, 1), (17, 19), (65, 67), (129, 33)] {
        for pattern in 0..7 {
            for (quality, low) in [
                (0, false),
                (25, false),
                (70, false),
                (100, false),
                (70, true),
            ] {
                for config in 0..5 {
                    let prefix = [0, 7, 31, 63, 97][(pattern as usize + config as usize) % 5];
                    out.push(case(w, h, pattern, quality, low, config, prefix));
                }
            }
        }
    }
    // Cross the rest of the legal tile-bit range on irregular geometry.
    for bits in 3..=9 {
        let mut c = case(65, 67, 6, 100, false, 2, 97);
        c.input[5] = bits;
        out.push(c);
    }
    out
}
