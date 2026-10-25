mod support;

use libwebpop::internal::enc::vp8l_enc::{WriteImage, WriteImageSize, WriteRealAlphaAndVersion};
use libwebpop::internal::utils::bit_writer_utils::*;

#[test]
fn committed_encoder_streams_match_scalar_c() {
    let cases = support::streams::fixture_cases();
    let lines: Vec<_> = include_str!("fixtures/streams.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .collect();
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

#[test]
fn selected_streams_decode_and_preserve_source_pixels() {
    let mut features = [false; 4];
    let mut count = 0;
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 43), (47, 1), (19, 17), (65, 67), (257, 2)] {
        for pattern in 0..36 {
            for (method, quality) in [(0, 70), (4, 70), (6, 100)] {
                for exact in [false, true] {
                    cases.push((w, h, pattern, method, quality, exact));
                }
            }
        }
    }
    for flags in support::parallel::map(&cases, |&(w, h, pattern, method, quality, exact)| {
        let case = support::streams::case(w, h, pattern, method, quality, exact, 0);
        let original = case.input.clone();
        let mut bw = VP8LBitWriterInit(0).unwrap();
        WriteImageSize(&mut bw, w as u32, h as u32).unwrap();
        WriteRealAlphaAndVersion(&mut bw, true).unwrap();
        let initial = VP8LBitWriterNumBytes(&bw);
        let stats = support::streams::encode(&case, &mut bw);
        assert_eq!(case.input, original);
        assert_eq!(stats.lossless_size + initial, VP8LBitWriterNumBytes(&bw));
        assert_eq!(
            stats.lossless_size,
            stats.lossless_hdr_size + stats.lossless_data_size
        );
        let mut file = Vec::new();
        WriteImage(&mut bw, &mut file).unwrap();
        let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.dimensions(), (w as u32, h as u32));
        for y in 0..h {
            for x in 0..w {
                let pixel = case.input[7 + y * (w + 3) + x];
                let actual = decoded.get_pixel(x as u32, y as u32).0;
                let expected = [
                    (pixel >> 16) as u8,
                    (pixel >> 8) as u8,
                    pixel as u8,
                    (pixel >> 24) as u8,
                ];
                assert_eq!(actual[3], expected[3]);
                if exact || expected[3] != 0 {
                    assert_eq!(
                        actual, expected,
                        "{w}x{h} pattern={pattern} method={method} exact={exact} ({x},{y})"
                    );
                }
            }
        }
        stats.lossless_features
    }) {
        for (i, reached) in features.iter_mut().enumerate() {
            *reached |= flags & (1 << i) != 0;
        }
        count += 1;
    }
    assert_eq!(count, 1296);
    assert_eq!(features, [true; 4]);
}
