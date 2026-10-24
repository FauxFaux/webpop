mod support;

use libwebpop::enc::predictor_enc::*;

fn delta(coefficient: u8, channel: u8) -> i32 {
    (i32::from(coefficient as i8) * i32::from(channel as i8)).div_euclid(32)
}

fn transform_pixel(pixel: u32, code: u32) -> u32 {
    let red = ((pixel >> 16) & 255) as u8;
    let green = (pixel >> 8) as u8;
    let new_red = (i32::from(red) - delta(code as u8, green)).rem_euclid(256) as u32;
    let blue =
        (i32::from(pixel as u8) - delta((code >> 8) as u8, green) - delta((code >> 16) as u8, red))
            .rem_euclid(256) as u32;
    (pixel & 0xff00ff00) | (new_red << 16) | blue
}

#[test]
fn committed_cross_color_fixtures_match_scalar_c() {
    for line in include_str!("fixtures/cross_color.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        let function = words.next().unwrap();
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(support::Case::new(function, input).rust(), expected);
    }
}

#[test]
fn costs_match_independent_byte_histograms_and_neighbor_biases() {
    for case in support::cross_color::leaf_cases()
        .into_iter()
        .filter(|c| matches!(c.input[0], 2 | 3))
        .step_by(13)
    {
        let a = &case.input;
        let blue = a[0] == 3;
        let header = if blue { 8 } else { 7 };
        let mut counts = [0; 256];
        let pixels = &a[header + 256..];
        let code = if blue {
            ((a[7] & 255) << 16) | ((a[6] & 255) << 8)
        } else {
            a[6] & 255
        };
        for y in 0..a[3] as usize {
            for x in 0..a[2] as usize {
                let transformed = transform_pixel(pixels[y * a[1] as usize + x], code);
                let channel = if blue {
                    transformed as u8
                } else {
                    (transformed >> 16) as u8
                };
                counts[channel as usize] += 1;
            }
        }
        let mut cost =
            PredictionCostCrossColor(a[header..header + 256].try_into().unwrap(), &counts);
        let component = if blue { 8 } else { 0 };
        let matches = [a[4], a[5]]
            .into_iter()
            .filter(|p| ((p >> component) & 255) == (a[6] & 255))
            .count();
        cost -= ((matches + usize::from(a[6] == 0)) as i64 * 3) << 23;
        if blue {
            let matches = [a[4], a[5]]
                .into_iter()
                .filter(|p| ((p >> 16) & 255) == (a[7] & 255))
                .count();
            cost -= ((matches + usize::from(a[7] == 0)) as i64 * 3) << 23;
        }
        let actual = case.rust();
        assert_eq!(
            cost,
            (u64::from(actual[0]) | (u64::from(actual[1]) << 32)) as i64
        );
    }
    let zero = libwebpop::dsp::lossless::VP8LMultipliers::default();
    let empty = [0; 256];
    assert_eq!(PredictionCostCrossColor(&empty, &empty), 0);
    // Byte wrapping matches neighbors, but the zero-coefficient bias uses the
    // original signed candidate rather than its wrapped byte representation.
    assert_eq!(
        GetPredictionCostCrossColorRed(&[], 0, 0, 0, zero, zero, 0, &empty),
        -(9i64 << 23)
    );
    assert_eq!(
        GetPredictionCostCrossColorRed(&[], 0, 0, 0, zero, zero, 256, &empty),
        -(6i64 << 23)
    );
    assert_eq!(
        GetPredictionCostCrossColorBlue(&[], 0, 0, 0, zero, zero, 0, 0, &empty),
        -(18i64 << 23)
    );
    assert_eq!(
        GetPredictionCostCrossColorBlue(&[], 0, 0, 0, zero, zero, 256, 256, &empty),
        -(12i64 << 23)
    );
}

#[test]
fn selected_maps_reproduce_transforms_and_invert_all_argb() {
    let mut codes = std::collections::BTreeSet::new();
    let mut mixed_maps = 0;
    let mut optimized = 0;
    let mut transparent = 0;
    let cases = support::cross_color::search_cases();
    for (case_codes, mixed, optimized_count, transparent_count) in
        support::parallel::map(&cases, |case| {
            let mut codes = std::collections::BTreeSet::new();
            let mut mixed_maps = 0;
            let mut optimized = 0;
            let mut transparent = 0;
            let a = &case.input;
            let (width, height) = (a[1] as usize, a[2] as usize);
            let out = case.rust();
            let bits = out[0];
            assert!((a[3]..=9).contains(&bits));
            optimized += usize::from(bits > a[3]);
            let map_width = width.div_ceil(1 << bits);
            let map_size = map_width * height.div_ceil(1 << bits);
            assert_eq!(out.len(), 1 + map_size + width * height);
            let map = &out[1..1 + map_size];
            mixed_maps += usize::from(map.iter().any(|m| *m != map[0]));
            for (i, &pixel) in out[1 + map_size..].iter().enumerate() {
                let code = map[((i / width) >> bits) * map_width + ((i % width) >> bits)];
                assert_eq!(code >> 24, 255);
                codes.insert(code);
                let source = a[5 + i];
                assert_eq!(pixel, transform_pixel(source, code));
                assert_eq!(pixel & 0xff00ff00, source & 0xff00ff00);
                let green = (pixel >> 8) as u8;
                let red = (i32::from((pixel >> 16) as u8) + delta(code as u8, green))
                    .rem_euclid(256) as u8;
                let blue = (i32::from(pixel as u8)
                    + delta((code >> 8) as u8, green)
                    + delta((code >> 16) as u8, red))
                .rem_euclid(256) as u32;
                assert_eq!((pixel & 0xff00ff00) | (u32::from(red) << 16) | blue, source);
                transparent += usize::from(source >> 24 == 0);
            }
            (codes, mixed_maps, optimized, transparent)
        })
    {
        codes.extend(case_codes);
        mixed_maps += mixed;
        optimized += optimized_count;
        transparent += transparent_count;
    }
    assert!(mixed_maps > 0 && optimized > 0 && transparent > 0);
    for shift in [0, 8, 16] {
        assert!(codes.iter().any(|c| ((c >> shift) as u8 as i8) < 0));
        assert!(codes.iter().any(|c| ((c >> shift) as u8 as i8) > 0));
    }
}

#[test]
fn copying_a_tile_preserves_all_other_pixels() {
    for case in support::cross_color::leaf_cases()
        .into_iter()
        .filter(|c| c.input[0] == 6)
    {
        let a = &case.input;
        let (width, height, x, y, size) = (
            a[1] as usize,
            a[2] as usize,
            a[3] as usize,
            a[4] as usize,
            a[5] as usize,
        );
        let actual = case.rust();
        for row in 0..height {
            for col in 0..width {
                let i = row * width + col;
                let expected = if (x..x + size).contains(&col) && (y..y + size).contains(&row) {
                    transform_pixel(a[7 + i], a[6])
                } else {
                    a[7 + i]
                };
                assert_eq!(actual[i], expected);
            }
        }
    }
}

#[test]
fn uniform_zero_coefficients_compact_to_the_largest_tile_size() {
    for pixel in [0, 0xff000000] {
        for bits in 2..=9 {
            let mut pixels = vec![pixel; 513 * 9];
            let (selected, map) = VP8LColorSpaceTransform(513, 9, bits, 100, &mut pixels).unwrap();
            assert_eq!(selected, 9);
            assert_eq!(map, vec![0xff000000; 2]);
            assert!(pixels.iter().all(|p| *p == pixel));
        }
    }
    for byte in 0u32..=255 {
        for shift in [0, 8, 16] {
            let code = byte << shift;
            assert_eq!(
                MultipliersToColorCode(&ColorCodeToMultipliers(code)),
                code | 0xff000000
            );
        }
    }
}

#[test]
fn complete_files_decode_with_all_transform_combinations() {
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    let mut count = 0;
    for case in support::cross_color::transform_cases()
        .into_iter()
        .filter(|c| c.input[0] == 17 && c.input[1] == 19 && c.input[7] == 0)
    {
        let a = &case.input;
        let (width, height, quality, low, exact) = (17usize, 19usize, a[3], a[4] != 0, a[5] != 0);
        for use_palette in [false, true] {
            let palette_size = [2, 4, 16, 17][(quality / 25) as usize % 4];
            let palette: Vec<_> = (0..palette_size)
                .map(|i| {
                    if i % 3 == 0 {
                        i * 65793
                    } else {
                        0xff000000 | (i * 65793)
                    }
                })
                .collect();
            let mut source = a[8..].to_vec();
            if use_palette {
                for p in &mut source {
                    *p = palette[*p as usize % palette.len()];
                }
            }
            let mut pixels = source.clone();
            let mut current_width = width;
            let mut bw = VP8LBitWriterInit(0).unwrap();
            WriteImageSize(&mut bw, width as u32, height as u32).unwrap();
            WriteRealAlphaAndVersion(&mut bw, source.iter().any(|p| p >> 24 != 255)).unwrap();
            let mut chain = VP8LHashChainInit(width * height).unwrap();
            let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
            if use_palette {
                EncodePalette(&mut bw, &palette, &mut chain, &mut refs[..2], low).unwrap();
                (current_width, pixels) =
                    MapImageFromPalette(&source, width, &palette, width, height).unwrap();
            }
            if a[6] & 1 != 0 {
                ApplySubtractGreen(&mut bw, &mut pixels).unwrap();
            }
            if a[6] & 2 != 0 {
                ApplyPredictFilter(
                    &mut bw,
                    &mut pixels,
                    &mut chain,
                    &mut refs[..2],
                    current_width,
                    height,
                    6,
                    6,
                    quality,
                    low,
                    100,
                    exact,
                    false,
                )
                .unwrap();
            }
            ApplyCrossColorFilter(
                &mut bw,
                &mut pixels,
                &mut chain,
                &mut refs[..2],
                current_width,
                height,
                a[2],
                quality,
                low,
            )
            .unwrap();
            VP8LPutBits(&mut bw, 0, 1).unwrap();
            let initial = VP8LBitWriterNumBytes(&bw);
            let mut cache = 0;
            EncodeImageInternal(
                &mut bw,
                &pixels,
                &mut chain,
                &mut refs,
                current_width,
                height,
                quality,
                low,
                &[CrunchSubConfig {
                    lz77: kLZ77Standard | kLZ77RLE,
                    do_no_cache: false,
                }],
                &mut cache,
                2,
                initial,
            )
            .unwrap();
            let file = WriteImage(&mut bw).unwrap();
            let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
                .unwrap()
                .to_rgba8();
            assert_eq!(decoded.dimensions(), (width as u32, height as u32));
            for (actual, &pixel) in decoded.pixels().zip(&source) {
                assert_eq!(actual.0[3], (pixel >> 24) as u8);
                if pixel >> 24 != 0 || exact || low || use_palette || a[6] & 2 == 0 {
                    assert_eq!(
                        actual.0,
                        [
                            (pixel >> 16) as u8,
                            (pixel >> 8) as u8,
                            pixel as u8,
                            (pixel >> 24) as u8
                        ]
                    );
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 768);
}
