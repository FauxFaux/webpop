mod support;

#[test]
fn committed_predictor_fixtures_match_scalar_c() {
    for line in include_str!("fixtures/predictors.txt")
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
fn residuals_reconstruct_pixels_and_preserve_visible_channels() {
    use libwebpop::dsp::{lossless::VP8LPredictors, lossless_common::VP8LAddPixels};
    let mut cleaned = 0;
    let mut modes_used = [false; 14];
    for case in support::predictors::cases()
        .into_iter()
        .filter(|c| c.input[0] == 2)
    {
        let a = &case.input;
        let (w, h, bits, low, exact) = (a[1] as usize, a[2] as usize, a[3], a[4] != 0, a[5] != 0);
        let tiles = w.div_ceil(1 << bits);
        let map_size = tiles * h.div_ceil(1 << bits);
        let original = &a[6 + map_size..];
        let residuals = case.rust();
        let mut decoded = vec![0u32; w * h];
        for y in 0..h {
            for x in 0..w {
                let mode = if low {
                    11
                } else {
                    ((a[6 + (y >> bits) * tiles + (x >> bits)] >> 8) & 255) as usize
                };
                modes_used[mode] = true;
                let prediction = if y == 0 {
                    if x == 0 {
                        0xff000000
                    } else {
                        decoded[y * w + x - 1]
                    }
                } else if x == 0 {
                    decoded[(y - 1) * w]
                } else {
                    let top_right = if x + 1 < w {
                        decoded[(y - 1) * w + x + 1]
                    } else {
                        decoded[y * w]
                    };
                    VP8LPredictors[mode](
                        decoded[y * w + x - 1],
                        &[
                            decoded[(y - 1) * w + x - 1],
                            decoded[(y - 1) * w + x],
                            top_right,
                        ],
                    )
                };
                let p = VP8LAddPixels(prediction, residuals[y * w + x]);
                decoded[y * w + x] = p;
                let input = original[y * w + x];
                assert_eq!(p >> 24, input >> 24);
                if exact || low || input >> 24 != 0 {
                    assert_eq!(p, input);
                } else {
                    assert_eq!(residuals[y * w + x] & 0xffffff, 0);
                    cleaned += usize::from(p != input);
                }
            }
        }
    }
    assert!(cleaned > 0);
    assert!(modes_used.into_iter().all(|used| used));
}

#[test]
fn constant_planes_and_transparency_control_eligible_modes() {
    use libwebpop::enc::predictor_enc::*;
    for mask in 0..16 {
        let mut delta = 0;
        for plane in 0..4 {
            if mask & (1 << plane) != 0 {
                delta |= 1 << ((3 - plane) * 8);
            }
        }
        assert_eq!(
            GetPlaneDiffMask(&[0xff000000, 0xff000000 ^ delta], 1, true),
            (mask, 0)
        );
        assert_eq!(
            GetPlaneDiffMask(&[0xff123456, 0xff123456 ^ delta], 1, true),
            (mask, u32::from(mask & 14 != 14))
        );
    }
    assert_eq!(
        GetPlaneDiffMask(&[0x00123456, 0x00789abc], 1, false),
        (0, 1)
    );
    assert_eq!(
        GetPlaneDiffMask(&[0x00123456, 0xff123456], 1, false),
        (15, 0)
    );
    assert_eq!(GetPlaneDiffMask(&[0, 0xff000000], 1, false), (1, 0));
    assert_eq!(GetPlaneDiffMask(&[0xff123456], 2, false), (15, 0));
    let zero = [0; 1024];
    assert_eq!(
        PredictionCostSpatialHistogram(&zero, &zero, 11, 11, 11, 0),
        -(30i64 << 23)
    );
    assert_eq!(PredictionCostBias(&[0; 256], 1, 94), 0);
    let mut counts = [0; 256];
    counts[0] = 10;
    assert_eq!(PredictionCostBias(&counts, 1, 94), -(1i64 << 23));
}

#[test]
fn active_plane_histograms_match_byte_counts() {
    use libwebpop::enc::predictor_enc::UpdateHisto;
    let residuals = [0, 0xffffffff, 0x12345678, 0x80808080, 0x12345678];
    for mask in 0..16 {
        let mut actual = [7; 1024];
        UpdateHisto(&mut actual, &residuals, mask);
        for plane in 0..4 {
            for value in 0..256 {
                let count = residuals
                    .iter()
                    .filter(|pixel| pixel.to_be_bytes()[plane] == value as u8)
                    .count() as u32;
                assert_eq!(
                    actual[plane * 256 + value],
                    7 + if mask & (1 << plane) != 0 { count } else { 0 }
                );
            }
        }
    }
}

#[test]
fn automatic_predictors_reconstruct_pixels() {
    use libwebpop::dsp::{lossless::VP8LPredictors, lossless_common::VP8LAddPixels};
    let mut selected_bits = std::collections::BTreeSet::new();
    let mut selected_modes = std::collections::BTreeSet::new();
    let mut mixed_maps = 0;
    let mut cleaned = 0;
    for case in support::predictors::search_cases()
        .into_iter()
        .filter(|c| c.input[0] == 6)
    {
        let a = &case.input;
        let (w, h, low, exact) = (a[1] as usize, a[2] as usize, a[5] != 0, a[6] != 0);
        let out = case.rust();
        let bits = out[0];
        assert!((a[3]..=9).contains(&bits));
        let map_width = w.div_ceil(1 << bits);
        let map_size = map_width * h.div_ceil(1 << bits);
        assert_eq!(out.len(), 1 + map_size + w * h);
        let map = &out[1..1 + map_size];
        let residuals = &out[1 + map_size..];
        selected_bits.insert(bits);
        mixed_maps += usize::from(map.iter().any(|m| *m != map[0]));
        if low {
            assert_eq!(bits, a[4]);
            assert!(map.iter().all(|m| *m == 0xff000b00));
        }
        let mut decoded = vec![0; w * h];
        for y in 0..h {
            for x in 0..w {
                let mode_pixel = map[(y >> bits) * map_width + (x >> bits)];
                let mode = ((mode_pixel >> 8) & 255) as usize;
                assert!(mode < 14);
                assert_eq!(mode_pixel, 0xff000000 | ((mode as u32) << 8));
                selected_modes.insert(mode);
                let predict = if y == 0 {
                    if x == 0 {
                        0xff000000
                    } else {
                        decoded[y * w + x - 1]
                    }
                } else if x == 0 {
                    decoded[(y - 1) * w]
                } else {
                    let top_right = if x + 1 < w {
                        decoded[(y - 1) * w + x + 1]
                    } else {
                        decoded[y * w]
                    };
                    VP8LPredictors[mode](
                        decoded[y * w + x - 1],
                        &[
                            decoded[(y - 1) * w + x - 1],
                            decoded[(y - 1) * w + x],
                            top_right,
                        ],
                    )
                };
                let i = y * w + x;
                decoded[i] = VP8LAddPixels(predict, residuals[i]);
                let source = a[7 + i];
                assert_eq!(decoded[i] >> 24, source >> 24);
                if exact || low || source >> 24 != 0 {
                    assert_eq!(decoded[i], source);
                } else {
                    assert_eq!(residuals[i] & 0xffffff, 0);
                    cleaned += usize::from(decoded[i] != source);
                }
            }
        }
    }
    assert!(selected_bits.contains(&2) && selected_bits.contains(&9));
    assert!(selected_modes.len() >= 10);
    assert!(mixed_maps > 0 && cleaned > 0);
}

#[test]
fn uniform_predictor_ties_compact_to_the_largest_tile_size() {
    use libwebpop::enc::predictor_enc::*;
    for (pixel, exact, expected_mode) in [
        (0xff000000, true, 0),
        (0xff123456, true, 1),
        (0x00123456, false, 1),
    ] {
        for (w, h) in [(1, 1), (17, 19), (513, 9)] {
            let source = vec![pixel; w * h];
            for (min, max) in [(2, 2), (2, 9), (7, 8)] {
                let (bits, map) =
                    GetBestPredictorsAndSubSampling(w, h, min, max, &source, exact).unwrap();
                assert_eq!(bits, 9);
                assert_eq!(
                    map,
                    vec![0xff000000 | (expected_mode << 8); w.div_ceil(512) * h.div_ceil(512)]
                );
            }
        }
    }
}

#[test]
fn clamping_matches_an_exhaustive_auxiliary_size_search() {
    use libwebpop::enc::vp8l_enc::ClampBits;
    for case in support::predictors::clamp_cases() {
        let a = &case.input;
        let count =
            |bits: u32| (a[0] as usize).div_ceil(1 << bits) * (a[1] as usize).div_ceil(1 << bits);
        let start = (a[2] as i32).clamp(a[3] as i32, a[4] as i32) as u32;
        let limited = (start..=a[4])
            .find(|&b| count(b) <= a[5] as usize)
            .unwrap_or(a[4]);
        let expected = if count(limited) == 1 {
            (a[3]..=limited).find(|&b| count(b) == 1).unwrap()
        } else {
            limited
        };
        assert_eq!(
            ClampBits(
                a[0] as usize,
                a[1] as usize,
                a[2] as i32,
                a[3],
                a[4],
                a[5] as usize
            ),
            expected
        );
    }
    assert_eq!(ClampBits(513, 513, 2, 2, 9, 1 << 14), 3);
    assert_eq!(ClampBits(17, 19, 9, 2, 9, 1 << 14), 5);
}

#[test]
fn production_predictor_writer_decodes_with_preceding_transforms() {
    use libwebpop::dsp::lossless_enc::VP8LSubtractGreenFromBlueAndRed;
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    let mut count = 0;
    for case in support::predictors::transform_cases()
        .into_iter()
        .filter(|c| c.input[0] == 17 && c.input[1] == 19 && c.input[7] == 0)
    {
        let a = &case.input;
        let (width, height, quality, low, exact) = (17usize, 19usize, a[4], a[5] != 0, a[6] != 0);
        for preceding in 0..3 {
            let mut source = a[8..].to_vec();
            // A small palette with hidden RGB exercises packing before the
            // predictor, including the changed current width and map geometry.
            let palette_size = [2, 4, 16, 17][(quality / 25) as usize % 4];
            let palette: Vec<_> = (0..palette_size)
                .map(|i| {
                    let rgb = i * 65793;
                    if i % 3 == 0 { rgb } else { 0xff000000 | rgb }
                })
                .collect();
            if preceding == 2 {
                for pixel in &mut source {
                    *pixel = palette[(*pixel as usize) % palette.len()];
                }
            }
            let mut pixels = source.clone();
            let mut current_width = width;
            let mut bw = VP8LBitWriterInit(0).unwrap();
            WriteImageSize(&mut bw, width as u32, height as u32).unwrap();
            WriteRealAlphaAndVersion(&mut bw, source.iter().any(|p| p >> 24 != 255)).unwrap();
            let mut chain = VP8LHashChainInit(width * height).unwrap();
            let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
            if preceding == 1 {
                VP8LPutBits(&mut bw, 1, 1).unwrap();
                VP8LPutBits(&mut bw, 2, 2).unwrap(); // SUBTRACT_GREEN_TRANSFORM
                VP8LSubtractGreenFromBlueAndRed(&mut pixels);
            } else if preceding == 2 {
                (current_width, pixels) =
                    MapImageFromPalette(&source, width, &palette, width, height).unwrap();
                EncodePalette(&mut bw, &palette, &mut chain, &mut refs[..2], low).unwrap();
            }
            ApplyPredictFilter(
                &mut bw,
                &mut pixels,
                &mut chain,
                &mut refs[..2],
                current_width,
                height,
                a[2] as i32,
                a[3],
                quality,
                low,
                exact,
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
                if pixel >> 24 != 0 || low || exact || preceding == 2 {
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
    assert_eq!(count, 576);
}

#[test]
fn selected_and_supplied_predictor_maps_decode_in_complete_files() {
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    for case in support::predictors::cases()
        .into_iter()
        .chain(support::predictors::search_cases())
        .filter(|c| {
            c.input[1] == 17
                && c.input[2] == 19
                && ((c.input[0] == 2 && c.input[3] == 2)
                    || (c.input[0] == 6 && c.input[3] == 2 && matches!(c.input[4], 6 | 9)))
        })
    {
        let a = &case.input;
        let (w, h) = (17usize, 19usize);
        let (bits, modes, source, residuals, low, exact) = if a[0] == 2 {
            let size = w.div_ceil(4) * h.div_ceil(4);
            let modes = if a[4] != 0 {
                vec![0xff000b00; size]
            } else {
                a[6..6 + size].to_vec()
            };
            (2, modes, &a[6 + size..], case.rust(), a[4] != 0, a[5] != 0)
        } else {
            let out = case.rust();
            let bits = out[0];
            let size = w.div_ceil(1 << bits) * h.div_ceil(1 << bits);
            (
                bits,
                out[1..1 + size].to_vec(),
                &a[7..],
                out[1 + size..].to_vec(),
                a[5] != 0,
                a[6] != 0,
            )
        };
        let map_w = w.div_ceil(1 << bits);
        let map_h = h.div_ceil(1 << bits);
        let mut bw = VP8LBitWriterInit(0).unwrap();
        WriteImageSize(&mut bw, w as u32, h as u32).unwrap();
        WriteRealAlphaAndVersion(&mut bw, source.iter().any(|p| p >> 24 != 255)).unwrap();
        // Supplied/selected maps exercise the wire format directly; the test
        // above also covers automatic selection through the production writer.
        VP8LPutBits(&mut bw, 1, 1).unwrap();
        VP8LPutBits(&mut bw, 0, 2).unwrap();
        VP8LPutBits(&mut bw, bits - 2, 3).unwrap();
        let mut chain = VP8LHashChainInit(w * h).unwrap();
        let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
        EncodeImageNoHuffman(
            &mut bw,
            &modes,
            &mut chain,
            &mut refs[..2],
            map_w,
            map_h,
            70,
            low,
        )
        .unwrap();
        VP8LPutBits(&mut bw, 0, 1).unwrap();
        let initial = VP8LBitWriterNumBytes(&bw);
        let mut cache = 0;
        EncodeImageInternal(
            &mut bw,
            &residuals,
            &mut chain,
            &mut refs,
            w,
            h,
            70,
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
        assert_eq!(decoded.dimensions(), (w as u32, h as u32));
        for (actual, &pixel) in decoded.pixels().zip(source) {
            assert_eq!(actual.0[3], (pixel >> 24) as u8);
            if pixel >> 24 != 0 || low || exact {
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
    }
}
