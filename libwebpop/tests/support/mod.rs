#![allow(dead_code)]

pub mod entropy_coding;
pub mod matches;
pub mod references;

use libwebpop::dsp::{lossless::*, lossless_common::*, lossless_enc::*};
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
            "VP8LCostEstimates" | "VP8LTrace" | "VP8LGetBackwardReferences" | "VP8LMatches" => {
                matches::evaluate(self)
            }
            "VP8LBackwardRefs"
            | "PopulationCost"
            | "VP8LHistogram"
            | "VP8LHistogramSet"
            | "VP8LDistanceToPlaneCode" => references::evaluate(self),
            "VP8LBitWriter"
            | "VP8LCreateHuffmanTree"
            | "VP8LCreateCompressedHuffmanTree"
            | "StoreHuffmanCode" => entropy_coding::evaluate(self),
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
            "VP8LFastLog2" => vec![VP8LFastLog2(a[0])],
            "VP8LFastSLog2" => words64(VP8LFastSLog2(a[0])),
            "FastLog2Slow_C" => vec![FastLog2Slow_C(a[0])],
            "FastSLog2Slow_C" => words64(FastSLog2Slow_C(a[0])),
            "RightShiftRound" => words64(RightShiftRound(read64(a), a[2])),
            "DivRound" => words64(DivRound(read64(a) as i64, read64(&a[2..]) as i64) as u64),
            "VP8LPredictors" => vec![VP8LPredictors[a[0] as usize](
                a[1],
                a[2..5].try_into().unwrap(),
            )],
            "VP8LPredictorsSub" => {
                let len = (a.len() - 4) / 2;
                let mut out = vec![0; len];
                VP8LPredictorsSub[a[0] as usize](&a[1..len + 2], &a[len + 2..], &mut out);
                out
            }
            "ShannonEntropy_C" => words64(ShannonEntropy_C(a)),
            "CombinedShannonEntropy_C" => words64(CombinedShannonEntropy_C(
                a[..256].try_into().unwrap(),
                a[256..].try_into().unwrap(),
            )),
            "VP8LBitEntropyInit"
            | "VP8LBitsEntropyUnrefined"
            | "GetEntropyUnrefined_C"
            | "GetCombinedEntropyUnrefined_C" => {
                // Dirty initial state verifies that every output field resets.
                let mut e = VP8LBitEntropy {
                    entropy: 999,
                    sum: 999,
                    nonzeros: 999,
                    max_val: 999,
                    nonzero_code: 999,
                };
                let mut stats = VP8LStreaks {
                    counts: [999; 2],
                    streaks: [[999; 2]; 2],
                };
                match self.function.as_str() {
                    "VP8LBitEntropyInit" => VP8LBitEntropyInit(&mut e),
                    "VP8LBitsEntropyUnrefined" => VP8LBitsEntropyUnrefined(a, &mut e),
                    "GetEntropyUnrefined_C" => GetEntropyUnrefined_C(a, &mut e, &mut stats),
                    _ => GetCombinedEntropyUnrefined_C(
                        &a[..a.len() / 2],
                        &a[a.len() / 2..],
                        &mut e,
                        &mut stats,
                    ),
                }
                let mut out = words64(e.entropy);
                out.extend([e.sum, e.nonzeros as u32, e.max_val, e.nonzero_code]);
                if self.function.starts_with("Get") {
                    out.extend(stats.counts.map(|v| v as u32));
                    out.extend(stats.streaks.into_iter().flatten().map(|v| v as u32));
                }
                out
            }
            "ExtraCost_C" => vec![ExtraCost_C(a)],
            "AddVector_C" | "AddVectorEq_C" | "VectorMismatch_C" => {
                let (a, b) = a.split_at(a.len() / 2);
                if self.function == "VectorMismatch_C" {
                    vec![VectorMismatch_C(a, b) as u32]
                } else {
                    let mut out = b.to_vec();
                    if self.function == "AddVector_C" {
                        AddVector_C(a, b, &mut out);
                    } else {
                        AddVectorEq_C(a, &mut out);
                    }
                    out
                }
            }
            "VP8LBundleColorMap_C" => {
                let row: Vec<_> = a[1..].iter().map(|&v| v as u8).collect();
                let mut out = vec![0; row.len().div_ceil(1 << a[0])];
                VP8LBundleColorMap_C(&row, a[0], &mut out);
                out
            }
            "VP8LCollectColorRedTransforms_C" | "VP8LCollectColorBlueTransforms_C" => {
                let mut histo: [u32; 256] = a[5..261].try_into().unwrap();
                if self.function == "VP8LCollectColorRedTransforms_C" {
                    VP8LCollectColorRedTransforms_C(
                        &a[261..],
                        a[0] as usize,
                        a[1] as usize,
                        a[2] as usize,
                        a[3] as i32,
                        &mut histo,
                    );
                } else {
                    VP8LCollectColorBlueTransforms_C(
                        &a[261..],
                        a[0] as usize,
                        a[1] as usize,
                        a[2] as usize,
                        a[3] as i32,
                        a[4] as i32,
                        &mut histo,
                    );
                }
                histo.to_vec()
            }
            name => panic!("no Rust implementation for {name}"),
        }
    }
}

pub fn words64(v: u64) -> Vec<u32> {
    vec![v as u32, (v >> 32) as u32]
}
fn read64(a: &[u32]) -> u64 {
    u64::from(a[0]) | (u64::from(a[1]) << 32)
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

/// Milestone 1 scalar DSP coverage, shared by fixtures and the native backend.
pub fn dsp_cases() -> Vec<Case> {
    let mut cases = dsp_fixture_cases();
    let mut push = |name: &str, input: Vec<u32>| cases.push(Case::new(name, input));
    let mut state = SEED;
    // Every lookup entry and every value in both integer approximation ranges.
    for v in 0..=65536 {
        push("VP8LFastLog2", vec![v]);
        push("VP8LFastSLog2", vec![v]);
    }
    for bit in 8..=28 {
        for v in [(1 << bit) - 1, 1 << bit, (1 << bit) + 1] {
            if v >= 256 {
                push("FastLog2Slow_C", vec![v]);
                push("FastSLog2Slow_C", vec![v]);
            }
        }
    }
    for _ in 0..4096 {
        let v = random(&mut state) % (1 << 28);
        push("VP8LFastLog2", vec![v]);
        push("VP8LFastSLog2", vec![v]);
    }
    for shift in 0..64 {
        for v in [0, 1, u64::MAX, 1_u64 << shift, (1_u64 << shift) - 1] {
            let mut a = words64(v);
            a.push(shift);
            push("RightShiftRound", a);
        }
    }
    for a in -128_i64..=128 {
        for b in -16_i64..=16 {
            if b == 0 {
                continue;
            }
            let mut input = words64(a as u64);
            input.extend(words64(b as u64));
            push("DivRound", input);
        }
    }
    // Exhaust the byte-pair domain for averaging and signed half prediction.
    for a in 0..256 {
        for b in 0..256 {
            let a = a * 0x01010101;
            let b = b * 0x01010101;
            push("VP8LPredictors", vec![7, a, b, b, b]);
            push("VP8LPredictors", vec![13, a, b, a, b]);
        }
    }
    // Boundary triples cover clamp saturation, odd/negative half differences,
    // select ties and nested average rounding in every lane.
    let bytes = [0_u32, 1, 2, 3, 127, 128, 254, 255];
    for a in bytes {
        for b in bytes {
            for c in bytes {
                for mode in 0..14 {
                    push(
                        "VP8LPredictors",
                        vec![
                            mode,
                            a * 0x01010101,
                            b * 0x01010101,
                            c * 0x01010101,
                            (255 - a) * 0x01010101,
                        ],
                    );
                }
            }
        }
    }
    for _ in 0..1024 {
        let neighbors: Vec<_> = (0..4).map(|_| random(&mut state)).collect();
        for mode in 0..14 {
            let mut input = vec![mode];
            input.extend_from_slice(&neighbors);
            push("VP8LPredictors", input);
        }
    }
    for len in [0, 1, 2, 3, 7, 16, 31, 256, 1025] {
        for mode in 0..14 {
            let mut input = vec![mode];
            input.extend((0..2 * len + 3).map(|_| random(&mut state)));
            push("VP8LPredictorsSub", input);
        }
        let a: Vec<_> = (0..len).map(|_| random(&mut state)).collect();
        let b: Vec<_> = (0..len).map(|_| random(&mut state)).collect();
        let mut both = a.clone();
        both.extend_from_slice(&b);
        push("AddVector_C", both.clone());
        push("AddVectorEq_C", both);
        // First mismatch at every position, plus an entirely equal vector.
        for position in 0..=len {
            let mut b = a.clone();
            if position < len {
                b[position] ^= 1;
            }
            let mut both = a.clone();
            both.extend(b);
            push("VectorMismatch_C", both);
        }
    }
    // All short 3-valued distributions: zero/single symbols and repeated bins.
    for len in 1..=7 {
        for mut pattern in 0..3_usize.pow(len) {
            let mut a = Vec::new();
            for _ in 0..len {
                a.push((pattern % 3) as u32);
                pattern /= 3;
            }
            push("ShannonEntropy_C", a.clone());
            push("VP8LBitsEntropyUnrefined", a.clone());
            push("GetEntropyUnrefined_C", a.clone());
            a.extend((0..len).map(|_| random(&mut state) % 3));
            push("GetCombinedEntropyUnrefined_C", a);
        }
    }
    for len in [1, 2, 3, 4, 5, 16, 256, 280, 512, 2328] {
        for kind in 0..8 {
            let a: Vec<_> = (0..len)
                .map(|i| match kind {
                    0 => 0,
                    1 => 1,
                    2 => 65536,
                    3 => {
                        if i % 8 < 4 {
                            0
                        } else {
                            7
                        }
                    }
                    4 => (i / 3 % 5) as u32,
                    5 => (i / 4 % 5) as u32,
                    6 => {
                        if i == len - 1 {
                            1 << 27
                        } else {
                            0
                        }
                    }
                    _ => random(&mut state) % 10000,
                })
                .collect();
            push("ShannonEntropy_C", a.clone());
            push("VP8LBitsEntropyUnrefined", a.clone());
            push("GetEntropyUnrefined_C", a.clone());
            if len >= 6 && len % 2 == 0 {
                push("ExtraCost_C", a.clone());
            }
            let mut both = a;
            both.extend((0..len).map(|_| random(&mut state) % 1000));
            if len == 256 {
                push("CombinedShannonEntropy_C", both.clone());
            }
            push("GetCombinedEntropyUnrefined_C", both);
        }
    }
    for _ in 0..128 {
        push(
            "CombinedShannonEntropy_C",
            (0..512).map(|_| random(&mut state) % 100000).collect(),
        );
        push("ExtraCost_C", (0..40).map(|_| random(&mut state)).collect());
    }
    for xbits in 0..=3 {
        let limit = 1 << (8 >> xbits);
        for len in 0..=65 {
            for kind in 0..3 {
                let mut input = vec![xbits];
                input.extend((0..len).map(|i| match kind {
                    0 => limit - 1,
                    1 => i % limit,
                    _ => random(&mut state) % limit,
                }));
                push("VP8LBundleColorMap_C", input);
            }
        }
    }
    // All signed coefficients and channels, padded rectangular tiles, initial
    // nonzero bins and wraparound verify accumulation rather than replacement.
    for coeff in 0..256 {
        let pixels: Vec<_> = (0..272)
            .map(|i| 0x800000ff | ((i % 256) << 8) | ((255 - i % 256) << 16))
            .collect();
        for (name, c0, c1) in [
            ("VP8LCollectColorRedTransforms_C", coeff, 0),
            ("VP8LCollectColorBlueTransforms_C", coeff, 0),
            ("VP8LCollectColorBlueTransforms_C", 0, coeff),
        ] {
            let mut input = vec![17, 16, 16, c0, c1];
            input.extend((0..256).map(|i| if i % 2 == 0 { u32::MAX } else { i }));
            input.extend_from_slice(&pixels);
            push(name, input);
        }
    }
    for (width, height, stride) in [
        (0, 0, 0),
        (0, 3, 4),
        (3, 0, 4),
        (1, 1, 1),
        (7, 5, 11),
        (16, 17, 16),
    ] {
        for _ in 0..16 {
            let mut input = vec![
                stride,
                width,
                height,
                random(&mut state) % 256,
                random(&mut state) % 256,
            ];
            input.extend((0..256).map(|_| random(&mut state)));
            input.extend((0..stride * height).map(|_| random(&mut state)));
            push("VP8LCollectColorRedTransforms_C", input.clone());
            push("VP8LCollectColorBlueTransforms_C", input);
        }
    }
    cases
}

/// Compact edge cases whose C outputs are also committed for portable tests.
pub fn dsp_fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut push = |name: &str, a: Vec<u32>| cases.push(Case::new(name, a));
    for v in [
        0, 1, 2, 3, 255, 256, 257, 4095, 4096, 4097, 65535, 65536, 65537, 268435456,
    ] {
        push("VP8LFastLog2", vec![v]);
        push("VP8LFastSLog2", vec![v]);
        if v >= 256 {
            push("FastLog2Slow_C", vec![v]);
            push("FastSLog2Slow_C", vec![v]);
        }
    }
    for mode in 0..14 {
        push(
            "VP8LPredictors",
            vec![mode, 0x00ff0181, 0xff8002fe, 0x7f030080, 0x01027fff],
        );
        push(
            "VP8LPredictors",
            vec![mode, 0x02020202, 0x01010101, 0, 0xffffffff],
        );
        push(
            "VP8LPredictorsSub",
            vec![
                mode, 0x01234567, 0xff00ff00, 0x8901fecd, 0x10203040, 0xabcdef98, 0x98765432,
                0x99887766, 0x12345678, 0x13579bdf,
            ],
        );
    }
    push("VP8LBitEntropyInit", vec![]);
    for a in [
        vec![],
        vec![0],
        vec![0; 8],
        vec![0, 0, 7, 7, 7, 7, 7, 0, 1, 1, 1],
        vec![65535, 65536, 65537, 3, 1, 0],
        vec![0, 0, 1 << 27, 0],
        vec![1, 2, 3, 4],
    ] {
        push("ShannonEntropy_C", a.clone());
        push("VP8LBitsEntropyUnrefined", a.clone());
        if !a.is_empty() {
            push("GetEntropyUnrefined_C", a.clone());
            let mut both = a.clone();
            both.extend(a.into_iter().rev());
            push("GetCombinedEntropyUnrefined_C", both);
        }
    }
    let mut combined = vec![0; 512];
    combined[0] = 65536;
    combined[255] = 1;
    combined[256] = 7;
    combined[257] = 65535;
    push("CombinedShannonEntropy_C", combined);
    for xbits in 0..=3 {
        let mut input = vec![xbits];
        input.extend((0..11).map(|i| i % (1 << (8 >> xbits))));
        push("VP8LBundleColorMap_C", input);
    }
    for name in ["AddVector_C", "AddVectorEq_C", "VectorMismatch_C"] {
        push(name, vec![0, u32::MAX, 1, 7, 1, 1, u32::MAX, 7]);
    }
    push("ExtraCost_C", (0..40).map(|v| v * 10000).collect());
    for name in [
        "VP8LCollectColorRedTransforms_C",
        "VP8LCollectColorBlueTransforms_C",
    ] {
        let mut input = vec![3, 2, 2, 255, 128];
        input.extend((0..256).map(|i| if i % 2 == 0 { u32::MAX } else { i }));
        input.extend([0xff00ffff, 0x0180ff00, 0xdeadbeef, 0x807f8081, 0xffffffff]);
        push(name, input);
    }
    push("RightShiftRound", vec![u32::MAX, 0, 1]);
    push("RightShiftRound", vec![u32::MAX, u32::MAX, 63]);
    let mut div = words64((-7_i64) as u64);
    div.extend(words64(2));
    push("DivRound", div);
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
