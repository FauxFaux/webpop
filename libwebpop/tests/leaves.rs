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
