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
