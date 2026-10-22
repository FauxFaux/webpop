//! Direct, synthetic token serialization cases; distances are already plane codes.
use super::Case;
use libwebpop::enc::{backward_references_enc::*, histogram_enc::*, vp8l_enc::*};
use libwebpop::utils::bit_writer_utils::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let (width, bits, cache, groups, prefix, tiles) = (
        a[0] as usize,
        a[2],
        a[3],
        a[4] as usize,
        a[5],
        a[6] as usize,
    );
    let symbols = &a[7..7 + tiles];
    let mut histograms: Vec<_> = (0..groups)
        .map(|_| VP8LAllocateHistogram(cache).unwrap())
        .collect();
    let mut refs = VP8LBackwardRefsInit(256);
    let mut position = 0usize;
    let tile_width = if bits == 0 {
        1
    } else {
        width.div_ceil(1 << bits)
    };
    for t in a[7 + tiles..].chunks_exact(3) {
        let v = match t[0] {
            0 => PixOrCopyCreateLiteral(t[1]),
            1 => PixOrCopyCreateCacheIdx(t[1]),
            2 => PixOrCopyCreateCopy(t[1], t[2] as u16),
            _ => unreachable!(),
        };
        let tile = if bits == 0 {
            0
        } else {
            ((position / width) >> bits) * tile_width + ((position % width) >> bits)
        };
        let group = ((symbols[tile] >> 8) & 0xffff) as usize;
        HistogramAddSinglePixOrCopy(&mut histograms[group], &v, None, 0);
        position += PixOrCopyLength(&v) as usize;
        VP8LBackwardRefsCursorAdd(&mut refs, v).unwrap();
    }
    assert_eq!(position, width * a[1] as usize);
    let mut codes = GetHuffBitLengthsAndCodes(&mut histograms).unwrap();
    let mut output = Vec::new();
    for code in &codes {
        output.push(code.code_lengths.len() as u32);
        for (&length, &symbol) in code.code_lengths.iter().zip(&code.codes) {
            output.push(u32::from(length) | (u32::from(symbol) << 8));
        }
    }
    let mut bw = VP8LBitWriterInit(0).unwrap();
    VP8LPutBits(&mut bw, 5 & ((1u32 << prefix) - 1), prefix).unwrap();
    for code in &mut codes {
        StoreHuffmanCode(&mut bw, code).unwrap();
        ClearHuffmanTreeIfOnlyOneSymbol(code);
    }
    StoreImageToBitMask(&mut bw, width, bits, &refs, symbols, &codes).unwrap();
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    output.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&v| u32::from(v)),
    );
    output
}

pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    let mut seed = super::SEED;
    for (width, height) in [(1usize, 4103usize), (17, 33), (65, 67)] {
        for bits in [0, 2, 4] {
            for cache in [0, 1, 4, 10] {
                for prefix in [0, 7, 31] {
                    for pattern in 0..3 {
                        let groups = if bits == 0 { 1 } else { 4 };
                        let tiles = if bits == 0 {
                            1
                        } else {
                            width.div_ceil(1 << bits) * height.div_ceil(1 << bits)
                        };
                        let mut a = vec![
                            width as u32,
                            height as u32,
                            bits,
                            cache,
                            groups,
                            prefix,
                            tiles as u32,
                        ];
                        a.extend(
                            (0..tiles)
                                .map(|i| 0xa500004b | (((i * 3 + i / 5) as u32 % groups) << 8)),
                        );
                        let mut position = 0;
                        while position < width * height {
                            seed ^= seed << 13;
                            seed ^= seed >> 17;
                            seed ^= seed << 5;
                            let remaining = width * height - position;
                            let (mode, value, length) = if pattern == 0 {
                                (0, 0xff123456, 1)
                            } else if cache > 0 && position % 3 == 1 {
                                (1, seed & ((1 << cache) - 1), 1)
                            } else if position % 3 == 2 && remaining >= 2 {
                                let length = [2, 3, 4, 7, 8, 17, 257, 4095]
                                    [(seed as usize >> 8) % 8]
                                    .min(remaining);
                                (
                                    2,
                                    [1, 2, 120, 121, 256, 524288, 1048575][seed as usize % 7],
                                    length,
                                )
                            } else {
                                (
                                    0,
                                    if pattern == 1 {
                                        seed
                                    } else {
                                        seed % 3 * 0x55555555
                                    },
                                    1,
                                )
                            };
                            a.extend([mode, value, length as u32]);
                            position += length;
                        }
                        result.push(Case::new("TokenStream", a));
                    }
                }
            }
        }
    }
    result
}
