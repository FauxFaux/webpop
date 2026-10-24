mod support;

#[test]
fn committed_palette_fixtures_match_scalar_c() {
    for line in include_str!("fixtures/palette.txt")
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
fn extracted_colors_and_mapping_match_independent_sets() {
    use libwebpop::utils::palette::*;
    use std::collections::BTreeSet;
    for case in support::palette::cases() {
        let a = &case.input;
        let actual = case.rust();
        if a[0] == 3 {
            let (width, height, stride) = (a[1] as usize, a[2] as usize, a[3] as usize);
            let colors: BTreeSet<_> = (0..height)
                .flat_map(|y| a[4 + y * stride..4 + y * stride + width].iter().copied())
                .collect();
            assert_eq!(actual[0] as usize, colors.len().min(257));
            if colors.len() <= 256 {
                assert_eq!(&actual[1..], &colors.into_iter().collect::<Vec<_>>());
            }
        } else if a[0] == 0 {
            let n = a.len() - 1;
            for i in 0..n {
                assert_eq!(actual[i], a[1 + actual[n + i] as usize]);
            }
        } else {
            let mut sorted = actual;
            sorted.sort_unstable();
            assert_eq!(sorted, a[1..]);
        }
    }
    assert_eq!(SearchColorNoIdx(&[], 0), None);
    assert_eq!(SearchColorNoIdx(&[0, 2, 4], 3), None);
}

#[test]
fn monotonic_delta_order_and_transparent_threshold() {
    use libwebpop::utils::palette::*;
    for count in [17, 18, 256] {
        let sorted: Vec<u32> = (0..count).collect();
        let mut output = vec![0; count as usize];
        PaletteSortMinimizeDeltas(&sorted, &mut output);
        assert_eq!(output, sorted); // Monotonic shortcut precedes zero relocation.
        PaletteSortDefault(&sorted, &mut output);
        let mut expected = sorted;
        if count > 17 {
            expected.rotate_left(1);
        }
        assert_eq!(output, expected);
    }
}
#[test]
fn committed_spatial_palette_fixtures_match_scalar_c() {
    for line in include_str!("fixtures/spatial_palette.txt")
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
fn spatial_sorting_preserves_colors_and_refinement_reduces_cost() {
    for group in support::palette::spatial_cases().chunks_exact(5) {
        let a = &group[0].input;
        let n = a[4] as usize;
        let colors = &a[5..5 + n];
        let outputs: Vec<_> = group.iter().map(support::Case::rust).collect();
        for output in &outputs {
            let mut sorted = output.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, colors);
        }
        // Directly sum distances between indices on image edges. This bypasses
        // the production co-occurrence matrix and incremental insertion costs.
        let cost = |palette: &[u32]| {
            let pixels = &a[5 + n..];
            let (width, height, stride) = (a[1] as usize, a[2] as usize, a[3] as usize);
            let position = |pixel| palette.iter().position(|&c| c == pixel).unwrap();
            let mut total = 0u64;
            for y in 0..height {
                for x in 0..width {
                    let p = position(pixels[y * stride + x]);
                    if x > 0 {
                        total += p.abs_diff(position(pixels[y * stride + x - 1])) as u64;
                    }
                    if y > 0 {
                        total += p.abs_diff(position(pixels[(y - 1) * stride + x])) as u64;
                    }
                }
            }
            total
        };
        assert!(cost(&outputs[3]) <= cost(&outputs[2]));
        assert!(cost(&outputs[4]) <= cost(&outputs[1]));
    }
}
#[test]
fn committed_palette_transform_fixtures_match_scalar_c() {
    for line in include_str!("fixtures/palette_transform.txt")
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
fn packed_palette_images_decode_exactly() {
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    for case in support::palette::transform_cases()
        .into_iter()
        .filter(|c| c.input[5] == 0)
    {
        let a = &case.input;
        let (n, width, height, stride) =
            (a[0] as usize, a[1] as usize, a[2] as usize, a[3] as usize);
        let palette = &a[6..6 + n];
        let source = &a[6 + n..];
        let (packed_width, pixels) =
            MapImageFromPalette(source, stride, palette, width, height).unwrap();
        // Independently unpack every index, including the last partial group.
        let xbits = match n {
            1..=2 => 3,
            3..=4 => 2,
            5..=16 => 1,
            _ => 0,
        };
        let bits = 8 >> xbits;
        for y in 0..height {
            for x in 0..width {
                let index = (pixels[y * packed_width + (x >> xbits)]
                    >> (8 + bits * (x % (1 << xbits))))
                    & ((1 << bits) - 1);
                assert_eq!(palette[index as usize], source[y * stride + x]);
            }
        }
        let mut bw = VP8LBitWriterInit(0).unwrap();
        WriteImageSize(&mut bw, width as u32, height as u32).unwrap();
        WriteRealAlphaAndVersion(&mut bw, palette.iter().any(|p| p >> 24 != 255)).unwrap();
        let mut chain = VP8LHashChainInit(n.max(pixels.len())).unwrap();
        let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
        EncodePalette(&mut bw, palette, &mut chain, &mut refs[..2], a[4] != 0).unwrap();
        VP8LPutBits(&mut bw, 0, 1).unwrap(); // End transform list.
        let initial = VP8LBitWriterNumBytes(&bw);
        let mut cache = 0;
        EncodeImageInternal(
            &mut bw,
            &pixels,
            &mut chain,
            &mut refs,
            packed_width,
            height,
            70,
            a[4] != 0,
            &[CrunchSubConfig {
                lz77: kLZ77Standard | kLZ77RLE,
                do_no_cache: false,
            }],
            &mut cache,
            2,
            initial,
        )
        .unwrap();
        let mut file = Vec::new();
        WriteImage(&mut bw, &mut file).unwrap();
        let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (width as u32, height as u32));
        for y in 0..height {
            for x in 0..width {
                let p = source[y * stride + x];
                assert_eq!(
                    decoded.get_pixel(x as u32, y as u32).0,
                    [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]
                );
            }
        }
    }
}
