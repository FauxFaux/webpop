mod support;

use libwebpop::internal::enc::vp8l_enc::*;
use libwebpop::internal::utils::palette::PaletteSorting;

#[test]
fn committed_analysis_fixtures_match_scalar_c() {
    let cases = support::analysis::fixture_cases();
    let lines: Vec<_> = include_str!("fixtures/analysis.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .collect();
    assert_eq!(lines.len(), cases.len());
    for (line, case) in lines.into_iter().zip(cases) {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        let function = words.next().unwrap();
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        assert_eq!(function, case.function);
        assert_eq!(input, case.input);
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(case.rust(), expected);
    }
}

#[test]
fn first_pixel_and_entropy_ties_are_preserved() {
    use EntropyIx::*;
    // Every pixel is skipped by the histogram loop. The initial color still
    // determines whether subtract-green can be omitted in exhaustive trials.
    for (color, zero) in [
        (0, true),
        (0xff00ff00, true),
        (0xff010000, false),
        (0xff000001, false),
        (0xff123456, false),
    ] {
        let pixels = [color; 63];
        let result = AnalyzeEntropy(&pixels, 9, 7, 9, None, 5).unwrap();
        assert_eq!(result.min_entropy_ix, kDirect);
        assert_eq!(result.red_and_blue_always_zero, zero);
        assert!(result.red_and_blue_transformed_always_zero);
        let result = EncoderAnalyze(&pixels, 9, 7, 9, 6, 100).unwrap();
        let modes: Vec<_> = result
            .crunch_configs
            .iter()
            .map(|c| c.entropy_idx)
            .collect();
        assert_eq!(modes.contains(&kSubGreen), !zero);
        assert_eq!(modes.contains(&kSpatialSubGreen), !zero);
    }
}

#[test]
fn small_palettes_force_packing_and_preserve_reference_trials() {
    use EntropyIx::*;
    use PaletteSorting::*;
    for n in [1, 2, 3, 4, 5, 16] {
        let palette: Vec<_> = (0..n).map(|i| 0xff000000 | i * 0x010307).collect();
        let image: Vec<_> = (0..513).map(|i| palette[i % n as usize]).collect();
        let entropy = AnalyzeEntropy(&image, 19, 27, 19, Some(&palette), 2).unwrap();
        assert_eq!(entropy.min_entropy_ix, kPalette);
        assert!(entropy.red_and_blue_transformed_always_zero);
        let result = EncoderAnalyze(&image, 19, 27, 19, 4, 70).unwrap();
        let trials: Vec<_> = result
            .crunch_configs
            .iter()
            .map(|c| (c.entropy_idx, c.palette_sorting_type))
            .collect();
        assert_eq!(
            trials,
            [
                (kPalette, Some(kMinimizeDelta)),
                (kPaletteAndSpatial, Some(kMinimizeDelta)),
                (kPaletteAndSpatial, Some(kMinLAFromZeng)),
                (kPaletteAndSpatial, Some(kMinLAFromDelta))
            ]
        );
        for config in result.crunch_configs {
            assert_eq!(config.sub_configs_size, 2);
            assert_eq!(config.sub_configs[0].lz77, 3);
            assert_eq!(config.sub_configs[1].lz77, 4);
            assert!(config.sub_configs.iter().all(|s| !s.do_no_cache));
        }
        let low = EncoderAnalyze(&image, 19, 27, 19, 0, 100).unwrap();
        assert_eq!(low.crunch_configs.len(), 1);
        assert_eq!(
            low.crunch_configs[0].palette_sorting_type,
            Some(kSortedDefault)
        );
        assert_eq!(low.crunch_configs[0].sub_configs_size, 1);
    }
}

#[test]
fn padding_and_previous_skipped_pixels_do_not_change_analysis() {
    // Padding deliberately contains extra colors; it must affect neither
    // palette eligibility nor row-to-row entropy prediction.
    for pattern in 0..36 {
        let padded = support::analysis::pixels(19, 17, 22, pattern);
        let packed: Vec<_> = (0..17)
            .flat_map(|y| padded[y * 22..y * 22 + 19].iter().copied())
            .collect();
        for method in [0, 4, 5, 6] {
            let a = EncoderAnalyze(&padded, 19, 17, 22, method, 100).unwrap();
            let b = EncoderAnalyze(&packed, 19, 17, 19, method, 100).unwrap();
            assert_eq!(a.palette_size, b.palette_size);
            assert_eq!(
                &a.palette_sorted[..a.palette_size],
                &b.palette_sorted[..b.palette_size]
            );
            assert_eq!(a.crunch_configs, b.crunch_configs);
            assert_eq!(
                a.red_and_blue_transformed_always_zero,
                b.red_and_blue_transformed_always_zero
            );
        }
    }
}

#[test]
fn sampling_sizes_respect_limits_and_single_tile_compaction() {
    for case in support::analysis::sizing_cases() {
        let a = &case.input;
        let (w, h) = (a[0] as usize, a[1] as usize);
        let bits = GetHistoBits(a[2], a[3] != 0, w, h);
        let size = |b| w.div_ceil(1usize << b) * h.div_ceil(1usize << b);
        assert!((2..=9).contains(&bits));
        assert!(size(bits) <= 2600);
        if size(bits) == 1 && bits > 2 {
            assert!(size(bits - 1) > 1);
        }
        let transform = GetTransformBits(a[2], bits);
        assert!(transform <= bits && (2..=6).contains(&transform));
    }
}
