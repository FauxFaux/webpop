use super::{Case, SEED, random};
use libwebpop::internal::enc::vp8l_enc::*;
use libwebpop::internal::utils::palette::GetColorPalette;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let (w, h) = (a[0] as usize, a[1] as usize);
    if case.function == "AnalysisSizing" {
        let bits = GetHistoBits(a[2], a[3] != 0, w, h);
        return vec![bits, GetTransformBits(a[2], bits)];
    }
    let stride = a[2] as usize;
    if case.function == "AnalyzeEntropy" {
        let n = a[4] as usize;
        let result = AnalyzeEntropy(
            &a[5 + n..],
            w,
            h,
            stride,
            (n != 0).then_some(&a[5..5 + n]),
            a[3],
        )
        .unwrap();
        return vec![
            result.min_entropy_ix as u32,
            u32::from(result.red_and_blue_always_zero),
            u32::from(result.red_and_blue_transformed_always_zero),
        ];
    }
    let result = EncoderAnalyze(&a[5..], w, h, stride, a[3], a[4]).unwrap();
    let mut out = vec![result.palette_size as u32];
    out.extend_from_slice(&result.palette_sorted[..result.palette_size]);
    out.extend([
        result.histo_bits,
        result.predictor_transform_bits,
        result.cross_color_transform_bits,
        u32::from(result.red_and_blue_transformed_always_zero),
        result.crunch_configs.len() as u32,
    ]);
    for config in result.crunch_configs {
        out.extend([
            config.entropy_idx as u32,
            config.palette_sorting_type.map_or(5, |v| v as u32),
            config.sub_configs_size as u32,
        ]);
        for sub in &config.sub_configs[..config.sub_configs_size] {
            out.extend([sub.lz77, u32::from(sub.do_no_cache)]);
        }
    }
    out
}

pub fn pixels(w: usize, h: usize, stride: usize, pattern: u32) -> Vec<u32> {
    let mut seed = SEED;
    let mut image = vec![0xdeadbeef; (h - 1) * stride + w];
    for y in 0..h {
        for x in 0..w {
            let r = random(&mut seed);
            let i = y * w + x;
            let color = match pattern {
                0 => 0xff123456, // first pixel matters even if every pixel is skipped
                1 => 0xff000000,
                2 => r,
                3 => 0xff000000 | ((r & 255) * 0x10101), // subtract green
                4 => 0xff000000 | ((i as u32 & 255) * 0x10101),
                5 => 0xff000000 | (((x * 3 + y * 7) as u32 & 255) * 0x10101),
                6 => 0xff000000 | ((r & 255) << 8), // red/blue always zero
                7 => 0xff000000 | (((i / 7) as u32 & 255) << 8),
                8 => 0xff000000 | ((i as u32 * 0x010307) & 0xffffff),
                9 => {
                    0xff000000
                        | (((x * 13) as u32 & 255) << 16)
                        | (((y * 17) as u32 & 255) << 8)
                        | (i as u32 & 255)
                }
                10 => {
                    if y % 2 == 0 {
                        (x as u32).wrapping_mul(0x123457)
                    } else {
                        r
                    }
                }
                11 => (x as u32).wrapping_mul(0x123457), // repeated rows
                12 => (r & 255) << 24,                   // alpha only
                13 => {
                    if i == 0 {
                        0xff010001
                    } else {
                        0xff000000
                    }
                }
                // Exact palette thresholds with sequential or shuffled adjacency.
                n => {
                    let colors = [1, 2, 3, 4, 5, 16, 17, 224, 225, 256, 257][(n - 14) as usize / 2];
                    let index = if n % 2 == 0 {
                        i as u32 % colors
                    } else {
                        r % colors
                    };
                    index.wrapping_mul(0x123457)
                }
            };
            image[y * stride + x] = color;
        }
    }
    image
}

pub fn analysis_case(w: usize, h: usize, pattern: u32, method: u32, quality: u32) -> Case {
    let stride = w + 3;
    let mut a = vec![w as u32, h as u32, stride as u32, method, quality];
    a.extend(pixels(w, h, stride, pattern));
    Case::new("EncoderAnalyze", a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for pattern in 0..36 {
        cases.push(analysis_case(19, 17, pattern, 4, 70));
    }
    for (pattern, method, quality) in [
        (0, 0, 70),
        (2, 0, 70),
        (6, 6, 100),
        (2, 6, 100),
        (18, 6, 100),
        (34, 6, 100),
        (30, 5, 49),
        (30, 5, 50),
    ] {
        cases.push(analysis_case(19, 17, pattern, method, quality));
    }
    for (w, h, method, palette) in [
        (1, 1, 0, 0),
        (16383, 16383, 6, 0),
        (16383, 1, 4, 1),
        (257, 257, 5, 0),
        (65, 63, 4, 1),
        (4, 5, 6, 0),
    ] {
        cases.push(Case::new("AnalysisSizing", vec![w, h, method, palette]));
    }
    for pattern in 0..14 {
        let mut a = vec![19, 17, 22, 5, 0];
        a.extend(pixels(19, 17, 22, pattern));
        cases.push(Case::new("AnalyzeEntropy", a));
    }
    cases
}

pub fn sizing_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for w in [
        1, 3, 4, 5, 16, 17, 63, 64, 65, 255, 256, 257, 1023, 1024, 16383,
    ] {
        for h in [
            1, 3, 4, 5, 16, 17, 63, 64, 65, 255, 256, 257, 1023, 1024, 16383,
        ] {
            for method in 0..=6 {
                for palette in 0..=1 {
                    cases.push(Case::new("AnalysisSizing", vec![w, h, method, palette]));
                }
            }
        }
    }
    cases
}

pub fn cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for (w, h) in [(1, 1), (1, 43), (47, 1), (19, 17), (65, 67), (257, 2)] {
        for pattern in 0..36 {
            let stride = w + 3;
            let image = pixels(w, h, stride, pattern);
            let mut palette = [0; 256];
            let n = GetColorPalette(&image, w, h, stride, &mut palette);
            for use_palette in [false, true] {
                if use_palette && n > 256 {
                    continue;
                }
                for bits in [2, 5, 9] {
                    let size = if use_palette { n } else { 0 };
                    let mut a = vec![w as u32, h as u32, stride as u32, bits, size as u32];
                    a.extend_from_slice(&palette[..size]);
                    a.extend_from_slice(&image);
                    cases.push(Case::new("AnalyzeEntropy", a));
                }
            }
            for method in 0..=6 {
                for quality in [0, 49, 50, 70, 99, 100] {
                    let mut a = vec![w as u32, h as u32, stride as u32, method, quality];
                    a.extend_from_slice(&image);
                    cases.push(Case::new("EncoderAnalyze", a));
                }
            }
        }
    }
    cases
}
