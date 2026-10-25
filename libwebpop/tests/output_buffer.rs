use libwebpop::{
    WebPEncodeLosslessRGBA, WebPEncodeRGBA, WebPEncodingError,
    internal::WebPEncodeRGBAWithAlphaQuality,
    internal::WebPEncodeRGBAWithNearLossless,
    internal::{LosslessOptions, encode_lossless_rgba, encode_near_lossless_rgba},
};

fn encode(
    route: u8,
    rgba: &[u8],
    width: u32,
    height: u32,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError> {
    let stride = width as usize * 4;
    let options = LosslessOptions {
        exact: true,
        ..Default::default()
    };
    match route {
        0 => WebPEncodeLosslessRGBA(rgba, width, height, stride, output),
        1 => encode_lossless_rgba(rgba, width, height, stride, &options, output),
        2 => WebPEncodeRGBAWithNearLossless(rgba, width, height, stride, 60, output),
        3 => encode_near_lossless_rgba(rgba, width, height, stride, 60, &options, output),
        4 => WebPEncodeRGBA(rgba, width, height, stride, 75., output),
        5 => WebPEncodeRGBAWithAlphaQuality(rgba, width, height, stride, 75., 30, output),
        _ => unreachable!(),
    }
}

#[test]
fn output_replaces_previous_contents_and_reuses_capacity_across_encoders() {
    // Cover both opaque VP8 and VP8X/ALPH/VP8, plus VP8L and near-lossless.
    let images = [
        (vec![127; 17 * 19 * 4], 17, 19),
        (vec![12, 34, 56, 255], 1, 1),
    ];
    let mut output = vec![0xa5; 65536];
    let allocation = output.as_ptr();
    let capacity = output.capacity();
    for route in 0..6 {
        for (rgba, width, height) in &images {
            let mut expected = Vec::new();
            encode(route, rgba, *width, *height, &mut expected).unwrap();
            for previous_len in [0, 1, capacity] {
                output.resize(previous_len, 0xa5);
                encode(route, rgba, *width, *height, &mut output).unwrap();
                assert_eq!(
                    output, expected,
                    "route {route}, previous length {previous_len}"
                );
                assert_eq!(output.as_ptr(), allocation);
                assert_eq!(output.capacity(), capacity);
            }
            let decoded =
                image::load_from_memory_with_format(&output, image::ImageFormat::WebP).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (*width, *height));
        }
    }
}

#[test]
fn invalid_inputs_and_settings_preserve_previous_output() {
    let original = b"previous image";
    let mut output = original.to_vec();
    for route in 0..6 {
        for (rgba, width, height) in [(&[][..], 0, 1), (&[0; 3][..], 1, 1)] {
            assert!(encode(route, rgba, width, height, &mut output).is_err());
            assert_eq!(output, original);
        }
    }
    let rgba = [12, 34, 56, 255];
    assert!(WebPEncodeRGBA(&rgba, 1, 1, 4, f32::NAN, &mut output).is_err());
    assert_eq!(output, original);
    assert!(WebPEncodeRGBAWithAlphaQuality(&rgba, 1, 1, 4, 75., -1, &mut output).is_err());
    assert_eq!(output, original);
    assert!(WebPEncodeRGBAWithNearLossless(&rgba, 1, 1, 4, -1, &mut output).is_err());
    assert_eq!(output, original);
    let options = LosslessOptions {
        method: 7,
        ..Default::default()
    };
    assert!(encode_lossless_rgba(&rgba, 1, 1, 4, &options, &mut output).is_err());
    assert_eq!(output, original);
    assert!(encode_near_lossless_rgba(&rgba, 1, 1, 4, 60, &options, &mut output).is_err());
    assert_eq!(output, original);
}
