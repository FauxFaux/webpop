use super::{Case, SEED, random};
use libwebpop::internal::enc::vp8l_enc::*;
use libwebpop::internal::utils::bit_writer_utils::*;
use libwebpop::internal::utils::huffman_encode_utils::*;

fn inspect(bw: &VP8LBitWriter, out: &mut Vec<u32>) {
    out.push(VP8LBitWriterNumBytes(bw) as u32);
    let mut copy = VP8LBitWriterClone(bw).unwrap();
    let bytes = VP8LBitWriterFinish(&mut copy).unwrap();
    out.push(bytes.len() as u32);
    out.extend(bytes.iter().copied().map(u32::from));
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let mut out = Vec::new();
    match case.function.as_str() {
        "VP8LBitWriter" => {
            let mut bw = VP8LBitWriterInit(a[0] as usize).unwrap();
            let mut saved = VP8LBitWriterInit(0).unwrap();
            for op in a[1..].chunks_exact(3) {
                match op[0] {
                    0 => VP8LPutBits(&mut bw, op[1], op[2]).unwrap(),
                    1 => saved = VP8LBitWriterClone(&bw).unwrap(),
                    2 => VP8LBitWriterReset(&saved, &mut bw),
                    3 => VP8LBitWriterSwap(&mut bw, &mut saved),
                    4 => {
                        VP8LBitWriterFinish(&mut bw).unwrap();
                    }
                    5 => inspect(&bw, &mut out),
                    6 => {
                        VP8LBitWriterWipeOut(&mut bw);
                        bw = VP8LBitWriterInit(op[1] as usize).unwrap();
                    }
                    _ => panic!("unknown writer operation"),
                }
            }
            inspect(&bw, &mut out);
        }
        "VP8LCreateHuffmanTree" => {
            let mut histogram = a[1..].to_vec();
            let code = VP8LCreateHuffmanTree(&mut histogram, a[0]).unwrap();
            out.extend(histogram);
            out.extend(code.code_lengths.into_iter().map(u32::from));
            out.extend(code.codes.into_iter().map(u32::from));
        }
        "VP8LCreateCompressedHuffmanTree" => {
            let tree = HuffmanTreeCode {
                code_lengths: a.iter().map(|&v| v as u8).collect(),
                codes: vec![0; a.len()],
            };
            for token in VP8LCreateCompressedHuffmanTree(&tree).unwrap() {
                out.extend([u32::from(token.code), u32::from(token.extra_bits)]);
            }
        }
        "StoreHuffmanCode" => {
            let n = a[3] as usize;
            let mut histogram = a[4..4 + n].to_vec();
            let mut code = VP8LCreateHuffmanTree(&mut histogram, a[2]).unwrap();
            let mut bw = VP8LBitWriterInit(0).unwrap();
            VP8LPutBits(&mut bw, a[0], a[1]).unwrap();
            StoreHuffmanCode(&mut bw, &code).unwrap();
            ClearHuffmanTreeIfOnlyOneSymbol(&mut code);
            for value in a[4 + n..].chunks_exact(3) {
                WriteHuffmanCodeWithExtraBits(
                    &mut bw,
                    &code,
                    value[0] as usize,
                    value[1],
                    value[2],
                )
                .unwrap();
            }
            VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
            inspect(&bw, &mut out);
            out.extend(code.code_lengths.into_iter().map(u32::from));
            out.extend(code.codes.into_iter().map(u32::from));
        }
        _ => unreachable!(),
    }
    out
}

fn mask(bits: u32) -> u32 {
    if bits == 32 {
        u32::MAX
    } else {
        (1 << bits) - 1
    }
}

fn tree_case(histogram: Vec<u32>, depth: u32) -> Case {
    let mut a = vec![depth];
    a.extend(histogram);
    Case::new("VP8LCreateHuffmanTree", a)
}

fn store_case(histogram: Vec<u32>, prefix_bits: u32, prefix: u32) -> Case {
    let mut a = vec![prefix, prefix_bits, 15, histogram.len() as u32];
    a.extend_from_slice(&histogram);
    for (i, &count) in histogram.iter().enumerate() {
        if count != 0 {
            // Real symbols exercise the single-symbol clearing rule and
            // concatenation of code bits and length/distance extra bits.
            let bits = i as u32 % 17;
            a.extend([i as u32, (i as u32 * 997) & mask(bits), bits]);
        }
    }
    Case::new("StoreHuffmanCode", a)
}

pub fn entropy_coding_fixture_cases() -> Vec<Case> {
    let mut cases = vec![
        Case::new("VP8LBitWriter", vec![0]),
        Case::new(
            "VP8LBitWriter",
            vec![
                1,
                0,
                u32::MAX,
                0,
                0,
                5,
                3,
                0,
                u32::MAX,
                32,
                5,
                0,
                0,
                1,
                0,
                0,
                0,
                0x7fffffff,
                31,
                0,
                1,
                1,
                5,
                0,
                0,
                2,
                0,
                0,
                0,
                9,
                4,
                3,
                0,
                0,
                5,
                0,
                0,
                3,
                0,
                0,
                4,
                0,
                0,
                4,
                0,
                0,
                5,
                0,
                0,
                6,
                1,
                0,
                0,
                0xab,
                8,
            ],
        ),
    ];
    let histograms = vec![
        vec![],
        vec![0],
        vec![7],
        vec![0; 19],
        vec![0, 0, 5],
        vec![1, 1, 1],
        vec![5, 5, 5, 5, 5, 5, 5, 5],
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        vec![0, 0, 0, 1, 2, 1, 2, 3, 0, 0, 0, 0, 0, 7],
        vec![50, 1, 1, 1, 1, 1, 1, 90, 0, 0, 0, 2],
        (0..24).map(|i| 1 << i).collect(),
        (0..40).map(|i| if i < 8 { 1 << i } else { 0 }).collect(),
    ];
    for histogram in histograms {
        cases.push(tree_case(histogram.clone(), 15));
        cases.push(store_case(histogram, 3, 5));
    }
    // Force multiple count_min retries rather than accepting a deep tree.
    cases.push(tree_case((0..16).map(|i| 1 << i).collect(), 5));
    cases.push(tree_case((0..32).map(|i| 1 << (i % 24)).collect(), 7));
    for symbol in [0, 1, 2, 255, 256, 279, 2327] {
        let mut histogram = vec![0; symbol + 1];
        histogram[symbol] = 11;
        cases.push(store_case(histogram, 0, 0));
    }
    for (a, b) in [(0, 1), (0, 255), (1, 256), (255, 256)] {
        let mut histogram = vec![0; b + 1];
        histogram[a] = 19;
        histogram[b] = 7;
        cases.push(store_case(histogram, 31, 0x76543210));
    }
    // Long trailing-zero token sequences exercise trimmed-length storage.
    for n in [280, 2328] {
        let mut histogram = vec![0; n];
        histogram[..5].copy_from_slice(&[1, 100, 2, 100, 1]);
        cases.push(store_case(histogram, 7, 0x55));
    }
    let mut histogram = vec![0; 2328];
    histogram[..4].fill(1);
    cases.push(store_case(histogram, 0, 0)); // trimmed_length == 2
    for value in [0, 1, 8, 15] {
        for run in [0, 1, 2, 3, 4, 6, 7, 8, 10, 11, 12, 138, 139, 140, 277] {
            cases.push(Case::new(
                "VP8LCreateCompressedHuffmanTree",
                vec![value; run],
            ));
        }
    }
    cases.push(Case::new(
        "VP8LCreateCompressedHuffmanTree",
        vec![8, 8, 8, 0, 0, 0, 8, 8, 8, 8, 1, 1, 1, 1],
    ));
    cases
}

pub fn entropy_coding_cases() -> Vec<Case> {
    let mut cases = entropy_coding_fixture_cases();
    let mut state = SEED;
    // Every pending offset/next-write width, including ignored zero-bit values.
    for offset in 0..64 {
        for bits in 0..=32 {
            let mut a = vec![0];
            for _ in 0..offset {
                a.extend([0, random(&mut state) & 1, 1]);
            }
            a.extend([1, 0, 0, 0, random(&mut state) & mask(bits), bits, 5, 0, 0]);
            a.extend([2, 0, 0, 0, random(&mut state) & mask(bits), bits, 5, 0, 0]);
            a.extend([3, 0, 0, 0, 1, 1, 4, 0, 0, 4, 0, 0]);
            cases.push(Case::new("VP8LBitWriter", a));
        }
    }
    // Cross the C buffer's initial 1 KiB and subsequent growth boundaries.
    for hint in [0, 1, 1023, 1024, 32768] {
        let mut a = vec![hint];
        for i in 0..10000 {
            a.extend([0, random(&mut state), 32]);
            if i == 255 || i == 8192 {
                a.extend([5, 0, 0, 1, 0, 0]);
            }
        }
        a.extend([5, 0, 0, 2, 0, 0, 0, 0x7fffffff, 31, 3, 0, 0, 5, 0, 0]);
        a.extend([6, 0, 0, 0, 1, 1]);
        cases.push(Case::new("VP8LBitWriter", a));
    }
    // Exhaust short populations: ties, zeros and RLE smoothing boundaries.
    for n in 0..=7 {
        for mut pattern in 0..3_u32.pow(n) {
            let histogram = (0..n)
                .map(|_| {
                    let v = pattern % 3;
                    pattern /= 3;
                    v
                })
                .collect::<Vec<_>>();
            cases.push(tree_case(histogram.clone(), 7));
            cases.push(store_case(histogram, 1, 1));
        }
    }
    for _ in 0..1024 {
        let size = [19, 40, 256, 280, 2328][random(&mut state) as usize % 5];
        let mode = random(&mut state) % 5;
        let histogram = (0..size)
            .map(|_| {
                let v = random(&mut state);
                match mode {
                    0 => v % 4,
                    1 => {
                        if v % 11 == 0 {
                            1 + v % 10000
                        } else {
                            0
                        }
                    }
                    2 => 1 << (v % 16),
                    3 => 50 + v % 7,
                    _ => v % 100000,
                }
            })
            .collect::<Vec<_>>();
        cases.push(tree_case(histogram.clone(), 15));
        let prefix_bits = random(&mut state) % 32;
        let prefix = random(&mut state) & mask(prefix_bits);
        cases.push(store_case(histogram, prefix_bits, prefix));
        let mut depths = Vec::new();
        while depths.len() < size {
            let value = random(&mut state) % 16;
            let run = 1 + random(&mut state) as usize % 160;
            depths.extend(std::iter::repeat_n(value, run.min(size - depths.len())));
        }
        cases.push(Case::new("VP8LCreateCompressedHuffmanTree", depths));
    }
    cases
}
