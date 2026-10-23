use super::{Case, SEED, random};
use libwebpop::dsp::{dec::TransformWHT_C, enc::*};

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "VP8FTransform2" => {
            let mut src = [0xa5; 3 * BPS + 8];
            let mut reference = [0x5a; 3 * BPS + 8];
            for i in 0..32 {
                src[i / 8 * BPS + i % 8] = a[i] as u8;
                reference[i / 8 * BPS + i % 8] = a[32 + i] as u8;
            }
            let mut out = [0; 32];
            FTransform2_C(&src, &reference, &mut out);
            signed_words(&out)
        }
        "VP8ITransform" => {
            let width = if a[0] != 0 { 8 } else { 4 };
            let count = width * 4;
            // Use exact minimum source/coeff lengths, full output for padding.
            let mut reference = vec![0x5a; 3 * BPS + width];
            for i in 0..count {
                reference[i / width * BPS + i % width] = a[1 + i] as u8;
            }
            let coefficients: Vec<_> = a[1 + count..].iter().map(|&v| v as i16).collect();
            let mut out = [0xa5; 4 * BPS];
            ITransform_C(&reference, &coefficients, &mut out, a[0] != 0);
            out.into_iter().map(u32::from).collect()
        }
        "VP8FTransformWHT" => {
            let input: Vec<_> = a.iter().map(|&v| v as i16).collect();
            let mut out = [0; 16];
            FTransformWHT_C(&input, &mut out);
            signed_words(&out)
        }
        "VP8TransformWHT" => {
            let input = std::array::from_fn(|i| a[i] as i16);
            let mut out: Vec<_> = a[16..].iter().map(|&v| v as i16).collect();
            TransformWHT_C(&input, &mut out);
            signed_words(&out)
        }
        name => panic!("unknown VP8 transform: {name}"),
    }
}

pub fn signed_words(values: &[i16]) -> Vec<u32> {
    values.iter().map(|&v| i32::from(v) as u32).collect()
}

pub fn inverse_case(reference: &[u8], coefficients: &[i16], do_two: bool) -> Case {
    let mut input = vec![u32::from(do_two)];
    input.extend(reference.iter().map(|&v| u32::from(v)));
    input.extend(signed_words(coefficients));
    Case::new("VP8ITransform", input)
}

pub fn wht_case(dc: &[i16; 16], inverse: bool, seed: &mut u32) -> Case {
    // Non-DC slots have full-width signed values, unrelated to the DC input.
    let mut blocks: Vec<i16> = (0..256).map(|_| random(seed) as i16).collect();
    if inverse {
        let mut input = signed_words(dc);
        input.extend(signed_words(&blocks));
        Case::new("VP8TransformWHT", input)
    } else {
        for i in 0..16 {
            blocks[16 * i] = dc[i];
        }
        Case::new("VP8FTransformWHT", signed_words(&blocks))
    }
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for do_two in [false, true] {
        let count = if do_two { 32 } else { 16 };
        // Every signed DC value in the oracle's safe coefficient domain;
        // alternating borders straddle both clipping thresholds and rounding.
        for dc in -4095i16..=4095 {
            let mut coefficients = vec![0; count];
            coefficients[0] = dc;
            if do_two {
                coefficients[16] = -dc;
            }
            let reference: Vec<_> = (0..count)
                .map(|i| [0, 1, 127, 128, 254, 255][i % 6])
                .collect();
            cases.push(inverse_case(&reference, &coefficients, do_two));
        }
        for position in 0..count {
            for value in [-4095, -2048, -17, -8, -4, -1, 1, 3, 4, 7, 8, 17, 2047, 4095] {
                let mut coefficients = vec![0; count];
                coefficients[position] = value;
                cases.push(inverse_case(&vec![128; count], &coefficients, do_two));
            }
        }
        for _ in 0..512 {
            let reference: Vec<_> = (0..count).map(|_| random(&mut seed) as u8).collect();
            let coefficients: Vec<_> = (0..count)
                .map(|_| (random(&mut seed) % 8191) as i16 - 4095)
                .collect();
            cases.push(inverse_case(&reference, &coefficients, do_two));
        }
    }
    for inverse in [false, true] {
        let values: &[i16] = if inverse {
            &[
                -16384, -8193, -4096, -17, -8, -4, -3, -1, 0, 1, 3, 4, 7, 8, 17, 4095, 8191, 16383,
            ]
        } else {
            &[
                -2048, -1025, -17, -8, -4, -3, -1, 0, 1, 3, 4, 7, 8, 17, 1023, 2047,
            ]
        };
        for &value in values {
            cases.push(wht_case(&[value; 16], inverse, &mut seed));
            for position in 0..16 {
                let mut dc = [0; 16];
                dc[position] = value;
                cases.push(wht_case(&dc, inverse, &mut seed));
            }
        }
        for _ in 0..1024 {
            let dc = std::array::from_fn(|_| {
                if inverse {
                    (random(&mut seed) % 32768) as i16 - 16384
                } else {
                    (random(&mut seed) % 4096) as i16 - 2048
                }
            });
            cases.push(wht_case(&dc, inverse, &mut seed));
        }
    }
    for _ in 0..1024 {
        cases.push(Case::new(
            "VP8FTransform2",
            (0..64).map(|_| random(&mut seed) & 255).collect::<Vec<_>>(),
        ));
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    // Keep representative extrema, rounding boundaries, impulses and noise
    // without recording the exhaustive DC sweep in the portable corpus.
    cases()
        .into_iter()
        .enumerate()
        .filter_map(|(i, case)| {
            let keep = match case.function.as_str() {
                "VP8ITransform" => {
                    i % 1024 == 0 || [4091, 4094, 4095, 4096, 4099, 8191, 8192, 8193].contains(&i)
                }
                "VP8FTransformWHT" | "VP8TransformWHT" => i % 47 == 0,
                "VP8FTransform2" => i % 97 == 0,
                _ => unreachable!(),
            };
            keep.then_some(case)
        })
        .collect()
}
