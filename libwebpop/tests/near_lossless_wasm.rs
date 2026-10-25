#[path = "support/near_lossless_public.rs"]
mod public;
mod support;

#[test]
fn full_wasm_export_regression() {
    use std::{fmt::Write, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run = root.parent().unwrap().join("target/full-wasm-oracle");
    std::fs::create_dir_all(&run).unwrap();
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 65),
        (65, 1),
        (2, 65),
        (65, 2),
        (19, 17),
        (65, 67),
    ] {
        for pattern in [2, 9, 12, 24] {
            let image = support::public_images::image(w, h, pattern);
            for strength in [0, 20, 40, 60, 80, 99, 100] {
                cases.push(public::request(&image, strength));
            }
        }
    }
    let mut requests = String::new();
    for case in &cases {
        write!(requests, "{} {}", case.function, case.input.len()).unwrap();
        for word in &case.input {
            write!(requests, " {word:08x}").unwrap();
        }
        requests.push('\n');
    }
    let input = run.join("requests.txt");
    let output = run.join("responses.txt");
    std::fs::write(&input, requests).unwrap();
    let result = Command::new("node")
        .arg(root.join("tests/reference/full-wasm/near-lossless.mjs"))
        .arg(&input)
        .arg(&output)
        .output()
        .expect("wasm-oracle requires Node with Wasm SIMD support");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let results = std::fs::read_to_string(output).unwrap();
    let mut changes = [0; 4];
    assert_eq!(results.lines().count(), cases.len());
    for (case, line) in cases.iter().zip(results.lines()) {
        let image = public::unpack(case);
        let mut words = line.split_whitespace();
        let len: usize = words.next().unwrap().parse().unwrap();
        let bytes: Vec<_> = words.map(|w| u8::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(bytes.len(), len);
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.dimensions(), (image.width, image.height));
        let q = 1 << libwebpop::internal::dsp::lossless_common::VP8LNearLosslessBits(case.input[3]);
        for y in 0..image.height as usize {
            for x in 0..image.width as usize {
                let source = &image.rgba[y * image.stride + x * 4..][..4];
                let actual = decoded.get_pixel(x as u32, y as u32).0;
                if source[3] == 0 || source[3] == 255 {
                    assert_eq!(source[3], actual[3]);
                }
                for lane in 0..4 {
                    if lane == 3 || (source[3] != 0 && actual[3] != 0) {
                        assert!(u32::from(source[lane].abs_diff(actual[lane])) <= q / 2);
                        changes[lane] += usize::from(source[lane] != actual[lane]);
                    }
                }
            }
        }
        if case.input[3] == 100 {
            image.check_decoded(&bytes);
        }
    }
    assert!(changes.into_iter().all(|n| n > 0), "changes {changes:?}");
    eprintln!(
        "{} independently decoded full-Wasm encodings; changed RGBA {changes:?}",
        cases.len()
    );
}
