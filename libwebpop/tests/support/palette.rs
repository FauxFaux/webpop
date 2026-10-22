use super::Case;
use libwebpop::utils::palette::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    if a[0] == 3 {
        let mut palette = [0; 256];
        let count = GetColorPalette(
            &a[4..],
            a[1] as usize,
            a[2] as usize,
            a[3] as usize,
            &mut palette,
        );
        let mut out = vec![count as u32];
        if count <= 256 {
            out.extend_from_slice(&palette[..count]);
        }
        return out;
    }
    let palette = &a[1..];
    let mut out = vec![0; palette.len()];
    match a[0] {
        0 => {
            let mut map = out.clone();
            PrepareMapToPalette(palette, &mut out, &mut map);
            out.extend(map);
        }
        1 => PaletteSortDefault(palette, &mut out),
        2 => PaletteSortMinimizeDeltas(palette, &mut out),
        _ => unreachable!(),
    }
    out
}

pub fn cases() -> Vec<Case> {
    let mut state = super::SEED;
    let mut cases = Vec::new();
    for n in [1usize, 2, 3, 4, 5, 16, 17, 18, 224, 225, 256, 257] {
        for pattern in 0..8 {
            let mut colors = Vec::new();
            for i in 0..n {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                colors.push(match pattern {
                    0 => i as u32,
                    1 => (i as u32) * 0x010101,
                    2 => (i as u32) << 24,
                    _ => state,
                });
            }
            colors.sort_unstable();
            colors.dedup();
            // Keep zero present to force the >17 transparent-color branch.
            if pattern == 3 {
                colors[0] = 0;
            }
            if n <= 256 {
                for mode in [1, 2, 0] {
                    let mut input = vec![mode];
                    if mode == 0 {
                        colors.reverse();
                    }
                    input.extend_from_slice(&colors);
                    cases.push(Case::new("PaletteLeaves", input));
                    if mode == 0 {
                        colors.reverse();
                    }
                }
            }
            for width in [1, 7, n] {
                let height = n.div_ceil(width) + 1;
                let stride = width + 3;
                let mut input = vec![3, width as u32, height as u32, stride as u32];
                for y in 0..height {
                    for x in 0..stride {
                        input.push(if x < width {
                            colors[(y * width + x) % colors.len()]
                        } else {
                            0xdeadbeef
                        });
                    }
                }
                input.truncate(4 + (height - 1) * stride + width);
                cases.push(Case::new("PaletteLeaves", input));
            }
        }
    }
    cases
}

pub fn spatial_evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let n = a[4] as usize;
    let method = match a[0] {
        0 => PaletteSorting::kSortedDefault,
        1 => PaletteSorting::kMinimizeDelta,
        2 => PaletteSorting::kModifiedZeng,
        3 => PaletteSorting::kMinLAFromZeng,
        4 => PaletteSorting::kMinLAFromDelta,
        _ => unreachable!(),
    };
    let mut out = vec![0; n];
    PaletteSort(
        method,
        &a[5 + n..],
        a[1] as usize,
        a[2] as usize,
        a[3] as usize,
        &a[5..5 + n],
        &mut out,
    )
    .unwrap();
    out
}

pub fn spatial_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut state = super::SEED;
    for n in [1usize, 2, 3, 4, 5, 16, 17, 18, 32, 64, 224, 225, 256] {
        for width in [1, 7, n] {
            for pattern in 0..6 {
                let height = (n * 3).div_ceil(width) + 1;
                let stride = width + 2;
                let colors: Vec<u32> = (0..n)
                    .map(|i| {
                        if i == 0 {
                            0
                        } else {
                            0xff000000 | (i as u32 * 65793)
                        }
                    })
                    .collect();
                let mut pixels = Vec::new();
                for y in 0..height {
                    for x in 0..stride {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        let pos = y * width + x;
                        // Ensure every palette color occurs before varying adjacency.
                        let index = if pos < n {
                            pos
                        } else {
                            match pattern {
                                0 => pos % n,
                                1 => (pos / 3) % n,
                                2 => (x + y) % n,
                                3 => (x * y) % n,
                                4 => (state as usize) % n,
                                _ => ((state as usize) % 3).min(n - 1),
                            }
                        };
                        pixels.push(if x < width { colors[index] } else { 0xdeadbeef });
                    }
                }
                pixels.truncate((height - 1) * stride + width);
                for method in 0..5 {
                    let mut input =
                        vec![method, width as u32, height as u32, stride as u32, n as u32];
                    input.extend_from_slice(&colors);
                    input.extend_from_slice(&pixels);
                    cases.push(Case::new("PaletteSpatial", input));
                }
            }
        }
    }
    cases
}

pub fn transform_evaluate(case: &Case) -> Vec<u32> {
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    let a = &case.input;
    let n = a[0] as usize;
    let palette = &a[6..6 + n];
    let (width, pixels) = MapImageFromPalette(
        &a[6 + n..],
        a[3] as usize,
        palette,
        a[1] as usize,
        a[2] as usize,
    )
    .unwrap();
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut remaining = a[5];
    while remaining > 0 {
        let bits = remaining.min(32);
        VP8LPutBits(&mut bw, 0x05ad5ad5 & (u32::MAX >> (32 - bits)), bits).unwrap();
        remaining -= bits;
    }
    let mut chain = VP8LHashChainInit(n).unwrap();
    let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
    EncodePalette(&mut bw, palette, &mut chain, &mut refs, a[4] != 0).unwrap();
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    let mut out = vec![width as u32];
    out.extend(pixels);
    out.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&v| u32::from(v)),
    );
    out
}

pub fn transform_cases() -> Vec<Case> {
    let mut palettes = Vec::new();
    for n in [1usize, 2, 3, 4, 5, 16, 17, 18, 224, 225, 256] {
        let mut colors: Vec<_> = (0..n).map(|i| 0xff000000 | (i as u32 * 65793)).collect();
        palettes.push(colors.clone());
        colors[0] = 0;
        colors.rotate_left(1); // Force the trailing-zero omission threshold.
        palettes.push(colors);
    }
    palettes.extend([
        vec![0xff000001, 0xff000002, 0xff000003, 0xff000004], // Hash 1.
        vec![0xff000001, 0xff05009e, 0xff400001, 0xff800001], // Hash 2.
        vec![0x00123456, 0x01123456, 0x80123456, 0xff123456], // Search fallback.
    ]);
    let mut cases = Vec::new();
    for palette in palettes {
        for (width, height) in [(1, 19), (7, 3), (17, 5), (palette.len() + 1, 2)] {
            for low in 0..=1 {
                for prefix in [0, 5, 97] {
                    let stride = width + 2;
                    let mut input = vec![
                        palette.len() as u32,
                        width as u32,
                        height as u32,
                        stride as u32,
                        low,
                        prefix,
                    ];
                    input.extend_from_slice(&palette);
                    for y in 0..height {
                        for x in 0..stride {
                            let index = (y * width + x) / 3 % palette.len();
                            input.push(if x < width {
                                palette[index]
                            } else {
                                0xdeadbeef
                            });
                        }
                    }
                    input.truncate(6 + palette.len() + (height - 1) * stride + width);
                    cases.push(Case::new("PaletteTransform", input));
                }
            }
        }
    }
    cases
}
