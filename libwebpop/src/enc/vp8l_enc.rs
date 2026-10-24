// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8l_enc.c (analysis, streams and transforms).
// See COPYING and PATENTS.

use std::collections::TryReserveError;

use crate::utils::bit_writer_utils::{VP8LBitWriter, VP8LPutBits};
use crate::utils::huffman_encode_utils::{
    HuffmanTreeCode, HuffmanTreeToken, VP8LCreateCompressedHuffmanTree, VP8LCreateHuffmanTree,
};
use crate::utils::palette::PaletteSorting;
use crate::utils::utils::BitsLog2Floor;
use crate::webp::format_constants::CODE_LENGTH_CODES;

/// Transform candidates, in the upstream analysis/tie-breaking order.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntropyIx {
    kDirect,
    kSpatial,
    kSubGreen,
    kSpatialSubGreen,
    kPalette,
    kPaletteAndSpatial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntropyAnalysis {
    pub min_entropy_ix: EntropyIx,
    pub red_and_blue_always_zero: bool,
    pub red_and_blue_transformed_always_zero: bool,
}

fn AddSingle(p: u32, histograms: &mut [[u32; 256]], channels: [usize; 4]) {
    for (channel, shift) in channels.into_iter().zip([24, 16, 8, 0]) {
        histograms[channel][((p >> shift) & 255) as usize] += 1;
    }
}

fn AddSingleSubGreen(p: u32, histograms: &mut [[u32; 256]], red: usize, blue: usize) {
    let green = p >> 8;
    histograms[red][((p >> 16).wrapping_sub(green) & 255) as usize] += 1;
    histograms[blue][(p.wrapping_sub(green) & 255) as usize] += 1;
}

#[inline]
fn HashPix(pix: u32) -> usize {
    (pix.wrapping_add(pix >> 19).wrapping_mul(0x39c5fba7) >> 24) as usize
}

/// Estimate transform costs using the pinned encoder's fixed-point entropy
/// model. Geometry, sampling bits and the optional sorted palette must already
/// be validated. Padding is ignored; final-row padding is unnecessary.
pub fn AnalyzeEntropy(
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    palette_sorted: Option<&[u32]>,
    transform_bits: u32,
) -> Result<EntropyAnalysis, TryReserveError> {
    use super::histogram_enc::VP8LBitsEntropy;
    use crate::dsp::lossless_common::{LOG_2_PRECISION_BITS, VP8LFastLog2, VP8LSubPixels};
    use EntropyIx::*;

    assert!(width > 0 && height > 0 && stride >= width);
    assert!(
        (height - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(width))
            .is_some_and(|n| n <= argb.len())
    );
    if let Some(palette) = palette_sorted {
        assert!(!palette.is_empty() && palette.len() <= 256);
        if palette.len() <= 16 {
            return Ok(EntropyAnalysis {
                min_entropy_ix: kPalette,
                red_and_blue_always_zero: palette.iter().all(|p| p & 0x00ff00ff == 0),
                red_and_blue_transformed_always_zero: true,
            });
        }
    }

    // HistoIx in upstream: alpha/pred, green/pred, red/pred, blue/pred,
    // subgreen red/pred, subgreen blue/pred, palette hash.
    const A: usize = 0;
    const AP: usize = 1;
    const G: usize = 2;
    const GP: usize = 3;
    const R: usize = 4;
    const RP: usize = 5;
    const B: usize = 6;
    const BP: usize = 7;
    const RS: usize = 8;
    const RPS: usize = 9;
    const BS: usize = 10;
    const BPS: usize = 11;
    const P: usize = 12;
    let mut histo = Vec::new();
    histo.try_reserve_exact(13)?;
    histo.resize(13, [0u32; 256]);
    let mut pix_prev = argb[0];
    for y in 0..height {
        for x in 0..width {
            let pix = argb[y * stride + x];
            let pix_diff = VP8LSubPixels(pix, pix_prev);
            // Retain the previous pixel across row boundaries, including when
            // this pixel is skipped because it matches the row above.
            pix_prev = pix;
            if pix_diff == 0 || (y > 0 && pix == argb[(y - 1) * stride + x]) {
                continue;
            }
            AddSingle(pix, &mut histo, [A, R, G, B]);
            AddSingle(pix_diff, &mut histo, [AP, RP, GP, BP]);
            AddSingleSubGreen(pix, &mut histo, RS, BS);
            AddSingleSubGreen(pix_diff, &mut histo, RPS, BPS);
            histo[P][HashPix(pix)] += 1;
        }
    }
    for channel in [RPS, BPS, RP, GP, BP, AP] {
        histo[channel][0] += 1;
    }
    let comp: [u64; 13] = std::array::from_fn(|i| VP8LBitsEntropy(&histo[i]));
    let tiles =
        width.div_ceil(1 << transform_bits) as u64 * height.div_ceil(1 << transform_bits) as u64;
    let entropy = [
        comp[A] + comp[R] + comp[G] + comp[B],
        comp[AP] + comp[RP] + comp[GP] + comp[BP] + tiles * u64::from(VP8LFastLog2(14)),
        comp[A] + comp[RS] + comp[G] + comp[BS],
        comp[AP] + comp[RPS] + comp[GP] + comp[BPS] + tiles * u64::from(VP8LFastLog2(24)),
        comp[P] + ((palette_sorted.map_or(0, <[u32]>::len) as u64 * 8) << LOG_2_PRECISION_BITS),
    ];
    let modes = [kDirect, kSpatial, kSubGreen, kSpatialSubGreen, kPalette];
    let mut best = 0;
    let last = if palette_sorted.is_some() { 4 } else { 3 };
    for i in 1..=last {
        if entropy[i] < entropy[best] {
            best = i;
        }
    }
    let (red, blue) = [(R, B), (RP, BP), (RS, BS), (RPS, BPS), (R, B)][best];
    Ok(EntropyAnalysis {
        min_entropy_ix: modes[best],
        red_and_blue_always_zero: argb[0] & 0x00ff00ff == 0
            && (1..256).all(|i| histo[R][i] | histo[B][i] == 0),
        red_and_blue_transformed_always_zero: (1..256).all(|i| histo[red][i] | histo[blue][i] == 0),
    })
}

pub fn GetHistoBits(method: u32, use_palette: bool, width: usize, height: usize) -> u32 {
    use crate::webp::format_constants::{MIN_HUFFMAN_BITS, NUM_HUFFMAN_BITS};
    assert!(method <= 6);
    ClampBits(
        width,
        height,
        (if use_palette { 9 } else { 7 }) - method as i32,
        MIN_HUFFMAN_BITS,
        MIN_HUFFMAN_BITS + (1 << NUM_HUFFMAN_BITS) - 1,
        2600,
    )
}

pub fn GetTransformBits(method: u32, histo_bits: u32) -> u32 {
    assert!(method <= 6);
    histo_bits.min(if method < 4 {
        6
    } else if method > 4 {
        4
    } else {
        5
    })
}

/// One transform/palette trial, followed by one or two reference trials.
/// `None` corresponds to upstream's kUnusedPalette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrunchConfig {
    pub entropy_idx: EntropyIx,
    pub palette_sorting_type: Option<PaletteSorting>,
    pub sub_configs: [CrunchSubConfig; 2],
    pub sub_configs_size: usize,
}

/// Owned analysis state. Palette entries beyond palette_size are unspecified.
#[derive(Clone, Debug)]
pub struct EncoderAnalysis {
    pub palette_sorted: [u32; 256],
    pub palette_size: usize,
    pub histo_bits: u32,
    pub predictor_transform_bits: u32,
    pub cross_color_transform_bits: u32,
    pub red_and_blue_transformed_always_zero: bool,
    pub crunch_configs: Vec<CrunchConfig>,
}

fn AddPaletteSortingConfigs(
    entropy_idx: EntropyIx,
    include_modified_zeng: bool,
    configs: &mut Vec<CrunchConfig>,
    template: &CrunchConfig,
) {
    use PaletteSorting::*;
    for sorting in [
        kMinimizeDelta,
        kModifiedZeng,
        kMinLAFromZeng,
        kMinLAFromDelta,
    ] {
        if sorting == kModifiedZeng && !include_modified_zeng {
            continue;
        }
        if matches!(sorting, kMinLAFromZeng | kMinLAFromDelta)
            && entropy_idx != EntropyIx::kPaletteAndSpatial
        {
            continue;
        }
        configs.push(CrunchConfig {
            entropy_idx,
            palette_sorting_type: Some(sorting),
            ..template.clone()
        });
    }
}

/// Analyze prepared positive-stride ARGB and retain the original configuration
/// order, including palette refinements and first-minimum entropy ties. This
/// internal entry point assumes validated image geometry and settings.
pub fn EncoderAnalyze(
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    method: u32,
    quality: u32,
) -> Result<EncoderAnalysis, TryReserveError> {
    use super::backward_references_enc::{kLZ77Box, kLZ77RLE, kLZ77Standard};
    use crate::utils::palette::GetColorPalette;
    use EntropyIx::*;
    use PaletteSorting::*;

    assert!(method <= 6 && quality <= 100);
    let mut palette_sorted = [0; 256];
    let count = GetColorPalette(argb, width, height, stride, &mut palette_sorted);
    let use_palette = count <= 256;
    let palette_size = if use_palette { count } else { 0 };
    let histo_bits = GetHistoBits(method, use_palette, width, height);
    let transform_bits = GetTransformBits(method, histo_bits);
    let low_effort = method == 0;
    let n_lz77s = if !low_effort && (1..=16).contains(&palette_size) {
        2
    } else {
        1
    };
    let do_no_cache = (method == 6 && quality == 100) || (method == 5 && quality >= 50);
    let template = CrunchConfig {
        entropy_idx: kDirect,
        palette_sorting_type: None,
        sub_configs: [
            CrunchSubConfig {
                lz77: kLZ77Standard | kLZ77RLE,
                do_no_cache,
            },
            CrunchSubConfig {
                lz77: kLZ77Box,
                do_no_cache,
            },
        ],
        sub_configs_size: n_lz77s,
    };
    let mut configs = Vec::new();
    // CRUNCH_CONFIGS_MAX = six entropy modes + twice six palette methods.
    // Reserve once so appending trials cannot introduce unchecked allocations.
    configs.try_reserve_exact(18)?;
    let mut transformed_zero = false; // VP8LEncodeStream's initial value.
    if low_effort {
        configs.push(CrunchConfig {
            entropy_idx: if use_palette {
                kPalette
            } else {
                kSpatialSubGreen
            },
            palette_sorting_type: use_palette.then_some(kSortedDefault),
            ..template
        });
    } else {
        let analysis = AnalyzeEntropy(
            argb,
            width,
            height,
            stride,
            use_palette.then_some(&palette_sorted[..palette_size]),
            transform_bits,
        )?;
        transformed_zero = analysis.red_and_blue_transformed_always_zero;
        if method == 6 && quality == 100 {
            for entropy_idx in [
                kDirect,
                kSpatial,
                kSubGreen,
                kSpatialSubGreen,
                kPalette,
                kPaletteAndSpatial,
            ] {
                if matches!(entropy_idx, kSubGreen | kSpatialSubGreen)
                    && analysis.red_and_blue_always_zero
                {
                    continue;
                }
                if matches!(entropy_idx, kPalette | kPaletteAndSpatial) {
                    if use_palette {
                        AddPaletteSortingConfigs(entropy_idx, true, &mut configs, &template);
                    }
                } else {
                    configs.push(CrunchConfig {
                        entropy_idx,
                        ..template.clone()
                    });
                }
            }
        } else {
            configs.push(CrunchConfig {
                entropy_idx: analysis.min_entropy_ix,
                palette_sorting_type: use_palette.then_some(kMinimizeDelta),
                ..template.clone()
            });
            if method >= 4
                && quality >= 50
                && use_palette
                && (analysis.min_entropy_ix == kPalette || palette_size >= 225)
            {
                AddPaletteSortingConfigs(kPaletteAndSpatial, false, &mut configs, &template);
            }
        }
    }
    Ok(EncoderAnalysis {
        palette_sorted,
        palette_size,
        histo_bits,
        predictor_transform_bits: transform_bits,
        cross_color_transform_bits: transform_bits,
        red_and_blue_transformed_always_zero: transformed_zero,
        crunch_configs: configs,
    })
}

/// The lossless subset of upstream WebPAuxStats for the selected stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LosslessStreamStats {
    pub lossless_features: u32,
    pub histogram_bits: u32,
    pub transform_bits: u32,
    pub cross_color_transform_bits: u32,
    pub cache_bits: u32,
    pub palette_size: usize,
    pub lossless_size: usize,
    pub lossless_hdr_size: usize,
    pub lossless_data_size: usize,
}

/// Synchronous encoder storage. The prepared source remains immutable across
/// trials; all transform and matching storage belongs to this encoder.
struct VP8LEncoder<'a> {
    source: &'a [u32],
    width: usize,
    height: usize,
    stride: usize,
    method: u32,
    quality: u32,
    exact: bool,
    near_lossless: u32,
    #[cfg(test)]
    evaluated_candidates: Vec<(usize, usize, u32)>,
    analysis: EncoderAnalysis,
    argb: Vec<u32>,
    current_width: usize,
    hash_chain: super::backward_references_enc::VP8LHashChain,
    refs: [super::backward_references_enc::VP8LBackwardRefs; 4],
}

fn MakeInputImageCopy(enc: &mut VP8LEncoder<'_>) -> Result<(), TryReserveError> {
    enc.argb.clear();
    enc.argb.try_reserve_exact(
        enc.width
            .checked_mul(enc.height)
            .expect("validated image size"),
    )?;
    for y in 0..enc.height {
        enc.argb
            .extend_from_slice(&enc.source[y * enc.stride..y * enc.stride + enc.width]);
    }
    enc.current_width = enc.width;
    Ok(())
}

fn EncodeStreamHook(
    enc: &mut VP8LEncoder<'_>,
    bw: &mut VP8LBitWriter,
) -> Result<LosslessStreamStats, TryReserveError> {
    use super::backward_references_enc::VP8LBackwardRefsClear;
    use crate::utils::bit_writer_utils::*;
    use crate::utils::palette::PaletteSort;
    use EntropyIx::*;

    let low_effort = enc.method == 0;
    let byte_position = VP8LBitWriterNumBytes(bw);
    let bw_init = VP8LBitWriterClone(bw)?;
    let mut bw_best = if enc.analysis.crunch_configs.len() > 1 {
        VP8LBitWriterClone(bw)?
    } else {
        VP8LBitWriterInit(0)?
    };
    let mut best_size = usize::MAX;
    let mut best_stats = LosslessStreamStats::default();
    let mut tested_palettes = [[0u32; 256]; 6];
    let mut num_tested_palettes = 0;

    for idx in 0..enc.analysis.crunch_configs.len() {
        let config = enc.analysis.crunch_configs[idx].clone();
        let entropy_idx = config.entropy_idx;
        let use_palette = matches!(entropy_idx, kPalette | kPaletteAndSpatial);
        let use_subtract_green = matches!(entropy_idx, kSubGreen | kSpatialSubGreen);
        let use_predict = matches!(
            entropy_idx,
            kSpatial | kSpatialSubGreen | kPaletteAndSpatial
        );
        let use_cross_color = !low_effort
            && !use_palette
            && use_predict
            && !enc.analysis.red_and_blue_transformed_always_zero;
        let mut cache_bits = 0;
        let mut predictor_bits = 0;
        let mut cross_color_bits = 0;
        VP8LBackwardRefsClear(&mut enc.refs[0]);
        VP8LBackwardRefsClear(&mut enc.refs[1]);

        if use_palette {
            let size = enc.analysis.palette_size;
            let mut palette = [0; 256];
            PaletteSort(
                config.palette_sorting_type.expect("palette sorting method"),
                enc.source,
                enc.width,
                enc.height,
                enc.stride,
                &enc.analysis.palette_sorted[..size],
                &mut palette[..size],
            )?;
            if idx == 0 || entropy_idx != enc.analysis.crunch_configs[idx - 1].entropy_idx {
                num_tested_palettes = 0;
            }
            if tested_palettes[..num_tested_palettes]
                .iter()
                .any(|previous| previous[..size] == palette[..size])
            {
                continue;
            }
            tested_palettes[num_tested_palettes][..size].copy_from_slice(&palette[..size]);
            num_tested_palettes += 1;
            EncodePalette(
                bw,
                &palette[..size],
                &mut enc.hash_chain,
                &mut enc.refs[..2],
                low_effort,
            )?;
            (enc.current_width, enc.argb) = MapImageFromPalette(
                enc.source,
                enc.stride,
                &palette[..size],
                enc.width,
                enc.height,
            )?;
            cache_bits = BitsLog2Floor(size as u32) + 1;
        } else if enc.near_lossless < 100 && !use_predict {
            use super::near_lossless_enc::{NearLosslessError, VP8ApplyNearLossless};
            // Each candidate starts with the inherited source. In particular,
            // subtract-green must follow standalone preprocessing.
            enc.argb = VP8ApplyNearLossless(
                enc.source,
                enc.width as u32,
                enc.height as u32,
                enc.stride,
                enc.near_lossless,
            )
            .map_err(|error| match error {
                NearLosslessError::Allocation(error) => error,
                _ => unreachable!("validated encoder geometry and strength: {error}"),
            })?;
            enc.current_width = enc.width;
        } else {
            MakeInputImageCopy(enc)?;
        }
        if use_subtract_green {
            ApplySubtractGreen(bw, &mut enc.argb)?;
        }
        if use_predict {
            predictor_bits = ApplyPredictFilter(
                bw,
                &mut enc.argb,
                &mut enc.hash_chain,
                &mut enc.refs[..2],
                enc.current_width,
                enc.height,
                enc.analysis.predictor_transform_bits as i32,
                enc.method,
                enc.quality,
                low_effort,
                if use_palette { 100 } else { enc.near_lossless },
                enc.exact,
                use_subtract_green,
            )?;
        }
        if use_cross_color {
            cross_color_bits = ApplyCrossColorFilter(
                bw,
                &mut enc.argb,
                &mut enc.hash_chain,
                &mut enc.refs[..2],
                enc.current_width,
                enc.height,
                enc.analysis.cross_color_transform_bits,
                enc.quality,
                low_effort,
            )?;
        }
        VP8LPutBits(bw, 0, 1)?; // No more transforms.
        let (header, data) = EncodeImageInternal(
            bw,
            &enc.argb,
            &mut enc.hash_chain,
            &mut enc.refs,
            enc.current_width,
            enc.height,
            enc.quality,
            low_effort,
            &config.sub_configs[..config.sub_configs_size],
            &mut cache_bits,
            enc.analysis.histo_bits,
            byte_position,
        )?;
        let size = VP8LBitWriterNumBytes(bw);
        #[cfg(test)]
        enc.evaluated_candidates.push((
            idx,
            size,
            u32::from(use_predict)
                | (u32::from(use_cross_color) << 1)
                | (u32::from(use_subtract_green) << 2)
                | (u32::from(use_palette) << 3),
        ));
        if size < best_size {
            best_size = size;
            VP8LBitWriterSwap(bw, &mut bw_best);
            best_stats = LosslessStreamStats {
                lossless_features: u32::from(use_predict)
                    | (u32::from(use_cross_color) << 1)
                    | (u32::from(use_subtract_green) << 2)
                    | (u32::from(use_palette) << 3),
                histogram_bits: enc.analysis.histo_bits,
                transform_bits: predictor_bits,
                cross_color_transform_bits: cross_color_bits,
                cache_bits,
                palette_size: enc.analysis.palette_size,
                lossless_size: size - byte_position,
                lossless_hdr_size: header,
                lossless_data_size: data,
            };
        }
        if enc.analysis.crunch_configs.len() > 1 {
            VP8LBitWriterReset(&bw_init, bw);
        }
    }
    VP8LBitWriterSwap(bw, &mut bw_best);
    Ok(best_stats)
}

/// Analyze and encode prepared ARGB using the upstream synchronous trial loop.
/// The caller supplies image/container headers and prepared input (including
/// transparent-RGB cleanup when desired). Geometry/settings must be validated.
/// This entry point selects strength 100 and synchronous execution.
#[allow(clippy::too_many_arguments)]
pub fn VP8LEncodeStream(
    bw: &mut VP8LBitWriter,
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    method: u32,
    quality: u32,
    exact: bool,
) -> Result<LosslessStreamStats, TryReserveError> {
    VP8LEncodeStreamWithNearLossless(bw, argb, width, height, stride, method, quality, 100, exact)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn VP8LEncodeStreamWithNearLossless(
    bw: &mut VP8LBitWriter,
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    method: u32,
    quality: u32,
    near_lossless: u32,
    exact: bool,
) -> Result<LosslessStreamStats, TryReserveError> {
    assert!(near_lossless <= 100);
    use super::backward_references_enc::{VP8LBackwardRefsInit, VP8LHashChainInit};
    let analysis = EncoderAnalyze(argb, width, height, stride, method, quality)?;
    let size = width.checked_mul(height).expect("validated image size");
    let mut enc = VP8LEncoder {
        source: argb,
        width,
        height,
        stride,
        method,
        quality,
        exact,
        near_lossless,
        #[cfg(test)]
        evaluated_candidates: Vec::new(),
        analysis,
        argb: Vec::new(),
        current_width: width,
        hash_chain: VP8LHashChainInit(size)?,
        refs: std::array::from_fn(|_| VP8LBackwardRefsInit((size - 1) / 16 + 1)),
    };
    EncodeStreamHook(&mut enc, bw)
}

/// Encode the owned ARGB picture and lend its finalized VP8L bits to the
/// selected still/container writer before releasing the bit writer.
pub(crate) fn VP8LEncodeImage<T>(
    config: &super::config_enc::EncoderConfig,
    picture: &super::picture_enc::WebPPicture,
    write: impl FnOnce(
        crate::mux::muxinternal::EncodedImage<'_>,
    ) -> Result<T, crate::webp::encode::LosslessEncodeError>,
) -> Result<T, crate::webp::encode::LosslessEncodeError> {
    use super::picture_csp_enc::{RgbaInputError, WebPPictureHasTransparency};
    use crate::utils::bit_writer_utils::VP8LBitWriterInit;
    let initial_size = picture
        .argb
        .len()
        .checked_mul(2)
        .ok_or(RgbaInputError::SizeOverflow)?;
    let mut bw = VP8LBitWriterInit(initial_size)?;
    WriteImageSize(&mut bw, picture.width, picture.height)?;
    WriteRealAlphaAndVersion(&mut bw, WebPPictureHasTransparency(picture))?;
    VP8LEncodeStreamWithNearLossless(
        &mut bw,
        &picture.argb,
        picture.width as usize,
        picture.height as usize,
        picture.width as usize,
        config.options.method,
        config.options.quality,
        config.near_lossless,
        config.options.exact,
    )?;
    let data = crate::utils::bit_writer_utils::VP8LBitWriterFinish(&mut bw)?;
    write(crate::mux::muxinternal::EncodedImage::Vp8l { data })
}

/// Clamp a histogram/transform sampling size, respecting the auxiliary-image
/// size limit and choosing the smallest tile size when only one tile remains.
pub fn ClampBits(
    width: usize,
    height: usize,
    bits: i32,
    min_bits: u32,
    max_bits: u32,
    image_size_max: usize,
) -> u32 {
    assert!(width > 0 && height > 0 && min_bits <= max_bits && max_bits < usize::BITS);
    let image_size = |bits: u32| {
        width
            .div_ceil(1 << bits)
            .checked_mul(height.div_ceil(1 << bits))
            .expect("auxiliary image geometry")
    };
    let mut bits = bits.clamp(min_bits as i32, max_bits as i32) as u32;
    while bits < max_bits && image_size(bits) > image_size_max {
        bits += 1;
    }
    while bits > min_bits && image_size(bits - 1) == 1 {
        bits -= 1;
    }
    bits
}

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

#[inline]
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

#[inline]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Upstream WriteImage, writing the finalized RIFF file into `output`.
/// Reuses capacity and replaces previous contents on success; errors leave
/// contents unchanged. `output.len()` replaces C's coded_size out parameter.
pub fn WriteImage(bw: &mut VP8LBitWriter, output: &mut Vec<u8>) -> Result<(), ImageWriteError> {
    let data = crate::utils::bit_writer_utils::VP8LBitWriterFinish(bw)?;
    WriteImageData(data, output)
}

/// Assemble an already finalized VP8L bitstream (without the 0x2f signature).
pub(crate) fn WriteImageData(data: &[u8], output: &mut Vec<u8>) -> Result<(), ImageWriteError> {
    let header = WriteRiffHeader(data.len())?;
    let padding = (data.len() + 1) & 1;
    let size = data
        .len()
        .checked_add(header.len() + padding)
        .ok_or(ImageWriteError::SizeOverflow)?;
    // Reserve before clearing so allocation failure preserves the previous output.
    output.try_reserve_exact(size.saturating_sub(output.len()))?;
    output.clear();
    output.extend_from_slice(&header);
    output.extend_from_slice(data);
    output.resize(size, 0);
    Ok(())
}

#[inline]
fn ApplyPaletteHash0(color: u32) -> usize {
    ((color >> 8) & 255) as usize
}
#[inline]
fn ApplyPaletteHash1(color: u32) -> usize {
    ((color & 0x00ffffff).wrapping_mul(4222244071) >> 21) as usize
}
#[inline]
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

/// MapImageFromPalette with explicit source and palette storage.
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

/// Apply the predictor transform and write its header and auxiliary
/// map. Encoder-state fields and reusable hash/reference scratch are explicit.
/// `method` selects the search range as in upstream. Palette indices must
/// select near-lossless strength 100 to retain their exact values.
/// Returns the selected transform bits. The hash chain needs capacity for the
/// auxiliary map, and `refs` needs at least two reusable reference lists.
#[allow(clippy::too_many_arguments)]
pub fn ApplyPredictFilter(
    bw: &mut VP8LBitWriter,
    argb: &mut [u32],
    hash_chain: &mut super::backward_references_enc::VP8LHashChain,
    refs: &mut [super::backward_references_enc::VP8LBackwardRefs],
    width: usize,
    height: usize,
    predictor_transform_bits: i32,
    method: u32,
    quality: u32,
    low_effort: bool,
    near_lossless: u32,
    exact: bool,
    used_subtract_green: bool,
) -> Result<u32, TryReserveError> {
    use super::predictor_enc::{MAX_TRANSFORM_BITS, VP8LResidualImage};
    use crate::webp::format_constants::{MIN_TRANSFORM_BITS, NUM_TRANSFORM_BITS};
    const MAX_PREDICTOR_IMAGE_SIZE: usize = 1 << 14;
    assert!(method <= 6 && quality <= 100);
    let max_bits = ClampBits(
        width,
        height,
        predictor_transform_bits,
        MIN_TRANSFORM_BITS,
        MAX_TRANSFORM_BITS,
        MAX_PREDICTOR_IMAGE_SIZE,
    );
    let min_bits = ClampBits(
        width,
        height,
        max_bits as i32 - 2 * method.saturating_sub(4) as i32,
        MIN_TRANSFORM_BITS,
        MAX_TRANSFORM_BITS,
        MAX_PREDICTOR_IMAGE_SIZE,
    );
    let (bits, modes) = VP8LResidualImage(
        width,
        height,
        min_bits,
        max_bits,
        low_effort,
        argb,
        near_lossless,
        exact,
        used_subtract_green,
    )?;
    VP8LPutBits(bw, 1, 1)?; // TRANSFORM_PRESENT
    VP8LPutBits(bw, 0, 2)?; // PREDICTOR_TRANSFORM
    VP8LPutBits(bw, bits - MIN_TRANSFORM_BITS, NUM_TRANSFORM_BITS)?;
    EncodeImageNoHuffman(
        bw,
        &modes,
        hash_chain,
        refs,
        width.div_ceil(1 << bits),
        height.div_ceil(1 << bits),
        quality,
        low_effort,
    )?;
    Ok(bits)
}

/// Write and apply the subtract-green transform to a prepared image.
pub fn ApplySubtractGreen(bw: &mut VP8LBitWriter, argb: &mut [u32]) -> Result<(), TryReserveError> {
    VP8LPutBits(bw, 1, 1)?; // TRANSFORM_PRESENT
    VP8LPutBits(bw, 2, 2)?; // SUBTRACT_GREEN_TRANSFORM
    crate::dsp::lossless_enc::VP8LSubtractGreenFromBlueAndRed(argb);
    Ok(())
}

/// Select and apply cross-color multipliers, then write the transform header
/// and auxiliary map. Inputs may already contain predictor residuals.
/// Returns optimized tile bits; hash-chain/reference scratch is reusable as
/// for ApplyPredictFilter. Quality controls both search and auxiliary coding.
#[allow(clippy::too_many_arguments)]
pub fn ApplyCrossColorFilter(
    bw: &mut VP8LBitWriter,
    argb: &mut [u32],
    hash_chain: &mut super::backward_references_enc::VP8LHashChain,
    refs: &mut [super::backward_references_enc::VP8LBackwardRefs],
    width: usize,
    height: usize,
    min_bits: u32,
    quality: u32,
    low_effort: bool,
) -> Result<u32, TryReserveError> {
    use crate::webp::format_constants::{MIN_TRANSFORM_BITS, NUM_TRANSFORM_BITS};
    let (bits, map) =
        super::predictor_enc::VP8LColorSpaceTransform(width, height, min_bits, quality, argb)?;
    VP8LPutBits(bw, 1, 1)?; // TRANSFORM_PRESENT
    VP8LPutBits(bw, 1, 2)?; // CROSS_COLOR_TRANSFORM
    VP8LPutBits(bw, bits - MIN_TRANSFORM_BITS, NUM_TRANSFORM_BITS)?;
    EncodeImageNoHuffman(
        bw,
        &map,
        hash_chain,
        refs,
        width.div_ceil(1 << bits),
        height.div_ceil(1 << bits),
        quality,
        low_effort,
    )?;
    Ok(bits)
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::enc::backward_references_enc::{VP8LBackwardRefsInit, VP8LHashChainInit};
    use crate::utils::bit_writer_utils::*;

    #[test]
    fn combined_trials_retain_first_minimum_by_rounded_byte_count() {
        let mut tied = false;
        let mut distinct_tie = false;
        let mut later_winner = false;
        for pattern in 0..4 {
            let pixels: Vec<_> = (0..323)
                .map(|i| match pattern {
                    0 => 0xff123456,
                    1 => 0xff000000 | ((i % 4) * 0x010307),
                    2 => (i % 17) * 0x123457,
                    _ => 0xff000000 | (i * 0x010307),
                })
                .collect();
            let analysis = EncoderAnalyze(&pixels, 19, 17, 19, 6, 100).unwrap();
            let make_encoder = |analysis| VP8LEncoder {
                source: &pixels,
                width: 19,
                height: 17,
                stride: 19,
                method: 6,
                quality: 100,
                exact: true,
                near_lossless: 100,
                evaluated_candidates: Vec::new(),
                analysis,
                argb: Vec::new(),
                current_width: 19,
                hash_chain: VP8LHashChainInit(pixels.len()).unwrap(),
                refs: std::array::from_fn(|_| VP8LBackwardRefsInit(256)),
            };
            for prefix_bits in 0..=7 {
                let mut prefix = VP8LBitWriterInit(0).unwrap();
                VP8LPutBits(&mut prefix, 0x55 & (0x7f >> (7 - prefix_bits)), prefix_bits).unwrap();
                let mut best_size = usize::MAX;
                let mut best_bytes = Vec::new();
                let mut best_stats = LosslessStreamStats::default();
                for (i, config) in analysis.crunch_configs.iter().enumerate() {
                    let mut one = analysis.clone();
                    one.crunch_configs = vec![config.clone()];
                    let mut bw = VP8LBitWriterClone(&prefix).unwrap();
                    let stats = EncodeStreamHook(&mut make_encoder(one), &mut bw).unwrap();
                    let size = VP8LBitWriterNumBytes(&bw);
                    let bytes = VP8LBitWriterFinish(&mut bw).unwrap().to_vec();
                    if size < best_size {
                        later_winner |= i > 0;
                        best_size = size;
                        best_bytes = bytes;
                        best_stats = stats;
                    } else if size == best_size {
                        tied = true;
                        distinct_tie |= bytes != best_bytes;
                    }
                }
                let stats =
                    EncodeStreamHook(&mut make_encoder(analysis.clone()), &mut prefix).unwrap();
                assert_eq!(stats, best_stats);
                assert_eq!(VP8LBitWriterFinish(&mut prefix).unwrap(), best_bytes);
            }
        }
        assert!(
            tied && distinct_tie && later_winner,
            "equal sizes with different streams, and later winning trials"
        );
    }
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

#[cfg(test)]
mod near_lossless_tests {
    use super::*;
    use crate::utils::bit_writer_utils::*;

    #[test]
    fn candidate_routes_transforms_and_full_loop_match_pinned_c() {
        let mut routes = [0; 6];
        let mut selected = [0; 6];
        let mut count = 0;
        let mut replays = std::collections::BTreeMap::new();
        let mut duplicates = 0;
        let mut losing_trials = 0;
        for line in include_str!("../../tests/fixtures/near_lossless_streams.txt")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let (request, response) = line.split_once(" | ").unwrap();
            let a: Vec<u32> = request
                .split_whitespace()
                .skip(1)
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect();
            let expected: Vec<u32> = response
                .split_whitespace()
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect();
            let (width, height, stride) = (a[0] as usize, a[1] as usize, a[2] as usize);
            let mut analysis = EncoderAnalyze(&a[8..], width, height, stride, a[3], a[4]).unwrap();
            let num_configs = analysis.crunch_configs.len();
            let entropy = if a[7] == u32::MAX {
                u32::MAX
            } else {
                let config = analysis.crunch_configs[a[7] as usize].clone();
                let entropy = config.entropy_idx as u32;
                analysis.crunch_configs = vec![config];
                entropy
            };
            let size = width * height;
            let mut enc = VP8LEncoder {
                source: &a[8..],
                width,
                height,
                stride,
                method: a[3],
                quality: a[4],
                near_lossless: a[5],
                evaluated_candidates: Vec::new(),
                exact: a[6] != 0,
                analysis,
                argb: Vec::new(),
                current_width: width,
                hash_chain: super::super::backward_references_enc::VP8LHashChainInit(size).unwrap(),
                refs: std::array::from_fn(|_| {
                    super::super::backward_references_enc::VP8LBackwardRefsInit((size - 1) / 16 + 1)
                }),
            };
            let mut bw = VP8LBitWriterInit(0).unwrap();
            VP8LPutBits(&mut bw, 0x15, 5).unwrap();
            let stats = EncodeStreamHook(&mut enc, &mut bw).unwrap();
            let features = if entropy == u32::MAX {
                // The final scratch buffer belongs to the last evaluated trial;
                // the selected stream/stats can belong to an earlier one.
                let last = enc.analysis.crunch_configs.last().unwrap().entropy_idx;
                last as u32
            } else {
                entropy
            };
            let route = route(features, a[5]);
            if a[7] == u32::MAX {
                let mut best = (usize::MAX, 0);
                for &(index, size, features) in &enc.evaluated_candidates {
                    assert_eq!(replays[&(index as u32)], (size, features));
                    let entropy = enc.analysis.crunch_configs[index].entropy_idx as u32;
                    routes[self::route(entropy, a[5]) as usize] += 1;
                    if size < best.0 {
                        best = (size, features);
                    }
                }
                assert_eq!(best.1, stats.lossless_features);
                duplicates += num_configs - enc.evaluated_candidates.len();
                losing_trials += enc.evaluated_candidates.len() - 1;
                replays.clear();
                let f = stats.lossless_features;
                let r = if f & 8 != 0 {
                    if f & 1 != 0 { 5 } else { 4 }
                } else if a[5] == 100 {
                    0
                } else if f & 1 == 0 {
                    1
                } else if f & 4 != 0 {
                    3
                } else {
                    2
                };
                selected[r] += 1;
            } else {
                let (_, size, features) = enc.evaluated_candidates[0];
                replays.insert(a[7], (size, features));
            }
            let mut actual = vec![
                num_configs as u32,
                entropy,
                enc.current_width as u32,
                route,
                stats.lossless_features,
                stats.histogram_bits,
                stats.transform_bits,
                stats.cross_color_transform_bits,
                stats.cache_bits,
                stats.palette_size as u32,
                stats.lossless_size as u32,
                stats.lossless_hdr_size as u32,
                stats.lossless_data_size as u32,
                enc.argb.len() as u32,
            ];
            actual.extend(enc.argb);
            VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
            let bytes = VP8LBitWriterFinish(&mut bw).unwrap();
            actual.push(bytes.len() as u32);
            actual.extend(bytes.iter().map(|&b| u32::from(b)));
            let first = expected.iter().zip(&actual).position(|(a, b)| a != b);
            assert!(
                actual == expected,
                "candidate index {}, strength {}, first difference {first:?}; lengths {}/{}",
                a[7],
                a[5],
                actual.len(),
                expected.len()
            );
            count += 1;
        }
        assert!(
            routes.into_iter().all(|count| count > 0),
            "routes: {routes:?}"
        );
        assert!(selected[0] > 0 && selected[1..].iter().sum::<usize>() > 0);
        assert!(duplicates > 0 && losing_trials > 0);
        eprintln!(
            "{count} exact C candidate/full-loop comparisons; evaluated {routes:?}; selected {selected:?}"
        );
    }

    fn route(entropy: u32, strength: u32) -> u32 {
        use EntropyIx::*;
        if entropy == kPalette as u32 {
            4
        } else if entropy == kPaletteAndSpatial as u32 {
            5
        } else if strength == 100 {
            0
        } else if entropy == kSpatial as u32 {
            2
        } else if entropy == kSpatialSubGreen as u32 {
            3
        } else {
            1
        }
    }
}
