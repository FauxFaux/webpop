use crate::support::Case;

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h, pattern, method, quality) in [
        (19usize, 17usize, 2, 4, 70),
        (19, 17, 9, 4, 70),
        (19, 17, 12, 4, 70),
        (65, 5, 2, 6, 100),
        (65, 5, 12, 6, 100),
        (19, 17, 5, 6, 100),
        (7, 9, 2, 0, 70),
        (7, 9, 0, 6, 100),
    ] {
        let stride = w + 3;
        let pixels = crate::support::analysis::pixels(w, h, stride, pattern);
        let analysis =
            libwebpop::enc::vp8l_enc::EncoderAnalyze(&pixels, w, h, stride, method, quality)
                .unwrap();
        for strength in [0, 60, 100] {
            for index in (0..analysis.crunch_configs.len() as u32).chain([u32::MAX]) {
                let mut a = vec![
                    w as u32,
                    h as u32,
                    stride as u32,
                    method,
                    quality,
                    strength,
                    0,
                    index,
                ];
                a.extend_from_slice(&pixels);
                cases.push(Case::new("NearLosslessStream", a));
            }
        }
    }
    cases
}
