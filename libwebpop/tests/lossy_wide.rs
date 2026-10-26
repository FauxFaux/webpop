//! Exact portable-vector/scalar parity, including domains wider than normal
//! encoder coefficients and matrices. The scalar kernels have pinned C oracles.
use libwebpop::internal::{
    dsp::{
        cost::{GetResidualCost_C, SetResidualCoeffs_C},
        cost_wide::*,
        dec::TransformWHT_C,
        dec_wide::TransformWHT_wide,
        enc::*,
        enc_wide::*,
    },
    enc::{cost_enc::*, vp8i_enc::*},
};

fn random(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

#[test]
fn transforms_distortion_and_statistics_match_scalar() {
    // Independent DC normalization, inverse rounding/clipping, and error
    // identities, in addition to the scalar differential corpus below.
    let black = [0; 16 * BPS];
    let white = [255; 16 * BPS];
    assert_eq!(SSE4x4_wide(&black, &white), 16 * 255 * 255);
    assert_eq!(SSE8x8_wide(&black, &white), 64 * 255 * 255);
    assert_eq!(SSE16x8_wide(&black, &white), 128 * 255 * 255);
    assert_eq!(SSE16x16_wide(&black, &white), 256 * 255 * 255);
    let mut transformed = [0; 16];
    FTransform_wide(&white, &black, &mut transformed);
    assert_eq!(transformed[0], 8 * 255);
    assert_eq!(Disto4x4_wide(&white, &white, &[u16::MAX; 16]), 0);
    for dc in [i16::MIN, -4095, -5, -4, -1, 0, 1, 3, 4, 4095, i16::MAX] {
        let mut coefficients = [0; 16];
        coefficients[0] = dc;
        let mut pixels = [0xa5; 4 * BPS];
        ITransform_wide(&[128; 3 * BPS + 4], &coefficients, &mut pixels, false);
        let value = (128 + (i32::from(dc) + 4).div_euclid(8)).clamp(0, 255) as u8;
        for y in 0..4 {
            for x in 0..BPS {
                assert_eq!(pixels[y * BPS + x], if x < 4 { value } else { 0xa5 });
            }
        }
    }
    let mut seed = 0x8391bdef;
    for trial in 0..4096 {
        let mut a: [u8; 16 * BPS + 1] = std::array::from_fn(|_| random(&mut seed) as u8);
        let b: [u8; 16 * BPS + 1] = std::array::from_fn(|_| random(&mut seed) as u8);
        if trial < 256 {
            a.fill(trial as u8);
        }
        let (a, b) = (&a[1..], &b[1..]); // unaligned starts
        let mut scalar = [0; 32];
        let mut vector = [0; 32];
        FTransform2_C(a, b, &mut scalar);
        FTransform2_wide(a, b, &mut vector);
        assert_eq!(vector, scalar, "forward {trial}");
        let coeffs: [i16; 32] = std::array::from_fn(|i| {
            if trial < 64 {
                if i == trial % 32 {
                    if trial < 32 { i16::MIN } else { i16::MAX }
                } else {
                    0
                }
            } else {
                random(&mut seed) as i16
            }
        });
        for two in [false, true] {
            let width = if two { 8 } else { 4 };
            let mut scalar = [0xa5; 4 * BPS + 2];
            let mut vector = scalar;
            ITransform_C(
                &a[..3 * BPS + width],
                &coeffs,
                &mut scalar[1..1 + 3 * BPS + width],
                two,
            );
            ITransform_wide(
                &a[..3 * BPS + width],
                &coeffs,
                &mut vector[1..1 + 3 * BPS + width],
                two,
            );
            assert_eq!(vector, scalar, "inverse {trial} two={two}");
        }
        let mut dc = [0; 4];
        let mut dc_wide = dc;
        Mean16x4_C(a, &mut dc);
        Mean16x4_wide(a, &mut dc_wide);
        assert_eq!(dc_wide, dc);
        assert_eq!(SSE4x4_wide(a, b), SSE4x4_C(a, b));
        assert_eq!(SSE8x8_wide(a, b), SSE8x8_C(a, b));
        assert_eq!(SSE16x8_wide(a, b), SSE16x8_C(a, b));
        assert_eq!(SSE16x16_wide(a, b), SSE16x16_C(a, b));
        let weights = std::array::from_fn(|_| random(&mut seed) as u16);
        assert_eq!(Disto4x4_wide(a, b, &weights), Disto4x4_C(a, b, &weights));
        assert_eq!(
            Disto16x16_wide(a, b, &weights),
            Disto16x16_C(a, b, &weights)
        );
        let start = trial % 25;
        let mut hist = VP8Histogram::default();
        let mut hist_wide = hist;
        CollectHistogram_C(a, b, start, 24, &mut hist);
        CollectHistogram_wide(a, b, start, 24, &mut hist_wide);
        assert_eq!(hist_wide, hist);
    }
}

#[test]
fn walsh_transforms_preserve_scatter_and_padding() {
    let mut seed = 0x89138ab4;
    for trial in 0..4096 {
        let mut input: [i16; 16] = std::array::from_fn(|_| random(&mut seed) as i16);
        if trial < 32 {
            input.fill(0);
            input[trial % 16] = if trial < 16 { i16::MIN } else { i16::MAX };
        }
        let mut scalar: [i16; 259] = std::array::from_fn(|_| random(&mut seed) as i16);
        let mut vector = scalar;
        TransformWHT_C(&input, &mut scalar[1..242]);
        TransformWHT_wide(&input, &mut vector[1..242]);
        assert_eq!(vector, scalar, "inverse WHT {trial}");
        let mut scalar = [0; 16];
        let mut vector = scalar;
        let mut blocks: [i16; 241] = std::array::from_fn(|_| random(&mut seed) as i16);
        for i in 0..16 {
            blocks[16 * i] = input[i];
        }
        FTransformWHT_C(&blocks, &mut scalar);
        FTransformWHT_wide(&blocks, &mut vector);
        assert_eq!(vector, scalar, "forward WHT {trial}");
    }
}

#[test]
fn quantization_masks_rounding_signs_clipping_and_zigzag() {
    let mut seed = 0xf591ab39;
    for trial in 0..8192 {
        let matrix = VP8Matrix {
            q: std::array::from_fn(|_| random(&mut seed) as u16),
            iq: std::array::from_fn(|_| random(&mut seed) as u16),
            bias: std::array::from_fn(|_| random(&mut seed)),
            zthresh: std::array::from_fn(|_| {
                if trial % 2 == 0 {
                    random(&mut seed)
                } else {
                    random(&mut seed) & 65535
                }
            }),
            sharpen: std::array::from_fn(|_| random(&mut seed) as u16),
        };
        let mut scalar = std::array::from_fn(|i| match trial % 5 {
            0 => [i16::MIN, i16::MAX, -1, 0, 1][i % 5],
            1 => 0,
            _ => random(&mut seed) as i16,
        });
        let mut vector = scalar;
        let mut out_c = [0; 32];
        let mut out_wide = out_c;
        let nz_c = Quantize2Blocks_C(&mut scalar, &mut out_c, &matrix);
        let nz_wide = Quantize2Blocks_wide(&mut vector, &mut out_wide, &matrix);
        assert_eq!(
            (nz_wide, vector, out_wide),
            (nz_c, scalar, out_c),
            "quantize {trial}"
        );
    }
    // Every lane at, below and above its threshold; equality must quantize to 0.
    for lane in 0..16 {
        for value in [-1025i16, -1024, -1023, 1023, 1024, 1025] {
            let matrix = VP8Matrix {
                q: [4; 16],
                iq: [32768; 16],
                bias: [65536; 16],
                zthresh: [1024; 16],
                sharpen: [0; 16],
            };
            let mut scalar = [0; 16];
            scalar[lane] = value;
            let mut vector = scalar;
            let mut c = [0; 16];
            let mut w = c;
            assert_eq!(
                QuantizeBlock_wide(&mut vector, &mut w, &matrix),
                QuantizeBlock_C(&mut scalar, &mut c, &matrix)
            );
            assert_eq!((vector, w), (scalar, c));
        }
    }
}

#[test]
fn predictors_match_all_modes_borders_and_padding() {
    let mut seed = 0x9341bd12;
    for trial in 0..4096 {
        let t: [u8; 13] = std::array::from_fn(|i| match trial % 4 {
            0 => [0, 255][i % 2],
            1 => trial as u8,
            _ => random(&mut seed) as u8,
        });
        let mut scalar = [0xa5; PRED_SIZE_ENC + 2];
        let mut vector = scalar;
        Intra4Preds_C(&mut scalar[1..1 + PRED_SIZE_ENC], &t);
        Intra4Preds_wide(&mut vector[1..1 + PRED_SIZE_ENC], &t);
        assert_eq!(vector, scalar, "4x4 predictors {trial}");
        let left: [u8; 25] = std::array::from_fn(|_| random(&mut seed) as u8);
        let top: [u8; 16] = std::array::from_fn(|_| random(&mut seed) as u8);
        for l in [None, Some(&left[..])] {
            for t in [None, Some(&top[..])] {
                scalar.fill(0xa5);
                vector = scalar;
                Intra16Preds_C(&mut scalar[1..], l, t);
                Intra16Preds_wide(&mut vector[1..], l, t);
                assert_eq!(vector, scalar, "16x16 predictors {trial}");
                IntraChromaPreds_C(&mut scalar[1..], l, t);
                IntraChromaPreds_wide(&mut vector[1..], l, t);
                assert_eq!(vector, scalar, "chroma predictors {trial}");
            }
        }
    }
}

#[test]
fn residual_classification_and_costs_match_scalar() {
    let mut seed = 0x18fbab12;
    let mut proba = VP8EncProba::default();
    for trial in 0..1024 {
        for kind in &mut proba.coeffs {
            for band in kind {
                for context in band {
                    for p in context {
                        *p = random(&mut seed) as u8;
                    }
                }
            }
        }
        proba.dirty = true;
        VP8CalculateLevelCosts(&mut proba);
        for first in 0..2 {
            let mut coeffs = std::array::from_fn(|_| (random(&mut seed) % 4095) as i16 - 2047);
            for value in &mut coeffs[trial % 17..] {
                *value = 0;
            }
            if first != 0 {
                coeffs[0] = 0;
            }
            for kind in 0..NUM_TYPES {
                let mut scalar = VP8InitResidual(first, kind);
                let mut vector = scalar.clone();
                SetResidualCoeffs_C(&coeffs, &mut scalar);
                SetResidualCoeffs_wide(&coeffs, &mut vector);
                assert_eq!(vector.last, scalar.last);
                for ctx in 0..NUM_CTX {
                    assert_eq!(
                        GetResidualCost_wide(ctx, &vector, &proba),
                        GetResidualCost_C(ctx, &scalar, &proba)
                    );
                }
            }
        }
    }
}

#[test]
fn flatness_ignores_dc_and_counts_every_ac_position() {
    use libwebpop::internal::dsp::{quant::IsFlat, quant_wide::IsFlat_wide};
    let mut blocks = [[0; 16]; 16];
    for (i, block) in blocks.iter_mut().enumerate() {
        block[0] = if i % 2 == 0 { i16::MIN } else { i16::MAX };
    }
    for count in 0..=240 {
        if count > 0 {
            let i = count - 1;
            blocks[i / 15][1 + i % 15] = if i % 2 == 0 { -1 } else { 1 };
        }
        for len in 0..=16 {
            for threshold in [
                -1,
                0,
                1,
                2,
                3,
                15,
                16,
                count as i32 - 1,
                count as i32,
                240,
                i32::MAX,
            ] {
                let expected = len == 0 || count.min(15 * len) as i32 <= threshold;
                assert_eq!(IsFlat_wide(&blocks[..len], threshold), expected);
                assert_eq!(
                    IsFlat_wide(&blocks[..len], threshold),
                    IsFlat(&blocks[..len], threshold),
                    "count {count} len {len} threshold {threshold}"
                );
            }
        }
    }
}
