// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8l_enc.c (Huffman storage helpers).
// See COPYING and PATENTS.

use std::collections::TryReserveError;

use crate::utils::bit_writer_utils::{VP8LBitWriter, VP8LPutBits};
use crate::utils::huffman_encode_utils::{
    HuffmanTreeCode, HuffmanTreeToken, VP8LCreateCompressedHuffmanTree, VP8LCreateHuffmanTree,
};
use crate::utils::utils::BitsLog2Floor;
use crate::webp::format_constants::CODE_LENGTH_CODES;

fn StoreHuffmanTreeOfHuffmanTreeToBitMask(
    bw: &mut VP8LBitWriter,
    code_length_bitdepth: &[u8],
) -> Result<(), TryReserveError> {
    const STORAGE_ORDER: [usize; CODE_LENGTH_CODES] = [
        17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let mut codes_to_store = CODE_LENGTH_CODES;
    while codes_to_store > 4 && code_length_bitdepth[STORAGE_ORDER[codes_to_store - 1]] == 0 {
        codes_to_store -= 1;
    }
    VP8LPutBits(bw, (codes_to_store - 4) as u32, 4)?;
    for &symbol in &STORAGE_ORDER[..codes_to_store] {
        VP8LPutBits(bw, u32::from(code_length_bitdepth[symbol]), 3)?;
    }
    Ok(())
}

pub fn ClearHuffmanTreeIfOnlyOneSymbol(tree: &mut HuffmanTreeCode) {
    if tree
        .code_lengths
        .iter()
        .filter(|&&d| d != 0)
        .take(2)
        .count()
        < 2
    {
        tree.code_lengths.fill(0);
        tree.codes.fill(0);
    }
}

fn StoreHuffmanTreeToBitMask(
    bw: &mut VP8LBitWriter,
    tokens: &[HuffmanTreeToken],
    huffman_code: &HuffmanTreeCode,
) -> Result<(), TryReserveError> {
    for token in tokens {
        WriteHuffmanCode(bw, huffman_code, token.code as usize)?;
        let extra_bits = match token.code {
            16 => 2,
            17 => 3,
            18 => 7,
            _ => 0,
        };
        VP8LPutBits(bw, u32::from(token.extra_bits), extra_bits)?;
    }
    Ok(())
}

fn StoreFullHuffmanCode(
    bw: &mut VP8LBitWriter,
    tree: &HuffmanTreeCode,
) -> Result<(), TryReserveError> {
    VP8LPutBits(bw, 0, 1)?;
    let tokens = VP8LCreateCompressedHuffmanTree(tree)?;
    let mut histogram = [0; CODE_LENGTH_CODES];
    for token in &tokens {
        histogram[token.code as usize] += 1;
    }
    let mut huffman_code = VP8LCreateHuffmanTree(&mut histogram, 7)?;
    StoreHuffmanTreeOfHuffmanTreeToBitMask(bw, &huffman_code.code_lengths)?;
    ClearHuffmanTreeIfOnlyOneSymbol(&mut huffman_code);

    let mut trailing_zero_bits = 0;
    let mut trimmed_length = tokens.len();
    for token in tokens.iter().rev() {
        let ix = token.code as usize;
        if ix == 0 || ix == 17 || ix == 18 {
            trimmed_length -= 1;
            trailing_zero_bits += u32::from(huffman_code.code_lengths[ix]);
            trailing_zero_bits += match ix {
                17 => 3,
                18 => 7,
                _ => 0,
            };
        } else {
            break;
        }
    }
    let write_trimmed_length = trimmed_length > 1 && trailing_zero_bits > 12;
    let length = if write_trimmed_length {
        trimmed_length
    } else {
        tokens.len()
    };
    VP8LPutBits(bw, u32::from(write_trimmed_length), 1)?;
    if write_trimmed_length {
        if trimmed_length == 2 {
            VP8LPutBits(bw, 0, 5)?;
        } else {
            let trimmed = u32::try_from(trimmed_length - 2).expect("Huffman alphabet too large");
            let nbitpairs = BitsLog2Floor(trimmed) / 2 + 1;
            assert!(nbitpairs < 9);
            VP8LPutBits(bw, nbitpairs - 1, 3)?;
            VP8LPutBits(bw, trimmed, nbitpairs * 2)?;
        }
    }
    StoreHuffmanTreeToBitMask(bw, &tokens[..length], &huffman_code)
}

pub fn StoreHuffmanCode(
    bw: &mut VP8LBitWriter,
    tree: &HuffmanTreeCode,
) -> Result<(), TryReserveError> {
    let mut count = 0;
    let mut symbols = [0; 2];
    for (i, &depth) in tree.code_lengths.iter().enumerate() {
        if depth != 0 {
            if count < 2 {
                symbols[count] = i;
            }
            count += 1;
            if count == 3 {
                break;
            }
        }
    }
    if count == 0 {
        VP8LPutBits(bw, 1, 4)?;
    } else if count <= 2 && symbols[0] < 256 && symbols[1] < 256 {
        VP8LPutBits(bw, 1, 1)?;
        VP8LPutBits(bw, (count - 1) as u32, 1)?;
        if symbols[0] <= 1 {
            VP8LPutBits(bw, 0, 1)?;
            VP8LPutBits(bw, symbols[0] as u32, 1)?;
        } else {
            VP8LPutBits(bw, 1, 1)?;
            VP8LPutBits(bw, symbols[0] as u32, 8)?;
        }
        if count == 2 {
            VP8LPutBits(bw, symbols[1] as u32, 8)?;
        }
    } else {
        StoreFullHuffmanCode(bw, tree)?;
    }
    Ok(())
}

pub fn WriteHuffmanCode(
    bw: &mut VP8LBitWriter,
    code: &HuffmanTreeCode,
    index: usize,
) -> Result<(), TryReserveError> {
    VP8LPutBits(
        bw,
        u32::from(code.codes[index]),
        u32::from(code.code_lengths[index]),
    )
}

pub fn WriteHuffmanCodeWithExtraBits(
    bw: &mut VP8LBitWriter,
    code: &HuffmanTreeCode,
    index: usize,
    bits: u32,
    n_bits: u32,
) -> Result<(), TryReserveError> {
    let depth = u32::from(code.code_lengths[index]);
    assert!(depth + n_bits <= 32);
    VP8LPutBits(
        bw,
        (bits << depth) | u32::from(code.codes[index]),
        depth + n_bits,
    )
}

/// Build the five alphabets per histogram, preserving upstream's count smoothing.
pub fn GetHuffBitLengthsAndCodes(
    histograms: &mut [super::histogram_enc::VP8LHistogram],
) -> Result<Vec<HuffmanTreeCode>, TryReserveError> {
    let mut codes = Vec::new();
    codes.try_reserve_exact(histograms.len().checked_mul(5).expect("histogram count"))?;
    for h in histograms {
        for counts in [
            h.literal.as_mut_slice(),
            &mut h.red,
            &mut h.blue,
            &mut h.alpha,
            &mut h.distance,
        ] {
            codes.push(VP8LCreateHuffmanTree(counts, 15)?);
        }
    }
    Ok(codes)
}

/// Serialize tokens using the Huffman group at each token's starting pixel.
/// Distances must already have been converted to plane codes.
pub fn StoreImageToBitMask(
    bw: &mut VP8LBitWriter,
    width: usize,
    histo_bits: u32,
    refs: &super::backward_references_enc::VP8LBackwardRefs,
    histogram_symbols: &[u32],
    huffman_codes: &[HuffmanTreeCode],
) -> Result<(), TryReserveError> {
    use super::backward_references_enc::*;
    use crate::dsp::lossless_common::{VP8LPrefixEncode, VP8LSubSampleSize};
    use crate::webp::format_constants::NUM_LENGTH_CODES;
    assert!(width > 0);
    let histo_xsize = if histo_bits == 0 {
        1
    } else {
        VP8LSubSampleSize(width as u32, histo_bits) as usize
    };
    let (mut x, mut y) = (0, 0);
    for v in refs.iter() {
        let tile = if histo_bits == 0 {
            0
        } else {
            (y >> histo_bits) * histo_xsize + (x >> histo_bits)
        };
        let group = ((histogram_symbols[tile] >> 8) & 0xffff) as usize;
        let codes = &huffman_codes[5 * group..5 * group + 5];
        if PixOrCopyIsLiteral(v) {
            for (k, component) in [1, 2, 0, 3].into_iter().enumerate() {
                WriteHuffmanCode(bw, &codes[k], PixOrCopyLiteral(v, component) as usize)?;
            }
        } else if PixOrCopyIsCacheIdx(v) {
            WriteHuffmanCode(
                bw,
                &codes[0],
                256 + NUM_LENGTH_CODES + PixOrCopyCacheIdx(v) as usize,
            )?;
        } else {
            let (code, n_bits, bits) = VP8LPrefixEncode(PixOrCopyLength(v));
            WriteHuffmanCodeWithExtraBits(bw, &codes[0], 256 + code as usize, bits, n_bits)?;
            let (code, n_bits, bits) = VP8LPrefixEncode(PixOrCopyDistance(v));
            WriteHuffmanCode(bw, &codes[4], code as usize)?;
            VP8LPutBits(bw, bits, n_bits)?;
        }
        x += PixOrCopyLength(v) as usize;
        y += x / width;
        x %= width;
    }
    Ok(())
}

/// Auxiliary image stream (no transforms, cache or spatial Huffman image).
/// Like upstream, low_effort affects hash-chain construction only.
pub fn EncodeImageNoHuffman(
    bw: &mut VP8LBitWriter,
    argb: &[u32],
    hash_chain: &mut super::backward_references_enc::VP8LHashChain,
    refs: &mut [super::backward_references_enc::VP8LBackwardRefs],
    width: usize,
    height: usize,
    quality: u32,
    low_effort: bool,
) -> Result<(), TryReserveError> {
    use super::backward_references_enc::*;
    use super::histogram_enc::*;
    VP8LHashChainFill(hash_chain, quality, argb, width, height, low_effort)?;
    let cache_bits = VP8LGetBackwardReferences(
        width,
        height,
        argb,
        quality,
        false,
        kLZ77Standard | kLZ77RLE,
        0,
        false,
        hash_chain,
        refs,
    )?;
    assert_eq!(cache_bits, 0);
    let mut histogram = VP8LAllocateHistogram(0)?;
    VP8LHistogramStoreRefs(&refs[0], None, 0, &mut histogram);
    let mut codes = GetHuffBitLengthsAndCodes(std::slice::from_mut(&mut histogram))?;
    VP8LPutBits(bw, 0, 1)?;
    for code in &mut codes {
        StoreHuffmanCode(bw, code)?;
        ClearHuffmanTreeIfOnlyOneSymbol(code);
    }
    StoreImageToBitMask(bw, width, 0, &refs[0], &[0], &codes)
}
