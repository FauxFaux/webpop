// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8l_enc.c (image streams and Huffman storage).
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

/// Parameters for one main-image reference trial (upstream CrunchSubConfig).
#[derive(Clone, Copy, Debug)]
pub struct CrunchSubConfig {
    pub lz77: u32,
    pub do_no_cache: bool,
}

/// Encode prepared pixels, including cache selection, spatial Huffman groups
/// and reference trials. Transforms and image/container headers are supplied
/// by the caller. Returns (header bytes, data bytes) for the winning trial,
/// relative to init_byte_position, and updates cache_bits to that trial's value.
/// The internal API assumes validated geometry and configuration.
#[allow(clippy::too_many_arguments)]
pub fn EncodeImageInternal(
    bw: &mut VP8LBitWriter,
    argb: &[u32],
    hash_chain: &mut super::backward_references_enc::VP8LHashChain,
    refs: &mut [super::backward_references_enc::VP8LBackwardRefs],
    width: usize,
    height: usize,
    quality: u32,
    low_effort: bool,
    sub_configs: &[CrunchSubConfig],
    cache_bits: &mut u32,
    histogram_bits_in: u32,
    init_byte_position: usize,
) -> Result<(usize, usize), TryReserveError> {
    use super::backward_references_enc::*;
    use super::histogram_enc::*;
    use super::predictor_enc::VP8LOptimizeSampling;
    use crate::utils::bit_writer_utils::*;
    use crate::webp::format_constants::{MIN_HUFFMAN_BITS, NUM_HUFFMAN_BITS};
    const MAX_HUFFMAN_BITS: u32 = MIN_HUFFMAN_BITS + (1 << NUM_HUFFMAN_BITS) - 1;
    assert!((MIN_HUFFMAN_BITS..=MAX_HUFFMAN_BITS).contains(&histogram_bits_in));
    assert!(width > 0 && height > 0 && refs.len() >= 4);
    assert!(!sub_configs.is_empty() && sub_configs.len() <= 2);
    assert!(init_byte_position <= VP8LBitWriterNumBytes(bw));
    let histogram_size = width
        .div_ceil(1 << histogram_bits_in)
        .checked_mul(height.div_ceil(1 << histogram_bits_in))
        .expect("image geometry");
    let mut histogram_argb = Vec::new();
    histogram_argb.try_reserve_exact(histogram_size)?;
    histogram_argb.resize(histogram_size, 0);
    let mut histogram_chain = VP8LHashChainInit(histogram_size)?;
    let bw_init = VP8LBitWriterClone(bw)?;
    let mut bw_best = if sub_configs.len() > 1 || sub_configs[0].do_no_cache {
        VP8LBitWriterClone(bw)?
    } else {
        VP8LBitWriterInit(0)?
    };
    VP8LHashChainFill(hash_chain, quality, argb, width, height, low_effort)?;
    let cache_bits_init = if *cache_bits == 0 {
        MAX_COLOR_CACHE_BITS
    } else {
        *cache_bits
    };
    let mut best_size = usize::MAX;
    let mut sizes = (0, 0);
    for config in sub_configs {
        let best_cache = VP8LGetBackwardReferences(
            width,
            height,
            argb,
            quality,
            low_effort,
            config.lz77,
            cache_bits_init,
            config.do_no_cache,
            hash_chain,
            refs,
        )?;
        for i_cache in 0..if config.do_no_cache { 2 } else { 1 } {
            let trial_cache = if i_cache == 0 { best_cache } else { 0 };
            if i_cache == 1 && best_cache == 0 {
                break;
            }
            VP8LBitWriterReset(&bw_init, bw);
            let mut histogram_image = VP8LAllocateHistogramSet(histogram_size, trial_cache)?;
            let mut tmp = VP8LAllocateHistogram(trial_cache)?;
            VP8LGetHistoImageSymbols(
                width,
                height,
                &refs[i_cache],
                quality,
                low_effort,
                histogram_bits_in,
                trial_cache,
                &mut histogram_image,
                &mut tmp,
                &mut histogram_argb,
            )?;
            let groups = histogram_image.size;
            let mut codes = GetHuffBitLengthsAndCodes(&mut histogram_image.histograms[..groups])?;
            drop(histogram_image);
            drop(tmp);
            VP8LPutBits(bw, u32::from(trial_cache > 0), 1)?;
            if trial_cache > 0 {
                VP8LPutBits(bw, trial_cache, 4)?;
            }
            let mut image_size = 0;
            for symbol in &mut histogram_argb {
                image_size = image_size.max(*symbol as usize + 1);
                *symbol <<= 8;
            }
            let write_image = image_size > 1;
            VP8LPutBits(bw, u32::from(write_image), 1)?;
            let mut histogram_bits = histogram_bits_in;
            if write_image {
                histogram_bits = VP8LOptimizeSampling(
                    &mut histogram_argb,
                    width,
                    height,
                    histogram_bits_in,
                    MAX_HUFFMAN_BITS,
                );
                VP8LPutBits(bw, histogram_bits - MIN_HUFFMAN_BITS, NUM_HUFFMAN_BITS)?;
                EncodeImageNoHuffman(
                    bw,
                    &histogram_argb,
                    &mut histogram_chain,
                    &mut refs[2..],
                    width.div_ceil(1 << histogram_bits),
                    height.div_ceil(1 << histogram_bits),
                    quality,
                    low_effort,
                )?;
            }
            for code in &mut codes[..5 * image_size] {
                StoreHuffmanCode(bw, code)?;
                ClearHuffmanTreeIfOnlyOneSymbol(code);
            }
            let header_size = VP8LBitWriterNumBytes(bw) - init_byte_position;
            StoreImageToBitMask(
                bw,
                width,
                histogram_bits,
                &refs[i_cache],
                &histogram_argb,
                &codes,
            )?;
            let trial_size = VP8LBitWriterNumBytes(bw);
            // Upstream compares rounded byte counts, preserving the first tie.
            if trial_size < best_size {
                best_size = trial_size;
                *cache_bits = trial_cache;
                sizes = (header_size, trial_size - init_byte_position - header_size);
                VP8LBitWriterSwap(bw, &mut bw_best);
            }
        }
    }
    VP8LBitWriterSwap(bw, &mut bw_best);
    Ok(sizes)
}

/// Errors at the checked image-header/container boundary.
#[derive(Debug, thiserror::Error)]
pub enum ImageWriteError {
    #[error("image dimensions must be in 1..=16383")]
    InvalidDimensions,
    #[error("encoded image exceeds the RIFF size limit")]
    SizeOverflow,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
}

/// Upstream WriteImageSize, with validation before changing the writer.
pub fn WriteImageSize(
    bw: &mut VP8LBitWriter,
    width: u32,
    height: u32,
) -> Result<(), ImageWriteError> {
    if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
        return Err(ImageWriteError::InvalidDimensions);
    }
    VP8LPutBits(bw, width - 1, 14)?;
    VP8LPutBits(bw, height - 1, 14)?;
    Ok(())
}

/// Upstream WriteRealAlphaAndVersion (VP8L version zero).
pub fn WriteRealAlphaAndVersion(
    bw: &mut VP8LBitWriter,
    has_alpha: bool,
) -> Result<(), TryReserveError> {
    VP8LPutBits(bw, u32::from(has_alpha), 1)?;
    VP8LPutBits(bw, 0, 3)
}

/// Upstream WriteRiffHeader, returning the fixed header instead of invoking a
/// picture writer callback. Sizes include the signature and RIFF padding.
pub fn WriteRiffHeader(webpll_size: usize) -> Result<[u8; 21], ImageWriteError> {
    let vp8l_size = webpll_size
        .checked_add(1)
        .ok_or(ImageWriteError::SizeOverflow)?;
    let riff_size = vp8l_size
        .checked_add(12 + (vp8l_size & 1))
        .ok_or(ImageWriteError::SizeOverflow)?;
    let riff_size = u32::try_from(riff_size).map_err(|_| ImageWriteError::SizeOverflow)?;
    let vp8l_size = u32::try_from(vp8l_size).map_err(|_| ImageWriteError::SizeOverflow)?;
    let mut header = *b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f";
    header[4..8].copy_from_slice(&riff_size.to_le_bytes());
    header[16..20].copy_from_slice(&vp8l_size.to_le_bytes());
    Ok(header)
}

/// Upstream WriteImage, collecting the finalized stream in an owned RIFF file.
/// The returned length replaces C's coded_size out parameter.
pub fn WriteImage(bw: &mut VP8LBitWriter) -> Result<Vec<u8>, ImageWriteError> {
    let data = crate::utils::bit_writer_utils::VP8LBitWriterFinish(bw)?;
    let header = WriteRiffHeader(data.len())?;
    let padding = (data.len() + 1) & 1;
    let size = data
        .len()
        .checked_add(header.len() + padding)
        .ok_or(ImageWriteError::SizeOverflow)?;
    let mut output = Vec::new();
    output.try_reserve_exact(size)?;
    output.extend_from_slice(&header);
    output.extend_from_slice(data);
    output.resize(size, 0);
    Ok(output)
}

fn ApplyPaletteHash0(color: u32) -> usize {
    ((color >> 8) & 255) as usize
}
fn ApplyPaletteHash1(color: u32) -> usize {
    ((color & 0x00ffffff).wrapping_mul(4222244071) >> 21) as usize
}
fn ApplyPaletteHash2(color: u32) -> usize {
    ((color & 0x00ffffff).wrapping_mul((1 << 31) - 1) >> 21) as usize
}

fn PaletteHashTable(palette: &[u32], buffer: &mut [u16; 2048]) -> usize {
    for (i, hash) in [ApplyPaletteHash0, ApplyPaletteHash1, ApplyPaletteHash2]
        .into_iter()
        .enumerate()
    {
        buffer.fill(u16::MAX);
        let mut success = true;
        for (j, &color) in palette.iter().enumerate() {
            let index = hash(color);
            if buffer[index] != u16::MAX {
                success = false;
                break;
            }
            buffer[index] = j as u16;
        }
        if success {
            return i;
        }
    }
    3
}

/// Remap validated ARGB rows to packed green-channel palette indices. Each
/// source pixel must occur in the distinct palette, and xbits must permit its
/// indices. Source/destination strides are in words; padding is untouched.
#[allow(clippy::too_many_arguments)]
pub fn ApplyPalette(
    src: &[u32],
    src_stride: usize,
    dst: &mut [u32],
    dst_stride: usize,
    palette: &[u32],
    width: usize,
    height: usize,
    xbits: u32,
) -> Result<(), TryReserveError> {
    use crate::utils::palette::{PrepareMapToPalette, SearchColorNoIdx};
    assert!((1..=256).contains(&palette.len()) && xbits <= 3);
    assert!(palette.len() <= 1usize << (8 >> xbits));
    assert!(width > 0 && height > 0 && src_stride >= width);
    let packed_width = width.div_ceil(1 << xbits);
    assert!(dst_stride >= packed_width);
    let source_len = (height - 1)
        .checked_mul(src_stride)
        .and_then(|v| v.checked_add(width));
    let dest_len = (height - 1)
        .checked_mul(dst_stride)
        .and_then(|v| v.checked_add(packed_width));
    assert!(source_len.is_some_and(|v| v <= src.len()));
    assert!(dest_len.is_some_and(|v| v <= dst.len()));
    let mut row = Vec::new();
    row.try_reserve_exact(width)?;
    row.resize(width, 0u8);
    let mut table = [u16::MAX; 2048];
    let mode = if palette.len() < 4 {
        4
    } else {
        PaletteHashTable(palette, &mut table)
    };
    let mut sorted = [0; 256];
    let mut map = [0; 256];
    if mode == 3 {
        PrepareMapToPalette(
            palette,
            &mut sorted[..palette.len()],
            &mut map[..palette.len()],
        );
    }
    let mut previous = palette[0];
    let mut index = 0;
    for y in 0..height {
        for x in 0..width {
            let pixel = src[y * src_stride + x];
            if pixel != previous {
                index = match mode {
                    0 => table[ApplyPaletteHash0(pixel)] as usize,
                    1 => table[ApplyPaletteHash1(pixel)] as usize,
                    2 => table[ApplyPaletteHash2(pixel)] as usize,
                    3 => {
                        map[SearchColorNoIdx(&sorted[..palette.len()], pixel)
                            .expect("pixel absent from palette")] as usize
                    }
                    _ => palette
                        .iter()
                        .position(|&c| c == pixel)
                        .expect("pixel absent from palette"),
                };
                previous = pixel;
            }
            row[x] = index as u8;
        }
        crate::dsp::lossless_enc::VP8LBundleColorMap(
            &row,
            xbits,
            &mut dst[y * dst_stride..y * dst_stride + packed_width],
        );
    }
    Ok(())
}

/// Upstream MapImageFromPalette without the not-yet-integrated encoder state.
/// Returns the current (packed) width and owned transformed pixels.
pub fn MapImageFromPalette(
    src: &[u32],
    stride: usize,
    palette: &[u32],
    width: usize,
    height: usize,
) -> Result<(usize, Vec<u32>), TryReserveError> {
    assert!((1..=256).contains(&palette.len()));
    let xbits = match palette.len() {
        1..=2 => 3,
        3..=4 => 2,
        5..=16 => 1,
        _ => 0,
    };
    let packed_width = width.div_ceil(1 << xbits);
    let size = packed_width
        .checked_mul(height)
        .expect("validated image size");
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(size)?;
    pixels.resize(size, 0);
    ApplyPalette(
        src,
        stride,
        &mut pixels,
        packed_width,
        palette,
        width,
        height,
        xbits,
    )?;
    Ok((packed_width, pixels))
}

/// Write the color-indexing transform and delta-coded palette auxiliary image.
/// As in upstream, a trailing zero color may be omitted only above 17 colors.
pub fn EncodePalette(
    bw: &mut VP8LBitWriter,
    palette: &[u32],
    hash_chain: &mut super::backward_references_enc::VP8LHashChain,
    refs: &mut [super::backward_references_enc::VP8LBackwardRefs],
    low_effort: bool,
) -> Result<(), TryReserveError> {
    assert!((1..=256).contains(&palette.len()));
    let size = palette.len() - usize::from(palette.len() > 17 && palette.last() == Some(&0));
    VP8LPutBits(bw, 1, 1)?;
    VP8LPutBits(bw, 3, 2)?; // COLOR_INDEXING_TRANSFORM
    VP8LPutBits(bw, (size - 1) as u32, 8)?;
    let mut deltas = [0; 256];
    deltas[0] = palette[0];
    for i in 1..size {
        deltas[i] = crate::dsp::lossless_common::VP8LSubPixels(palette[i], palette[i - 1]);
    }
    EncodeImageNoHuffman(
        bw,
        &deltas[..size],
        hash_chain,
        refs,
        size,
        1,
        20,
        low_effort,
    )
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn palette_lookup_exercises_all_hashes_and_search_fallback() {
        for (expected, palette) in [
            (0, vec![0xff000000, 0xff000100, 0xff000200, 0xff000300]),
            (1, vec![0xff000001, 0xff000002, 0xff000003, 0xff000004]),
            (2, vec![0xff000001, 0xff05009e, 0xff400001, 0xff800001]),
            (3, vec![0x00123456, 0x01123456, 0x80123456, 0xff123456]),
        ] {
            assert_eq!(PaletteHashTable(&palette, &mut [0; 2048]), expected);
            let mut output = [0xdeadbeef; 8];
            let pixels = [
                palette[3], palette[0], palette[2], palette[1], 0xbaadf00d, palette[1], palette[2],
                palette[0], palette[3],
            ];
            ApplyPalette(&pixels, 5, &mut output, 4, &palette, 4, 2, 0).unwrap();
            assert_eq!(
                output,
                [
                    0xff000300, 0xff000000, 0xff000200, 0xff000100, 0xff000100, 0xff000200,
                    0xff000000, 0xff000300
                ]
            );
            // Repeat with actual destination padding and no final-row padding.
            let mut output = [0xdeadbeef; 10];
            ApplyPalette(&pixels, 5, &mut output, 6, &palette, 4, 2, 0).unwrap();
            assert_eq!(&output[4..6], &[0xdeadbeef; 2]);
            assert_eq!(
                &output[6..],
                &[0xff000100, 0xff000200, 0xff000000, 0xff000300]
            );
        }
    }
}
