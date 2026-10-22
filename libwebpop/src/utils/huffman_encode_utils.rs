// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/huffman_encode_utils.{c,h}.
// See COPYING and PATENTS.

use std::collections::TryReserveError;

use crate::webp::format_constants::MAX_ALLOWED_CODE_LENGTH;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HuffmanTreeToken {
    pub code: u8,
    pub extra_bits: u8,
}

/// The number of symbols is code_lengths.len(); both arrays have that length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HuffmanTreeCode {
    pub code_lengths: Vec<u8>,
    pub codes: Vec<u16>,
}

#[derive(Clone, Copy, Default)]
struct HuffmanTree {
    total_count: u32,
    value: usize,
    children: Option<(usize, usize)>,
}

fn ValuesShouldBeCollapsedToStrideAverage(a: u32, b: u32) -> bool {
    // Counts in encoder histograms fit in a signed int, as required by C's
    // signed subtraction/abs helper.
    a.abs_diff(b) < 4
}

fn OptimizeHuffmanForRle(good_for_rle: &mut [u8], counts: &mut [u32]) {
    let Some(last) = counts.iter().rposition(|&c| c != 0) else {
        return;
    };
    let length = last + 1;
    let mut symbol = counts[0];
    let mut stride = 0;
    for i in 0..=length {
        if i == length || counts[i] != symbol {
            if (symbol == 0 && stride >= 5) || (symbol != 0 && stride >= 7) {
                good_for_rle[i - stride..i].fill(1);
            }
            stride = 1;
            if i != length {
                symbol = counts[i];
            }
        } else {
            stride += 1;
        }
    }
    let mut stride = 0_u32;
    let mut limit = counts[0];
    let mut sum = 0_u32;
    for i in 0..=length {
        if i == length
            || good_for_rle[i] != 0
            || (i != 0 && good_for_rle[i - 1] != 0)
            || !ValuesShouldBeCollapsedToStrideAverage(counts[i], limit)
        {
            if stride >= 4 || (stride >= 3 && sum == 0) {
                let count = if sum == 0 {
                    0
                } else {
                    (sum.wrapping_add(stride / 2) / stride).max(1)
                };
                counts[i - stride as usize..i].fill(count);
            }
            stride = 0;
            sum = 0;
            limit = if i + 3 < length {
                counts[i]
                    .wrapping_add(counts[i + 1])
                    .wrapping_add(counts[i + 2])
                    .wrapping_add(counts[i + 3])
                    .wrapping_add(2)
                    / 4
            } else if i < length {
                counts[i]
            } else {
                0
            };
        }
        stride += 1;
        if i != length {
            sum = sum.wrapping_add(counts[i]);
            if stride >= 4 {
                limit = sum.wrapping_add(stride / 2) / stride;
            }
        }
    }
}

fn CompareHuffmanTrees(a: &HuffmanTree, b: &HuffmanTree) -> std::cmp::Ordering {
    // The C comparator orders equal counts by *increasing* symbol value,
    // despite the comment above that comparator saying otherwise.
    b.total_count
        .cmp(&a.total_count)
        .then(a.value.cmp(&b.value))
}

fn SetBitDepths(tree: &HuffmanTree, pool: &[HuffmanTree], depths: &mut [u8], level: u8) {
    if let Some((left, right)) = tree.children {
        SetBitDepths(&pool[left], pool, depths, level + 1);
        SetBitDepths(&pool[right], pool, depths, level + 1);
    } else {
        depths[tree.value] = level;
    }
}

fn GenerateOptimalTree(
    histogram: &[u32],
    tree_depth_limit: u32,
    bit_depths: &mut [u8],
) -> Result<(), TryReserveError> {
    let size = histogram.iter().filter(|&&c| c != 0).count();
    if size == 0 {
        return Ok(());
    }
    assert!(size <= 1 << (tree_depth_limit - 1));
    let mut tree = Vec::new();
    tree.try_reserve_exact(size)?;
    let mut pool = Vec::new();
    pool.try_reserve_exact(2 * size)?;
    let mut count_min = 1;
    loop {
        tree.clear();
        pool.clear();
        tree.extend(histogram.iter().enumerate().filter_map(|(value, &count)| {
            (count != 0).then_some(HuffmanTree {
                total_count: count.max(count_min),
                value,
                children: None,
            })
        }));
        tree.sort_unstable_by(CompareHuffmanTrees);
        if size == 1 {
            bit_depths[tree[0].value] = 1;
        } else {
            while tree.len() > 1 {
                pool.push(tree.pop().unwrap());
                pool.push(tree.pop().unwrap());
                let left = pool.len() - 1;
                let right = pool.len() - 2;
                let count = pool[left].total_count.wrapping_add(pool[right].total_count);
                let k = tree
                    .iter()
                    .position(|t| t.total_count <= count)
                    .unwrap_or(tree.len());
                tree.insert(
                    k,
                    HuffmanTree {
                        total_count: count,
                        value: 0,
                        children: Some((left, right)),
                    },
                );
            }
            SetBitDepths(&tree[0], &pool, bit_depths, 0);
        }
        if u32::from(*bit_depths.iter().max().unwrap()) <= tree_depth_limit {
            break;
        }
        count_min = count_min
            .checked_mul(2)
            .expect("histogram counts exceed encoder domain");
    }
    Ok(())
}

fn CodeRepeatedValues(
    mut repetitions: usize,
    tokens: &mut Vec<HuffmanTreeToken>,
    value: u8,
    prev: u8,
) {
    if value != prev {
        tokens.push(HuffmanTreeToken {
            code: value,
            extra_bits: 0,
        });
        repetitions -= 1;
    }
    while repetitions > 0 {
        if repetitions < 3 {
            tokens.extend((0..repetitions).map(|_| HuffmanTreeToken {
                code: value,
                extra_bits: 0,
            }));
            break;
        } else if repetitions < 7 {
            tokens.push(HuffmanTreeToken {
                code: 16,
                extra_bits: (repetitions - 3) as u8,
            });
            break;
        } else {
            tokens.push(HuffmanTreeToken {
                code: 16,
                extra_bits: 3,
            });
            repetitions -= 6;
        }
    }
}

fn CodeRepeatedZeros(mut repetitions: usize, tokens: &mut Vec<HuffmanTreeToken>) {
    while repetitions > 0 {
        if repetitions < 3 {
            tokens.extend((0..repetitions).map(|_| HuffmanTreeToken {
                code: 0,
                extra_bits: 0,
            }));
            break;
        } else if repetitions < 11 {
            tokens.push(HuffmanTreeToken {
                code: 17,
                extra_bits: (repetitions - 3) as u8,
            });
            break;
        } else if repetitions < 139 {
            tokens.push(HuffmanTreeToken {
                code: 18,
                extra_bits: (repetitions - 11) as u8,
            });
            break;
        } else {
            tokens.push(HuffmanTreeToken {
                code: 18,
                extra_bits: 0x7f,
            });
            repetitions -= 138;
        }
    }
}

pub fn VP8LCreateCompressedHuffmanTree(
    tree: &HuffmanTreeCode,
) -> Result<Vec<HuffmanTreeToken>, TryReserveError> {
    let depths = &tree.code_lengths;
    let mut tokens = Vec::new();
    tokens.try_reserve_exact(depths.len())?;
    let mut prev = 8;
    let mut i = 0;
    while i < depths.len() {
        let value = depths[i];
        assert!(value <= MAX_ALLOWED_CODE_LENGTH as u8);
        let mut k = i + 1;
        while k < depths.len() && depths[k] == value {
            k += 1;
        }
        if value == 0 {
            CodeRepeatedZeros(k - i, &mut tokens);
        } else {
            CodeRepeatedValues(k - i, &mut tokens, value, prev);
            prev = value;
        }
        i = k;
    }
    Ok(tokens)
}

fn ReverseBits(num_bits: u8, bits: u32) -> u16 {
    if num_bits == 0 {
        0
    } else {
        (bits.reverse_bits() >> (32 - num_bits)) as u16
    }
}

fn ConvertBitDepthsToSymbols(tree: &mut HuffmanTreeCode) {
    let mut depth_count = [0_u32; MAX_ALLOWED_CODE_LENGTH + 1];
    let mut next_code = [0_u32; MAX_ALLOWED_CODE_LENGTH + 1];
    for &depth in &tree.code_lengths {
        depth_count[depth as usize] += 1;
    }
    depth_count[0] = 0;
    let mut code = 0;
    for i in 1..=MAX_ALLOWED_CODE_LENGTH {
        code = (code + depth_count[i - 1]) << 1;
        next_code[i] = code;
    }
    for (&depth, code) in tree.code_lengths.iter().zip(&mut tree.codes) {
        *code = ReverseBits(depth, next_code[depth as usize]);
        next_code[depth as usize] += 1;
    }
}

/// Builds codes and mutates histogram counts using upstream's RLE heuristic.
/// Depth limits and histogram sizes follow the internal C preconditions.
/// Scratch storage and code arrays are owned instead of supplied by the caller.
pub fn VP8LCreateHuffmanTree(
    histogram: &mut [u32],
    tree_depth_limit: u32,
) -> Result<HuffmanTreeCode, TryReserveError> {
    assert!((1..=MAX_ALLOWED_CODE_LENGTH as u32).contains(&tree_depth_limit));
    let n = histogram.len();
    let mut rle = Vec::new();
    rle.try_reserve_exact(n)?;
    rle.resize(n, 0);
    OptimizeHuffmanForRle(&mut rle, histogram);
    let mut codes = Vec::new();
    codes.try_reserve_exact(n)?;
    codes.resize(n, 0);
    let mut tree = HuffmanTreeCode {
        code_lengths: rle,
        codes,
    };
    tree.code_lengths.fill(0);
    GenerateOptimalTree(histogram, tree_depth_limit, &mut tree.code_lengths)?;
    ConvertBitDepthsToSymbols(&mut tree);
    Ok(tree)
}
