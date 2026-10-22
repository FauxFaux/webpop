mod support;

use libwebpop::dsp::lossless_common::*;
use libwebpop::enc::picture_csp_enc::{RgbaInputError::*, checked_rgba_geometry};
use libwebpop::utils::color_cache_utils::VP8LColorCacheInit;

#[test]
fn committed_scalar_fixtures() {
    let fixtures = support::fixtures();
    assert!(fixtures.len() >= 40);
    for (case, expected) in fixtures {
        if case.function == "WebPEncodeLosslessRGBA" {
            let image = support::ImageCase {
                name: "public encoding fixture",
                width: case.input[0],
                height: case.input[1],
                stride: case.input[2] as usize,
                rgba: case.input[3..].iter().map(|&v| v as u8).collect(),
            };
            image.check_decoded(
                &expected
                    .into_iter()
                    .map(|v| u8::try_from(v).unwrap())
                    .collect::<Vec<_>>(),
            );
        } else {
            assert_eq!(case.rust(), expected, "{case:?}");
        }
    }
}

#[test]
fn independent_byte_arithmetic() {
    for a in 0..256_u32 {
        for b in 0..256_u32 {
            let x = u32::from_le_bytes([a as u8, 255, (255 - a) as u8, 0]);
            let y = u32::from_le_bytes([b as u8, 1, (255 - b) as u8, 255]);
            let mut add = [0; 4];
            let mut sub = [0; 4];
            for (i, (a, b)) in x.to_le_bytes().into_iter().zip(y.to_le_bytes()).enumerate() {
                add[i] = a.wrapping_add(b);
                sub[i] = a.wrapping_sub(b);
            }
            assert_eq!(VP8LAddPixels(x, y).to_le_bytes(), add);
            assert_eq!(VP8LSubPixels(x, y).to_le_bytes(), sub);
        }
    }
}

#[test]
fn prefix_codes_reconstruct_distances() {
    for distance in 1..=1 << 20 {
        let (code, bits, value) = VP8LPrefixEncode(distance);
        let reconstructed = if code < 4 {
            code + 1
        } else {
            ((2 + (code & 1)) << bits) + value + 1
        };
        assert_eq!(reconstructed, distance);
        assert!(value < (1 << bits));
        assert_eq!(VP8LPrefixEncodeBits(distance), (code, bits));
    }
}

#[test]
fn checked_geometry_boundaries() {
    assert_eq!(checked_rgba_geometry(4, 1, 1, 4), Ok(4));
    assert_eq!(checked_rgba_geometry(20, 2, 2, 12), Ok(20));
    assert_eq!(checked_rgba_geometry(19, 2, 2, 12), Err(InputTooShort));
    assert_eq!(
        checked_rgba_geometry(usize::MAX, 0, 1, 4),
        Err(InvalidDimensions)
    );
    assert_eq!(
        checked_rgba_geometry(usize::MAX, 1, 0, 4),
        Err(InvalidDimensions)
    );
    for dimension in [16384, u32::MAX] {
        assert_eq!(
            checked_rgba_geometry(usize::MAX, dimension, 1, usize::MAX),
            Err(InvalidDimensions)
        );
        assert_eq!(
            checked_rgba_geometry(usize::MAX, 1, dimension, usize::MAX),
            Err(InvalidDimensions)
        );
    }
    assert_eq!(checked_rgba_geometry(65532, 16383, 1, 65532), Ok(65532));
    assert_eq!(checked_rgba_geometry(65532, 1, 16383, 4), Ok(65532));
    assert_eq!(checked_rgba_geometry(100, 2, 1, 7), Err(InvalidStride));
    assert_eq!(
        checked_rgba_geometry(usize::MAX, 1, 2, usize::MAX),
        Err(SizeOverflow)
    );
    assert_eq!(
        checked_rgba_geometry(usize::MAX, 1, 3, usize::MAX / 2 + 1),
        Err(SizeOverflow)
    );
    // A single row never uses the stride to compute an offset.
    assert_eq!(checked_rgba_geometry(4, 1, 1, usize::MAX), Ok(4));
    assert_eq!(VP8LSubSampleSize(u32::MAX, 0), u32::MAX);
    assert_eq!(VP8LSubSampleSize(u32::MAX, 31), 2);
    for bits in [0, 12, 32, u32::MAX] {
        assert!(VP8LColorCacheInit(bits).is_none());
    }
}

#[test]
fn palette_bundles_recover_indices() {
    use libwebpop::dsp::lossless_enc::VP8LBundleColorMap_C;
    for xbits in 0..=3 {
        let bits_per_index = 8 >> xbits;
        let mask = (1 << bits_per_index) - 1;
        for width in 0..=65_usize {
            let row: Vec<_> = (0..width).map(|x| ((x * 37) & mask) as u8).collect();
            let mut packed = vec![0; width.div_ceil(1 << xbits)];
            VP8LBundleColorMap_C(&row, xbits, &mut packed);
            for (x, &index) in row.iter().enumerate() {
                let pixel = packed[x >> xbits];
                assert_eq!(pixel >> 24, 255);
                assert_eq!(pixel & 0x00ff00ff, 0);
                let shift = 8 + bits_per_index * (x % (1 << xbits));
                assert_eq!((pixel >> shift) & mask as u32, u32::from(index));
            }
            // Unused high index bits in a partial final bundle must be zero.
            if width % (1 << xbits) != 0 {
                let used_bits = bits_per_index * (width % (1 << xbits));
                assert_eq!(
                    (packed.last().unwrap() >> (8 + used_bits)) & (255 >> used_bits),
                    0
                );
            }
        }
    }
}

#[test]
fn entropy_streaks_track_run_starts_and_reset() {
    use libwebpop::dsp::lossless::{VP8LBitEntropy, VP8LStreaks};
    use libwebpop::dsp::lossless_enc::{GetEntropyUnrefined_C, VP8LBitsEntropyUnrefined};
    let population = [0, 0, 0, 7, 7, 7, 7, 0, 0, 0, 0, 1, 1, 1];
    let mut e = VP8LBitEntropy::default();
    let mut stats = VP8LStreaks::default();
    GetEntropyUnrefined_C(&population, &mut e, &mut stats);
    assert_eq!(
        (e.sum, e.nonzeros, e.max_val, e.nonzero_code),
        (31, 7, 7, 11)
    );
    assert_eq!(stats.counts, [1, 1]);
    assert_eq!(stats.streaks, [[3, 4], [3, 4]]);
    let entropy = e.entropy;
    VP8LBitsEntropyUnrefined(&population, &mut e);
    assert_eq!(e.entropy, entropy);
    assert_eq!(e.nonzero_code, 13);
    GetEntropyUnrefined_C(&[0; 4], &mut e, &mut stats);
    assert_eq!(e, VP8LBitEntropy::default());
    assert_eq!(stats.counts, [1, 0]);
    assert_eq!(stats.streaks, [[0, 4], [0, 0]]);
}
