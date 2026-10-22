use libwebpop::enc::backward_references_enc::{VP8LBackwardRefsInit, VP8LHashChainInit};
use libwebpop::enc::vp8l_enc::EncodeImageNoHuffman;
use libwebpop::utils::bit_writer_utils::{VP8LBitWriterFinish, VP8LBitWriterInit, VP8LPutBits};

// Test-only untransformed container. The public encoder still needs analysis
// and transforms; this exercises the milestone-5 integration independently.
fn encode(pixels: &[u32], width: usize, height: usize, quality: u32, low_effort: bool) -> Vec<u8> {
    let mut bw = VP8LBitWriterInit(0).unwrap();
    VP8LPutBits(&mut bw, (width - 1) as u32, 14).unwrap();
    VP8LPutBits(&mut bw, (height - 1) as u32, 14).unwrap();
    VP8LPutBits(&mut bw, u32::from(pixels.iter().any(|p| p >> 24 != 255)), 1).unwrap();
    VP8LPutBits(&mut bw, 0, 3).unwrap(); // version
    VP8LPutBits(&mut bw, 0, 1).unwrap(); // no transforms
    // Main images include a spatial Huffman flag; auxiliary images omit it.
    // Both flags are zero, so prefix it before the auxiliary cache flag.
    VP8LPutBits(&mut bw, 0, 1).unwrap();
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
    EncodeImageNoHuffman(
        &mut bw, pixels, &mut chain, &mut refs, width, height, quality, low_effort,
    )
    .unwrap();
    let mut payload = vec![0x2f];
    payload.extend_from_slice(VP8LBitWriterFinish(&mut bw).unwrap());
    let padding = payload.len() & 1;
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&((12 + payload.len() + padding) as u32).to_le_bytes());
    file.extend_from_slice(b"WEBPVP8L");
    file.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    file.extend_from_slice(&payload);
    file.resize(file.len() + padding, 0);
    file
}

#[test]
fn untransformed_auxiliary_streams_decode() {
    let mut seed = 0x6d2b79f5u32;
    for (width, height) in [(1, 1), (1, 37), (41, 1), (17, 19), (64, 33)] {
        for pattern in 0..5 {
            let pixels: Vec<u32> = (0..width * height)
                .map(|i| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    match pattern {
                        0 => 0xff123456,
                        1 => {
                            if i % 2 == 0 {
                                0
                            } else {
                                0xffffffff
                            }
                        }
                        2 => 0xff000000 | ((i % width) as u32 * 0x010203),
                        3 => seed,
                        _ => [0xff102030, 0x80506070, 0x00123456][(i / 7) % 3],
                    }
                })
                .collect();
            for (quality, low_effort) in [(0, false), (70, false), (100, true)] {
                let file = encode(&pixels, width, height, quality, low_effort);
                let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
                    .unwrap()
                    .to_rgba8();
                assert_eq!(decoded.dimensions(), (width as u32, height as u32));
                for (actual, &pixel) in decoded.pixels().zip(&pixels) {
                    assert_eq!(
                        actual.0,
                        [
                            (pixel >> 16) as u8,
                            (pixel >> 8) as u8,
                            pixel as u8,
                            (pixel >> 24) as u8
                        ]
                    );
                }
            }
        }
    }
}

#[test]
fn committed_auxiliary_streams_match_scalar_c() {
    for line in include_str!("fixtures/auxiliary.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (request, response) = line.split_once(" | ").unwrap();
        let a: Vec<u32> = request
            .split_whitespace()
            .skip(2)
            .map(|w| u32::from_str_radix(w, 16).unwrap())
            .collect();
        let expected: Vec<u8> = response
            .split_whitespace()
            .skip(1)
            .map(|w| u8::from_str_radix(w, 16).unwrap())
            .collect();
        let mut bw = VP8LBitWriterInit(0).unwrap();
        VP8LPutBits(&mut bw, a[4], a[5]).unwrap();
        let mut chain = VP8LHashChainInit((a[0] * a[1]) as usize).unwrap();
        let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
        EncodeImageNoHuffman(
            &mut bw,
            &a[6..],
            &mut chain,
            &mut refs,
            a[0] as usize,
            a[1] as usize,
            a[2],
            a[3] != 0,
        )
        .unwrap();
        VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
        assert_eq!(VP8LBitWriterFinish(&mut bw).unwrap(), expected);
    }
}
