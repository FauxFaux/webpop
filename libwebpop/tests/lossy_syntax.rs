#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::{
    enc::{syntax_enc::*, tree_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};
use support::{vp8_boolean::Decoder, vp8_syntax as syntax};

#[test]
fn committed_syntax_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_syntax.txt"));
    let cases = syntax::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}
fn read(d: &mut Decoder<'_>, bits: u32) -> u32 {
    (0..bits).fold(0, |v, _| 2 * v + u32::from(d.bit(128)))
}
fn signed(d: &mut Decoder<'_>, bits: u32) -> i32 {
    if !d.bit(128) {
        return 0;
    }
    let magnitude = read(d, bits) as i32;
    if d.bit(128) { -magnitude } else { magnitude }
}
fn decode_mode(d: &mut Decoder<'_>, ty: u32, top: u8, left: u8, p: &[u8; 3]) -> u8 {
    match ty {
        0 => {
            // Independent tree traversal: internal nodes are positive, leaves
            // use -(mode+1). Keep the tree separate from the encoder's branches.
            let tree = [
                [-1, 1],
                [-2, 2],
                [-3, 3],
                [4, 6],
                [-4, 5],
                [-5, -6],
                [-7, 7],
                [-8, 8],
                [-9, -10],
            ];
            let probs = kBModesProba[top as usize][left as usize];
            let mut node = 0usize;
            loop {
                let next = tree[node][usize::from(d.bit(probs[node]))];
                if next < 0 {
                    return (-next - 1) as u8;
                }
                node = next as usize;
            }
        }
        1 => {
            if d.bit(156) {
                if d.bit(128) { 1 } else { 3 }
            } else if d.bit(163) {
                2
            } else {
                0
            }
        }
        2 => {
            if !d.bit(142) {
                0
            } else if !d.bit(114) {
                2
            } else if !d.bit(183) {
                3
            } else {
                1
            }
        }
        3 => {
            let high = usize::from(d.bit(p[0]));
            (2 * high + usize::from(d.bit(p[high + 1]))) as u8
        }
        _ => unreachable!(),
    }
}
#[test]
fn mode_trees_decode_every_context_and_sentinel() {
    for case in syntax::mode_cases() {
        let a = &case.input;
        let out = case.rust();
        let bytes: Vec<_> = out[2..].iter().map(|&v| v as u8).collect();
        let mut d = Decoder::new(&bytes);
        assert_eq!(
            decode_mode(
                &mut d,
                a[0],
                a[2] as u8,
                a[3] as u8,
                &std::array::from_fn(|i| a[4 + i] as u8)
            ),
            a[1] as u8
        );
        assert_eq!(read(&mut d, 11), 0x5ad);
    }
}
fn decode_headers(d: &mut Decoder<'_>, enc: &VP8Encoder) {
    let segmented = d.bit(128);
    assert_eq!(segmented, enc.segment_hdr.num_segments > 1);
    if segmented {
        let map = d.bit(128);
        assert_eq!(map, enc.segment_hdr.update_map);
        assert!(d.bit(128));
        assert!(d.bit(128));
        for s in &enc.dqm {
            assert_eq!(signed(d, 7), s.quant);
        }
        for s in &enc.dqm {
            assert_eq!(signed(d, 6), s.fstrength);
        }
        if map {
            for p in enc.proba.segments {
                let value = if d.bit(128) { read(d, 8) } else { 255 };
                assert_eq!(value, u32::from(p));
            }
        }
    }
}
fn decode_filter(d: &mut Decoder<'_>, hdr: &VP8EncFilterHeader) {
    assert_eq!(d.bit(128), hdr.simple);
    assert_eq!(read(d, 6), hdr.level as u32);
    assert_eq!(read(d, 3), hdr.sharpness as u32);
    if d.bit(128) {
        assert_ne!(hdr.i4x4_lf_delta, 0);
        assert!(d.bit(128));
        assert_eq!(read(d, 4), 0);
        assert_eq!(signed(d, 6), hdr.i4x4_lf_delta);
        assert_eq!(read(d, 3), 0);
    } else {
        assert_eq!(hdr.i4x4_lf_delta, 0);
    }
}
fn decode_quant(d: &mut Decoder<'_>, enc: &VP8Encoder) {
    assert_eq!(read(d, 7), enc.base_quant as u32);
    for delta in [
        enc.dq_y1_dc,
        enc.dq_y2_dc,
        enc.dq_y2_ac,
        enc.dq_uv_dc,
        enc.dq_uv_ac,
    ] {
        assert_eq!(signed(d, 4), delta);
    }
}
fn decode_modes(d: &mut Decoder<'_>, enc: &VP8Encoder) {
    for y in 0..enc.mb_h {
        for x in 0..enc.mb_w {
            let mb = enc.mb_info[y * enc.mb_w + x];
            let at = enc.preds_offset(x, y);
            if enc.segment_hdr.update_map {
                assert_eq!(decode_mode(d, 3, 0, 0, &enc.proba.segments), mb.segment);
            }
            if enc.proba.use_skip_proba {
                assert_eq!(d.bit(enc.proba.skip_proba), mb.skip);
            }
            assert_eq!(d.bit(145), mb.r#type != 0);
            if mb.r#type != 0 {
                assert_eq!(decode_mode(d, 1, 0, 0, &[0; 3]), enc.preds[at]);
            } else {
                for row in 0..4 {
                    for col in 0..4 {
                        let i = at + row * enc.preds_w + col;
                        assert_eq!(
                            decode_mode(
                                d,
                                0,
                                enc.preds[i - enc.preds_w],
                                enc.preds[i - 1],
                                &[0; 3]
                            ),
                            enc.preds[i]
                        );
                    }
                }
            }
            assert_eq!(decode_mode(d, 2, 0, 0, &[0; 3]), mb.uv_mode);
        }
    }
}
#[test]
fn partition_headers_and_raster_modes_decode_independently() {
    for case in syntax::syntax_cases() {
        let a = &case.input;
        if a[0] == 5 {
            continue;
        }
        let enc = syntax::initialized(a);
        let out = case.rust();
        let bytes: Vec<_> = out[if a[0] == 4 { 0 } else { 2 }..]
            .iter()
            .map(|&v| v as u8)
            .collect();
        let mut d = Decoder::new(&bytes);
        match a[0] {
            0 => decode_headers(&mut d, &enc),
            1 => decode_filter(&mut d, &enc.filter_hdr),
            2 => decode_quant(&mut d, &enc),
            3 => decode_modes(&mut d, &enc),
            4 => {
                assert_eq!(read(&mut d, 2), 0);
                decode_headers(&mut d, &enc);
                decode_filter(&mut d, &enc.filter_hdr);
                assert_eq!(read(&mut d, 2), 0);
                decode_quant(&mut d, &enc);
                assert!(!d.bit(128));
                for ((&expected, &default), &p) in enc
                    .proba
                    .coeffs
                    .iter()
                    .flatten()
                    .flatten()
                    .flatten()
                    .zip(VP8CoeffsProba0.iter().flatten().flatten().flatten())
                    .zip(VP8CoeffsUpdateProba.iter().flatten().flatten().flatten())
                {
                    let v = if d.bit(p) {
                        read(&mut d, 8) as u8
                    } else {
                        default
                    };
                    assert_eq!(v, expected);
                }
                assert_eq!(d.bit(128), enc.proba.use_skip_proba);
                if enc.proba.use_skip_proba {
                    assert_eq!(read(&mut d, 8), u32::from(enc.proba.skip_proba));
                }
                decode_modes(&mut d, &enc);
            }
            _ => unreachable!(),
        }
        if a[0] != 4 {
            assert_eq!(read(&mut d, 11), 0x5ad);
        }
    }
}
#[test]
fn opaque_container_sizes_padding_and_frame_limits() {
    for profile in 0..4 {
        for size0 in [0, 1, (1 << 19) - 1] {
            let h = PutVP8FrameHeader(16383, 1, profile, size0).unwrap();
            let tag = u32::from_le_bytes([h[0], h[1], h[2], 0]);
            assert_eq!(tag >> 5, size0 as u32);
            assert_eq!((tag >> 1) & 7, u32::from(profile));
            assert_eq!(tag & 17, 16);
            assert_eq!(&h[3..6], &[0x9d, 1, 0x2a]);
            assert_eq!(&h[6..], &[255, 63, 1, 0]);
        }
    }
    for (w, h, p) in [
        (0, 1, 0),
        (1, 0, 0),
        (16384, 1, 0),
        (1, 16384, 0),
        (1, 1, 4),
    ] {
        assert!(matches!(
            PutVP8FrameHeader(w, h, p, 0),
            Err(VP8WriteError::InvalidFrame)
        ));
    }
    assert!(matches!(
        OpaqueWebPSizes(1 << 19, 0),
        Err(VP8WriteError::Partition0Overflow)
    ));
    assert!(matches!(
        OpaqueWebPSizes(0, usize::MAX),
        Err(VP8WriteError::FileTooBig)
    ));
    let max_tokens = 0xffff_fffeusize - 22;
    assert_eq!(
        OpaqueWebPSizes(0, max_tokens).unwrap(),
        (0xffff_fff2, 0xffff_fffe)
    );
    assert!(matches!(
        OpaqueWebPSizes(0, max_tokens + 1),
        Err(VP8WriteError::FileTooBig)
    ));
    for n in 0..4 {
        let tokens = vec![0x93; n];
        let mut bytes = Vec::new();
        AssembleOpaqueWebP(1, 1, 0, &[1, 2, 3], &tokens, &mut bytes).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8
        );
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize,
            bytes.len() - 20
        );
        assert_eq!(&bytes[30..33], &[1, 2, 3]);
        assert_eq!(&bytes[33..33 + n], tokens);
        if (3 + n) & 1 != 0 {
            assert_eq!(*bytes.last().unwrap(), 0);
        }
    }
    let enc = VP8Encoder::new(17, 17, false).unwrap();
    let mut output = b"previous image".to_vec();
    assert!(matches!(
        VP8EncWrite(&enc, 1, 1, 0, &[], &mut output),
        Err(VP8WriteError::InvalidFrame)
    ));
    assert_eq!(output, b"previous image");
    assert!(matches!(
        AssembleOpaqueWebP(0, 1, 0, &[], &[], &mut output),
        Err(VP8WriteError::InvalidFrame)
    ));
    assert_eq!(output, b"previous image");
}
#[test]
fn fixed_dc_images_decode_independently() {
    for case in syntax::fixed_cases() {
        let a = &case.input;
        let bytes = syntax::fixed_image(a[0], a[1], a[2] as u8);
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (a[0], a[1]));
        assert!(image.pixels().all(|p| p.0 == [130, 130, 130, 255]));
    }
}
#[test]
fn writing_modes_preserves_frame_scratch_and_stored_decisions() {
    let mut seed = support::SEED;
    let case = syntax::syntax_case(4, 33, 49, &mut seed);
    let mut enc = syntax::initialized(&case.input);
    enc.nz.fill(0x12345678);
    enc.y_top.fill(7);
    enc.uv_top.fill(9);
    let before = (
        enc.nz.clone(),
        enc.y_top.clone(),
        enc.uv_top.clone(),
        enc.preds.clone(),
        enc.mb_info.clone(),
        enc.proba.clone(),
    );
    let first = GeneratePartition0(&enc).unwrap();
    let second = GeneratePartition0(&enc).unwrap();
    assert_eq!(VP8BitWriterBuf(&first), VP8BitWriterBuf(&second));
    assert_eq!(
        before,
        (
            enc.nz,
            enc.y_top,
            enc.uv_top,
            enc.preds,
            enc.mb_info,
            enc.proba
        )
    );
}
#[cfg(feature = "native-oracle")]
#[test]
fn syntax_matches_pinned_reference() {
    let cases = syntax::cases();
    native::parallel_cases("vp8-syntax", &cases, |c, out| {
        native::compare(c, out, &c.rust())
    });
    eprintln!("{} exact VP8 syntax/container requests", cases.len());
}
#[cfg(feature = "native-oracle")]
#[test]
fn syntax_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = syntax::fixture_cases();
    let outputs = native::run("vp8-syntax-fixtures", &cases);
    for (c, out) in cases.iter().zip(&outputs) {
        native::compare(c, out, &c.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 syntax and opaque containers.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (c, out) in cases.iter().zip(&outputs) {
            text.push_str(&c.function);
            for v in &c.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in out {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lossy_syntax.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_syntax.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
}
