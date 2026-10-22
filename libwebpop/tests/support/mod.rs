#![allow(dead_code)]

use libwebpop::dsp::{lossless::VP8LMultipliers, lossless_common::*, lossless_enc::*};
use libwebpop::utils::{color_cache_utils::*, utils::*};

pub const UPSTREAM_REVISION: &str = "1844042d1ca751e62e61df7617dba19c98588339";
pub const SEED: u32 = 0x6d2b79f5;

#[derive(Clone, Debug)]
pub struct Case {
    pub function: String,
    pub input: Vec<u32>,
}

impl Case {
    pub fn new(function: &str, input: impl Into<Vec<u32>>) -> Self {
        Self {
            function: function.into(),
            input: input.into(),
        }
    }

    pub fn rust(&self) -> Vec<u32> {
        let a = &self.input;
        match self.function.as_str() {
            "PutLE32" => {
                let mut bytes = [0; 4];
                PutLE32(&mut bytes, a[0]);
                bytes.into_iter().map(u32::from).collect()
            }
            "BitsLog2Floor" => vec![BitsLog2Floor(a[0])],
            "BitsCtz" => vec![BitsCtz(a[0])],
            "VP8LSubSampleSize" => vec![VP8LSubSampleSize(a[0], a[1])],
            "VP8LNearLosslessBits" => vec![VP8LNearLosslessBits(a[0])],
            "VP8LAddPixels" => vec![VP8LAddPixels(a[0], a[1])],
            "VP8LSubPixels" => vec![VP8LSubPixels(a[0], a[1])],
            "VP8LPrefixEncode" | "VP8LPrefixEncodeNoLUT" => {
                let (code, bits, value) = if self.function == "VP8LPrefixEncode" {
                    VP8LPrefixEncode(a[0])
                } else {
                    VP8LPrefixEncodeNoLUT(a[0])
                };
                vec![code, bits, value]
            }
            "VP8LPrefixEncodeBits" | "VP8LPrefixEncodeBitsNoLUT" => {
                let (code, bits) = if self.function == "VP8LPrefixEncodeBits" {
                    VP8LPrefixEncodeBits(a[0])
                } else {
                    VP8LPrefixEncodeBitsNoLUT(a[0])
                };
                vec![code, bits]
            }
            "VP8LHashPix" => vec![VP8LHashPix(a[0], a[1])],
            "VP8LColorCache" => {
                let mut cache = VP8LColorCacheInit(a[0]).unwrap();
                let mut copy = VP8LColorCacheInit(a[0]).unwrap();
                let mut out = Vec::new();
                for op in a[1..].chunks_exact(3) {
                    match op[0] {
                        0 => VP8LColorCacheInsert(&mut cache, op[2]),
                        1 => out.push(VP8LColorCacheContains(&cache, op[2]).unwrap_or(u32::MAX)),
                        2 => out.push(VP8LColorCacheGetIndex(&cache, op[2])),
                        3 => out.push(VP8LColorCacheLookup(&cache, op[1])),
                        4 => VP8LColorCacheSet(&mut cache, op[1], op[2]),
                        5 => VP8LColorCacheCopy(&cache, &mut copy),
                        6 => out.push(VP8LColorCacheLookup(&copy, op[1])),
                        _ => panic!("unknown cache operation"),
                    }
                }
                out.extend((0..1 << a[0]).map(|key| VP8LColorCacheLookup(&cache, key)));
                out.extend((0..1 << a[0]).map(|key| VP8LColorCacheLookup(&copy, key)));
                out
            }
            "VP8LSubtractGreenFromBlueAndRed_C" => {
                let mut data = a.clone();
                VP8LSubtractGreenFromBlueAndRed_C(&mut data);
                data
            }
            "VP8LTransformColor_C" => {
                let m = VP8LMultipliers {
                    green_to_red: a[0] as u8,
                    green_to_blue: a[1] as u8,
                    red_to_blue: a[2] as u8,
                };
                let mut data = a[3..].to_vec();
                VP8LTransformColor_C(&m, &mut data);
                data
            }
            name => panic!("no Rust implementation for {name}"),
        }
    }
}

pub fn fixtures() -> Vec<(Case, Vec<u32>)> {
    include_str!("../fixtures/leaves.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (input, output) = line.split_once('|').expect("fixture separator");
            let mut words = input.split_whitespace();
            let function = words.next().unwrap();
            let parse = |word| u32::from_str_radix(word, 16).expect("hexadecimal u32");
            (
                Case::new(function, words.map(parse).collect::<Vec<_>>()),
                output.split_whitespace().map(parse).collect(),
            )
        })
        .collect()
}

pub fn random(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

/// Backend-independent logical cases: no process, FFI, or guest-memory details.
pub fn differential_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut push = |name: &str, input: Vec<u32>| cases.push(Case::new(name, input));
    // Exhaust every byte pair with distinct neighboring channels to reveal
    // carries/borrows crossing byte lanes.
    for a in 0..256_u32 {
        for b in 0..256_u32 {
            let x = u32::from_le_bytes([a as u8, 255, (255 - a) as u8, 0]);
            let y = u32::from_le_bytes([b as u8, 1, (255 - b) as u8, 255]);
            push("VP8LAddPixels", vec![x, y]);
            push("VP8LSubPixels", vec![x, y]);
        }
    }
    for n in 1..=8192 {
        for name in ["VP8LPrefixEncode", "VP8LPrefixEncodeBits"] {
            push(name, vec![n]);
        }
        if n >= 3 {
            for name in ["VP8LPrefixEncodeNoLUT", "VP8LPrefixEncodeBitsNoLUT"] {
                push(name, vec![n]);
            }
        }
    }
    for bit in 0..31 {
        let boundary = 1_u32 << bit;
        for n in [boundary.saturating_sub(1), boundary, boundary + 1] {
            if n == 0 {
                continue;
            }
            push("BitsLog2Floor", vec![n]);
            push("BitsCtz", vec![n]);
            if n >= 3 {
                for name in [
                    "VP8LPrefixEncode",
                    "VP8LPrefixEncodeNoLUT",
                    "VP8LPrefixEncodeBits",
                    "VP8LPrefixEncodeBitsNoLUT",
                ] {
                    push(name, vec![n]);
                }
            }
        }
    }
    // The other boundary within each power-of-two interval is selected by
    // the second highest bit. Codes change at boundary + 1 (distance - 1).
    for bit in 0..30 {
        let boundary = 3_u32 << bit;
        for n in [boundary, boundary + 1, boundary + 2] {
            for name in [
                "VP8LPrefixEncode",
                "VP8LPrefixEncodeNoLUT",
                "VP8LPrefixEncodeBits",
                "VP8LPrefixEncodeBitsNoLUT",
            ] {
                push(name, vec![n]);
            }
        }
    }
    for bits in 0..=14 {
        for n in 0..=1025 {
            push("VP8LSubSampleSize", vec![n, bits]);
        }
    }
    for quality in 0..=100 {
        push("VP8LNearLosslessBits", vec![quality]);
    }
    let mut state = SEED;
    for _ in 0..4096 {
        let a = random(&mut state);
        let b = random(&mut state);
        push("PutLE32", vec![a]);
        push("BitsLog2Floor", vec![a.max(1)]);
        push("BitsCtz", vec![a.max(1)]);
        push("VP8LAddPixels", vec![a, b]);
        push("VP8LSubPixels", vec![a, b]);
        push("VP8LHashPix", vec![a, b % 32]);
    }
    // Every signed coefficient / signed channel pair, including arithmetic
    // right-shift rounding for negative products.
    for multiplier in 0..256 {
        let pixels: Vec<u32> = (0..256)
            .map(|c| 0x800000ff | c << 8 | (255 - c) << 16)
            .collect();
        for coeffs in [[multiplier, 0, 0], [0, multiplier, 0], [0, 0, multiplier]] {
            let mut input = coeffs.to_vec();
            input.extend_from_slice(&pixels);
            push("VP8LTransformColor_C", input);
        }
        push("VP8LSubtractGreenFromBlueAndRed_C", pixels);
    }
    for len in [0, 1, 2, 3, 7, 16, 31, 256, 1025] {
        let pixels: Vec<_> = (0..len).map(|_| random(&mut state)).collect();
        push("VP8LSubtractGreenFromBlueAndRed_C", pixels.clone());
        for _ in 0..32 {
            let mut input: Vec<_> = (0..3).map(|_| random(&mut state) & 255).collect();
            input.extend_from_slice(&pixels);
            push("VP8LTransformColor_C", input);
        }
    }
    for bits in 1..=11 {
        let mut input = vec![bits, 1, 0, 0]; // zero is present in the initial cache
        for _ in 0..2048 {
            let pixel = random(&mut state);
            let key = random(&mut state) & ((1 << bits) - 1);
            input.extend_from_slice(&[0, 0, pixel, 1, 0, pixel, 2, 0, pixel]);
            input.extend_from_slice(&[5, 0, 0, 4, key, !pixel, 3, key, 0, 6, key, 0]);
            input.extend_from_slice(&[1, 0, random(&mut state)]);
        }
        push("VP8LColorCache", input);
    }
    cases
}

#[derive(Clone, Debug)]
pub struct ImageCase {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub rgba: Vec<u8>,
}

impl ImageCase {
    pub fn request(&self) -> Case {
        let mut input = vec![self.width, self.height, self.stride as u32];
        input.extend(self.rgba.iter().copied().map(u32::from));
        Case::new("WebPEncodeLosslessRGBA", input)
    }

    pub fn check_decoded(&self, encoded: &[u8]) {
        assert_eq!(&encoded[..4], b"RIFF", "{}", self.name);
        assert_eq!(&encoded[8..16], b"WEBPVP8L", "{}", self.name);
        let decoded = image::load_from_memory_with_format(encoded, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(
            decoded.dimensions(),
            (self.width, self.height),
            "{}",
            self.name
        );
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                let expected = &self.rgba[y * self.stride + x * 4..][..4];
                let actual = &decoded.as_raw()[(y * self.width as usize + x) * 4..][..4];
                assert_eq!(actual[3], expected[3], "{} alpha at ({x}, {y})", self.name);
                if expected[3] != 0 {
                    assert_eq!(actual, expected, "{} at ({x}, {y})", self.name);
                }
            }
        }
    }
}

pub fn image_cases() -> Vec<ImageCase> {
    [
        ("one", 1, 1, 0),
        ("column", 1, 17, 3),
        ("row", 17, 1, 5),
        ("odd-mixed-alpha", 17, 13, 7),
        ("opaque", 16, 16, 0),
        ("transparent", 3, 5, 1),
        ("palette", 33, 9, 0),
    ]
    .into_iter()
    .map(|(name, width, height, padding)| {
        let stride = width as usize * 4 + padding;
        let mut rgba = vec![0xab; (height as usize - 1) * stride + width as usize * 4];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let i = (y * width as usize + x) as u32;
                let pixel = match name {
                    "transparent" => [17, 37, 91, 0],
                    "palette" => {
                        if (x + y) % 2 == 0 {
                            [255, 0, 0, 255]
                        } else {
                            [0, 255, 0, 255]
                        }
                    }
                    "opaque" => [i as u8, (i * 37) as u8, (i * 91) as u8, 255],
                    _ => [i as u8, (i * 37) as u8, (i * 91) as u8, (i % 256) as u8],
                };
                rgba[y * stride + x * 4..][..4].copy_from_slice(&pixel);
            }
        }
        ImageCase {
            name,
            width,
            height,
            stride,
            rgba,
        }
    })
    .collect()
}
