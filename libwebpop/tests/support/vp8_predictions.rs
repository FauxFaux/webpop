use super::{Case, SEED, random};
use libwebpop::internal::{dsp::enc::*, enc::vp8i_enc::*};

pub const LUMA16_OFFSETS: [usize; 4] = [I16DC16, I16TM16, I16VE16, I16HE16];
pub const CHROMA_OFFSETS: [usize; 4] = [C8DC8, C8TM8, C8VE8, C8HE8];
pub const LUMA4_OFFSETS: [usize; 10] = [
    I4DC4, I4TM4, I4VE4, I4HE4, I4RD4, I4VR4, I4LD4, I4VL4, I4HD4, I4HU4,
];

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let small = case.function == "VP8EncPredLuma4";
    let sentinel = a[usize::from(!small)] as u8;
    let mut dst = vec![sentinel; PRED_SIZE_ENC];
    if small {
        let top = std::array::from_fn(|i| a[1 + i] as u8);
        Intra4Preds_C(&mut dst, &top);
    } else {
        let chroma = case.function == "VP8EncPredChroma8";
        let left_size = if chroma { 25 } else { 17 };
        let left: Vec<_> = a[2..2 + left_size].iter().map(|&v| v as u8).collect();
        let top: Vec<_> = a[2 + left_size..].iter().map(|&v| v as u8).collect();
        let left = if a[0] & 1 != 0 { Some(&left[..]) } else { None };
        let top = if a[0] & 2 != 0 { Some(&top[..]) } else { None };
        if chroma {
            IntraChromaPreds_C(&mut dst, left, top);
        } else {
            Intra16Preds_C(&mut dst, left, top);
        }
    }
    dst.into_iter().map(u32::from).collect()
}

pub fn large_case(chroma: bool, flags: u32, left: &[u8], top: &[u8], sentinel: u8) -> Case {
    assert_eq!(left.len(), if chroma { 25 } else { 17 });
    assert_eq!(top.len(), 16);
    let mut input = vec![flags, u32::from(sentinel)];
    input.extend(left.iter().chain(top).map(|&v| u32::from(v)));
    Case::new(
        if chroma {
            "VP8EncPredChroma8"
        } else {
            "VP8EncPredLuma16"
        },
        input,
    )
}

pub fn small_case(top: &[u8; 13], sentinel: u8) -> Case {
    let mut input = vec![u32::from(sentinel)];
    input.extend(top.iter().map(|&v| u32::from(v)));
    Case::new("VP8EncPredLuma4", input)
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for chroma in [false, true] {
        let count = if chroma { 25 } else { 17 };
        for flags in 0..4 {
            for value in 0..=255 {
                cases.push(large_case(
                    chroma,
                    flags,
                    &vec![value; count],
                    &[value; 16],
                    0xa5,
                ));
            }
            for _ in 0..128 {
                let left: Vec<_> = (0..count).map(|_| random(&mut seed) as u8).collect();
                let top = std::array::from_fn::<_, 16, _>(|_| random(&mut seed) as u8);
                cases.push(large_case(
                    chroma,
                    flags,
                    &left,
                    &top,
                    random(&mut seed) as u8,
                ));
            }
        }
        for corner in [0, 1, 127, 128, 254, 255] {
            for l in [0, 1, 127, 128, 254, 255] {
                for t in [0, 1, 127, 128, 254, 255] {
                    let mut left = vec![l; count];
                    left[0] = corner;
                    let mut top = [t; 16];
                    if chroma {
                        left[16] = 255 - corner;
                        left[17..].fill(255 - l);
                        top[8..].fill(255 - t);
                    }
                    cases.push(large_case(chroma, 3, &left, &top, 0x5a));
                }
            }
        }
    }
    for value in 0..=255 {
        cases.push(small_case(&[value; 13], 0xa5));
    }
    for position in 0..13 {
        for value in [0, 1, 127, 128, 254, 255] {
            let mut top = [128; 13];
            top[position] = value;
            cases.push(small_case(&top, 0x5a));
        }
    }
    for a in [0, 1, 127, 128, 254, 255] {
        for b in [0, 1, 127, 128, 254, 255] {
            for c in [0, 1, 127, 128, 254, 255] {
                for start in [3, 5] {
                    let mut top = [128; 13];
                    top[start..start + 3].copy_from_slice(&[a, b, c]);
                    cases.push(small_case(&top, 0xa5));
                }
            }
        }
    }
    for i in 0..640 {
        let mut top = std::array::from_fn(|_| random(&mut seed) as u8);
        if i < 128 {
            let last = top[8];
            top[9..].fill(last);
        }
        cases.push(small_case(&top, random(&mut seed) as u8));
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    cases()
        .into_iter()
        .enumerate()
        .filter_map(|(i, case)| (i % 97 == 0).then_some(case))
        .collect()
}
