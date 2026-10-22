mod support;

use libwebpop::enc::vp8l_enc::{WriteImage, WriteImageSize, WriteRealAlphaAndVersion};
use libwebpop::utils::bit_writer_utils::*;

#[test]
fn committed_main_image_streams_match_scalar_c() {
    let lines: Vec<_> = include_str!("fixtures/main_images.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .collect();
    let cases = support::main_images::fixture_cases();
    assert_eq!(lines.len(), cases.len());
    for (line, case) in lines.into_iter().zip(cases) {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        assert_eq!(words.next().unwrap(), case.function);
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        assert_eq!(input, case.input);
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(case.rust(), expected);
    }
}

fn bits_at(data: &[u8], start: usize, count: usize) -> u32 {
    (0..count)
        .map(|i| u32::from((data[(start + i) / 8] >> ((start + i) % 8)) & 1) << i)
        .sum()
}

#[test]
fn main_images_decode_with_cache_and_multiple_huffman_groups() {
    let (mut cached, mut grouped, mut optimized) = (0, 0, 0);
    for (w, h) in [(1, 1), (1, 47), (43, 1), (17, 19), (65, 67), (129, 33)] {
        for pattern in 0..7 {
            for (quality, low) in [(0, false), (70, false), (100, false), (70, true)] {
                let case = support::main_images::case(w, h, pattern, quality, low, 2, 0);
                let pixels = support::main_images::pixels(&case);
                let mut bw = VP8LBitWriterInit(0).unwrap();
                WriteImageSize(&mut bw, w, h).unwrap();
                WriteRealAlphaAndVersion(&mut bw, pixels.iter().any(|p| p >> 24 != 255)).unwrap();
                VP8LPutBits(&mut bw, 0, 1).unwrap(); // no transforms
                let (cache, header, data) = support::main_images::encode(&case, &mut bw, 0);
                assert_eq!(header + data, VP8LBitWriterNumBytes(&bw));
                let file = WriteImage(&mut bw).unwrap();
                let stream = &file[21..]; // VP8L payload after signature
                assert_eq!(bits_at(stream, 33, 1), u32::from(cache > 0));
                let flag = if cache > 0 {
                    cached += 1;
                    assert_eq!(bits_at(stream, 34, 4), cache);
                    38
                } else {
                    34
                };
                if bits_at(stream, flag, 1) != 0 {
                    grouped += 1;
                    let histogram_bits = bits_at(stream, flag + 1, 3) + 2;
                    if histogram_bits > case.input[5] {
                        optimized += 1;
                    }
                }
                let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
                    .unwrap()
                    .into_rgba8();
                assert_eq!(decoded.dimensions(), (w, h));
                for (actual, &expected) in decoded.pixels().zip(pixels) {
                    // No transforms: even RGB under alpha zero must survive.
                    assert_eq!(
                        actual.0,
                        [
                            (expected >> 16) as u8,
                            (expected >> 8) as u8,
                            expected as u8,
                            (expected >> 24) as u8
                        ],
                        "{w}x{h}, pattern {pattern}, q {quality}, low {low}"
                    );
                }
            }
        }
    }
    assert!(
        cached > 0 && grouped > 0 && optimized > 0,
        "cache {cached}, groups {grouped}, sampling {optimized}"
    );
    eprintln!(
        "Decoded 168 images: {cached} cached, {grouped} multi-group, {optimized} subsampled maps"
    );
}

#[test]
fn reference_trials_choose_the_first_smallest_rounded_byte_count() {
    for pattern in 0..7 {
        for prefix in [0, 7, 31, 63, 97] {
            let mut together = support::main_images::case(33, 35, pattern, 70, false, 2, prefix);
            // Two independent single-cache trials; each writes identical prefix
            // bits. Their minimum establishes the expected winner independently
            // of the integrated writer reset/swap loop.
            together.input[9] = 0;
            together.input[11] = 0;
            let mut first = together.clone();
            first.input[7] = 1;
            first.input.drain(10..12);
            let mut second = together.clone();
            second.input[7] = 1;
            second.input.drain(8..10);
            let a = first.rust();
            let b = second.rust();
            let expected = if a[1] + a[2] <= b[1] + b[2] { a } else { b };
            assert_eq!(
                together.rust(),
                expected,
                "pattern {pattern}, prefix {prefix}"
            );
        }
    }
}
