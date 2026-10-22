mod support;

use libwebpop::enc::vp8l_enc::*;
use libwebpop::utils::bit_writer_utils::*;
use libwebpop::utils::huffman_encode_utils::*;
use std::collections::HashMap;
use support::entropy_coding::entropy_coding_fixture_cases;

#[test]
fn writer_matches_independent_bit_sequence() {
    let mut state = support::SEED;
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut expected = Vec::new();
    let mut bit_pos = 0_usize;
    for _ in 0..10000 {
        let width = support::random(&mut state) % 33;
        let mut value = support::random(&mut state);
        if width < 32 {
            value &= (1_u32 << width) - 1;
        }
        VP8LPutBits(&mut bw, value, width).unwrap();
        for bit in 0..width {
            if bit_pos % 8 == 0 {
                expected.push(0_u8);
            }
            expected[bit_pos / 8] |= (((value >> bit) & 1) as u8) << (bit_pos % 8);
            bit_pos += 1;
        }
        assert_eq!(VP8LBitWriterNumBytes(&bw), expected.len());
    }
    assert_eq!(VP8LBitWriterFinish(&mut bw).unwrap(), expected);
    assert_eq!(VP8LBitWriterFinish(&mut bw).unwrap(), expected);
    assert!(VP8LBitWriterInit(usize::MAX).is_err());
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn read(&mut self, bits: usize) -> u32 {
        let mut value = 0;
        for i in 0..bits {
            value |= u32::from((self.data[self.pos / 8] >> (self.pos % 8)) & 1) << i;
            self.pos += 1;
        }
        value
    }
}

/// Independent canonical decoder: assign codes by sorting (depth, symbol),
/// then match incoming MSB code bits. Single-symbol alphabets consume no bits.
struct Decoder {
    single: Option<usize>,
    codes: HashMap<(usize, u32), usize>,
}
impl Decoder {
    fn new(depths: &[u8]) -> Self {
        let mut symbols: Vec<_> = depths
            .iter()
            .enumerate()
            .filter_map(|(i, &d)| (d != 0).then_some((d, i)))
            .collect();
        assert!(!symbols.is_empty());
        symbols.sort_unstable();
        let single = (symbols.len() == 1).then_some(symbols[0].1);
        let mut codes = HashMap::new();
        let mut code = 0;
        let mut previous_depth = 0;
        for (depth, symbol) in symbols {
            code <<= depth - previous_depth;
            assert!(code < (1 << depth), "oversubscribed Huffman tree");
            codes.insert((depth as usize, code), symbol);
            code += 1;
            previous_depth = depth;
        }
        if single.is_none() {
            assert_eq!(code, 1 << previous_depth, "incomplete Huffman tree");
        }
        Self { single, codes }
    }

    fn decode(&self, reader: &mut Reader<'_>) -> usize {
        if let Some(symbol) = self.single {
            return symbol;
        }
        let mut code = 0;
        for depth in 1..=15 {
            code = (code << 1) | reader.read(1);
            if let Some(&symbol) = self.codes.get(&(depth, code)) {
                return symbol;
            }
        }
        panic!("invalid Huffman code");
    }
}

fn read_header(reader: &mut Reader<'_>, size: usize) -> (Vec<u8>, Option<usize>) {
    let mut depths = vec![0; size];
    if reader.read(1) != 0 {
        let count = reader.read(1) + 1;
        let bits = if reader.read(1) == 0 { 1 } else { 8 };
        let symbol = reader.read(bits) as usize;
        depths[symbol] = 1;
        if count == 2 {
            depths[reader.read(8) as usize] = 1;
        }
        return (depths, None);
    }
    let order = [
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let count = 4 + reader.read(4) as usize;
    let mut token_depths = [0; 19];
    for &symbol in &order[..count] {
        token_depths[symbol] = reader.read(3) as u8;
    }
    let decoder = Decoder::new(&token_depths);
    let trimmed = if reader.read(1) != 0 {
        let bits = 2 + 2 * reader.read(3) as usize;
        Some(2 + reader.read(bits) as usize)
    } else {
        None
    };
    let mut pos = 0;
    let mut prev = 8;
    for _ in 0..trimmed.unwrap_or(size) {
        if pos == size {
            break;
        }
        let symbol = decoder.decode(reader);
        if symbol < 16 {
            depths[pos] = symbol as u8;
            if symbol != 0 {
                prev = symbol as u8;
            }
            pos += 1;
        } else {
            let (bits, base, value) = match symbol {
                16 => (2, 3, prev),
                17 => (3, 3, 0),
                18 => (7, 11, 0),
                _ => unreachable!(),
            };
            let run = base + reader.read(bits) as usize;
            depths[pos..pos + run].fill(value);
            pos += run;
        }
    }
    (depths, trimmed)
}

#[test]
fn serialized_headers_and_symbols_round_trip() {
    let mut trimmed_lengths = Vec::new();
    let mut cases = entropy_coding_fixture_cases();
    let mut state = support::SEED;
    for n in [19, 40, 256, 280, 2328] {
        for _ in 0..16 {
            let mut input = vec![0, 0, 15, n];
            input.extend((0..n).map(|_| support::random(&mut state) % 100));
            cases.push(support::Case::new("StoreHuffmanCode", input));
        }
    }
    for case in cases
        .iter()
        .filter(|c| c.function == "StoreHuffmanCode" && c.input[3] != 0)
    {
        let a = &case.input;
        let n = a[3] as usize;
        let mut histogram = a[4..4 + n].to_vec();
        let mut tree = VP8LCreateHuffmanTree(&mut histogram, 15).unwrap();
        let mut bw = VP8LBitWriterInit(0).unwrap();
        VP8LPutBits(&mut bw, a[0], a[1]).unwrap();
        StoreHuffmanCode(&mut bw, &tree).unwrap();
        ClearHuffmanTreeIfOnlyOneSymbol(&mut tree);
        let symbols: Vec<_> = histogram
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| (c != 0).then_some(i))
            .collect();
        for &symbol in &symbols {
            WriteHuffmanCode(&mut bw, &tree, symbol).unwrap();
        }
        VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
        let bytes = VP8LBitWriterFinish(&mut bw).unwrap();
        let mut reader = Reader {
            data: bytes,
            pos: 0,
        };
        assert_eq!(reader.read(a[1] as usize), a[0]);
        let (mut decoded, trimmed) = read_header(&mut reader, n);
        if let Some(length) = trimmed {
            trimmed_lengths.push(length);
        }
        let decoder = Decoder::new(&decoded);
        if decoded.iter().filter(|&&d| d != 0).count() == 1 {
            decoded.fill(0);
        }
        assert_eq!(decoded, tree.code_lengths, "{case:?}");
        for &symbol in &symbols {
            assert_eq!(decoder.decode(&mut reader), symbol);
        }
        assert_eq!(reader.read(11), 0x5ad);
        assert_eq!(reader.read((8 - reader.pos % 8) % 8), 0);
    }
    assert!(trimmed_lengths.contains(&2));
    assert!(trimmed_lengths.iter().any(|&n| n > 2));
}

#[test]
fn compressed_depth_runs_round_trip() {
    for case in entropy_coding_fixture_cases()
        .iter()
        .filter(|c| c.function == "VP8LCreateCompressedHuffmanTree")
    {
        let tree = HuffmanTreeCode {
            code_lengths: case.input.iter().map(|&d| d as u8).collect(),
            codes: Vec::new(),
        };
        let mut decoded = Vec::new();
        let mut prev = 8;
        for token in VP8LCreateCompressedHuffmanTree(&tree).unwrap() {
            let (value, run) = match token.code {
                16 => (prev, 3 + token.extra_bits as usize),
                17 => (0, 3 + token.extra_bits as usize),
                18 => (0, 11 + token.extra_bits as usize),
                value => {
                    if value != 0 {
                        prev = value;
                    }
                    (value, 1)
                }
            };
            decoded.extend(std::iter::repeat_n(value, run));
        }
        assert_eq!(decoded, tree.code_lengths);
    }
}
