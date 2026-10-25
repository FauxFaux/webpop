use super::{Case, SEED, random};
use libwebpop::internal::utils::bit_writer_utils::*;

// Request: initial reservation, full-trace flag, then (op, value, bits) triples.
// Append (op 5) additionally consumes `value` byte words after its triple.
// Ops: probability bit, uniform bit, unsigned bits, signed bits, finish, append,
// wipe, reinitialize. State records omit allocation capacities and addresses.
pub fn inspect(bw: &VP8BitWriter, result: u32, out: &mut Vec<u32>) {
    let pos = VP8BitWriterPos(bw);
    let hash = VP8BitWriterBuf(bw).iter().fold(2166136261u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    out.extend([
        result,
        bw.range as u32,
        bw.value as u32,
        bw.run as u32,
        bw.nb_bits as u32,
        pos as u32,
        (pos >> 32) as u32,
        VP8BitWriterSize(bw) as u32,
        hash,
    ]);
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let mut bw = VP8BitWriterInit(a[0] as usize).unwrap();
    let mut out = Vec::new();
    inspect(&bw, 0, &mut out);
    let mut at = 2;
    let mut result = 0;
    while at < a.len() {
        let (op, value, bits) = (a[at], a[at + 1], a[at + 2]);
        at += 3;
        result = 0;
        match op {
            0 => result = VP8PutBit(&mut bw, value as i32, bits as u8).unwrap() as u32,
            1 => result = VP8PutBitUniform(&mut bw, value as i32).unwrap() as u32,
            2 => VP8PutBits(&mut bw, value, bits).unwrap(),
            3 => VP8PutSignedBits(&mut bw, value as i32, bits).unwrap(),
            4 => {
                VP8BitWriterFinish(&mut bw).unwrap();
            }
            5 => {
                let data: Vec<_> = a[at..at + value as usize]
                    .iter()
                    .map(|&v| v as u8)
                    .collect();
                at += value as usize;
                result = u32::from(VP8BitWriterAppend(&mut bw, &data).unwrap());
            }
            6 => VP8BitWriterWipeOut(&mut bw),
            7 => bw = VP8BitWriterInit(value as usize).unwrap(),
            _ => unreachable!(),
        }
        if a[1] != 0 {
            inspect(&bw, result, &mut out);
        }
    }
    if a[1] == 0 {
        inspect(&bw, result, &mut out);
    }
    out.push(VP8BitWriterSize(&bw) as u32);
    out.extend(VP8BitWriterBuf(&bw).iter().map(|&b| u32::from(b)));
    out
}

pub fn case(reserve: u32, trace: bool, operations: Vec<u32>) -> Case {
    let mut a = vec![reserve, u32::from(trace)];
    a.extend(operations);
    Case::new("VP8BitWriter", a)
}

// A minimized, valid coding sequence that resolves a deferred byte with carry.
// Retain it explicitly: carries across an outstanding run are rare in noise.
pub const CARRY_BITS: [(i32, u8); 23] = [
    (0, 163),
    (1, 155),
    (1, 240),
    (1, 16),
    (0, 106),
    (0, 82),
    (1, 229),
    (0, 216),
    (0, 188),
    (1, 11),
    (1, 108),
    (0, 12),
    (1, 115),
    (0, 146),
    (0, 90),
    (0, 158),
    (1, 135),
    (0, 125),
    (1, 156),
    (1, 189),
    (0, 177),
    (1, 169),
    (1, 238),
];

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    cases.push(case(0, true, vec![]));
    cases.push(case(0, true, vec![4, 0, 0]));
    // Every probability, both branches, alternating and grouped outcomes,
    // across the first flush and repeated flush boundaries.
    for prob in 0..=255 {
        for pattern in 0..4 {
            for len in [1, 8, 17] {
                let mut ops = Vec::new();
                for i in 0..len {
                    let bit = match pattern {
                        0 => 0,
                        1 => 1,
                        2 => i % 2,
                        _ => u32::from(i % 8 < 4),
                    };
                    ops.extend([0, bit, prob]);
                }
                ops.extend([4, 0, 0]);
                cases.push(case(prob % 3, true, ops));
            }
        }
    }
    // Exhaustive short uniform strings cover flush rounding and carry prefixes.
    for value in 0..4096 {
        cases.push(case(0, true, vec![2, value, 12, 4, 0, 0]));
    }
    // Every supported field width and signed limits, including zero's flag.
    for width in 1..=31 {
        for value in [0, 1, (1u32 << width) - 1, 1 << (width - 1), u32::MAX] {
            cases.push(case(0, true, vec![2, value, width, 4, 0, 0]));
        }
    }
    for width in 0..=30 {
        let limit = (1i32 << width) - 1;
        for value in [-limit, -1i32.min(limit), 0, 1i32.min(limit), limit] {
            cases.push(case(1, true, vec![3, value as u32, width, 4, 0, 0]));
        }
    }
    // Noncanonical truthy inputs must retain the C return value.
    for value in [0, 1, 2, 1 << 30, i32::MAX as u32, u32::MAX, i32::MIN as u32] {
        cases.push(case(0, true, vec![0, value, 128, 1, value, 0, 4, 0, 0]));
    }
    // Long deferred 0xff runs, with and without eventual carry. Large fields
    // and byte append sizes also cross the C writer's 1024-byte growth boundary.
    for len in [7, 8, 9, 15, 16, 17, 127, 1023, 4096] {
        for tail in [0, 1] {
            let mut ops = Vec::new();
            for _ in 0..len {
                ops.extend([1, 1, 0]);
            }
            ops.extend([0, tail, 255, 4, 0, 0]);
            cases.push(case(0, true, ops));
        }
    }
    for len in [0, 1, 1023, 1024, 1025, 4096] {
        let mut ops = vec![5, len, 0];
        ops.extend((0..len).map(|v| (v * 73 + 255) & 255));
        ops.extend([1, 1, 0, 5, 3, 0, 9, 8, 7, 4, 0, 0, 5, len, 0]);
        ops.extend((0..len).map(|v| (v * 19 + 7) & 255));
        ops.extend([6, 0, 0, 6, 0, 0, 7, 0, 0, 2, 0x5ad, 11, 4, 0, 0]);
        cases.push(case(u32::from(len == 0), true, ops));
    }
    let mut seed = SEED;
    for index in 0..768 {
        let mut ops = Vec::new();
        for _ in 0..[1, 7, 8, 9, 31, 64, 257, 1024][index % 8] {
            let value = random(&mut seed);
            match value % 4 {
                0 => ops.extend([0, (value >> 8) & 1, random(&mut seed) & 255]),
                1 => ops.extend([1, value >> 9, 0]),
                2 => ops.extend([2, random(&mut seed), 1 + (value >> 16) % 31]),
                _ => {
                    let width = 1 + (value >> 16) % 30;
                    let magnitude = (random(&mut seed) & ((1 << width) - 1)) as i32;
                    let signed = if value & 256 == 0 {
                        magnitude
                    } else {
                        -magnitude
                    };
                    ops.extend([3, signed as u32, width]);
                }
            }
        }
        ops.extend([4, 0, 0]);
        cases.push(case((index % 5) as u32 * 513, index % 8 != 7, ops));
    }
    let mut ops = Vec::new();
    for (bit, prob) in CARRY_BITS {
        ops.extend([0, bit as u32, u32::from(prob)]);
    }
    ops.extend([4, 0, 0]);
    cases.push(case(0, true, ops));
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let all = cases();
    // Include hand-picked long runs, lifecycle and field limits as well as the
    // compact probability sweep. Keep long random traces out of text fixtures.
    all.iter()
        .take(2)
        .chain(all.iter().skip(2).take(3072).step_by(67))
        .chain(all.iter().skip(3074).take(4096).step_by(257))
        .chain(all.iter().skip(7170).take(310).step_by(13))
        .chain(all.iter().skip(7480).take(31))
        .chain(all.iter().skip(7511).step_by(97))
        .chain(all.last())
        .cloned()
        .map(|mut c| {
            if c.input.len() > 1024 {
                c.input[1] = 0;
            }
            c
        })
        .collect()
}

// Independent streaming decoder: use interval sizes, no encoder normalization
// tables, deferred-byte runs, carry propagation or encoder accumulator.
pub struct Decoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
    code: u32,
    size: u32,
}
impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        let mut d = Self {
            bytes,
            cursor: 0,
            code: 0,
            size: 255,
        };
        for _ in 0..8 {
            d.code = 2 * d.code + d.next();
        }
        d
    }
    fn next(&mut self) -> u32 {
        assert!(
            self.cursor < self.bytes.len() * 8,
            "decoder exhausted terminated stream"
        );
        let bit = (self.bytes[self.cursor / 8] >> (7 - self.cursor % 8)) & 1;
        self.cursor += 1;
        u32::from(bit)
    }
    pub fn bit(&mut self, probability: u8) -> bool {
        let zero_size = ((self.size - 1) * u32::from(probability) / 256) + 1;
        let bit = self.code >= zero_size;
        if bit {
            self.code -= zero_size;
            self.size -= zero_size;
        } else {
            self.size = zero_size;
        }
        while self.size < 128 {
            self.size *= 2;
            self.code = 2 * self.code + self.next();
        }
        bit
    }
}
