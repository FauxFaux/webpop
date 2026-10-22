// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/predictor_enc.c.
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

/// Find the largest uniform power-of-two tiles and compact their top-left
/// samples in place. The untouched tail is retained, matching upstream.
pub fn VP8LOptimizeSampling(
    image: &mut [u32],
    full_width: usize,
    full_height: usize,
    bits: u32,
    max_bits: u32,
) -> u32 {
    assert!(full_width > 0 && full_height > 0 && bits <= max_bits && max_bits < usize::BITS);
    let width = full_width.div_ceil(1 << bits);
    let height = full_height.div_ceil(1 << bits);
    assert!(image.len() >= width.checked_mul(height).expect("image geometry"));
    let mut best_bits = bits;
    // Check rows before columns; partial groups at the edges are allowed.
    while best_bits < max_bits {
        let square = 1 << (best_bits - bits);
        let good = (0..height)
            .step_by(square * 2)
            .take_while(|&y| y + square < height)
            .all(|y| {
                image[y * width..(y + 1) * width]
                    == image[(y + square) * width..(y + square + 1) * width]
            });
        if !good {
            break;
        }
        best_bits += 1;
    }
    while best_bits > bits {
        let square = 1 << (best_bits - bits);
        let good = (0..height).all(|y| {
            (0..width).step_by(square).all(|x| {
                (x + 1..(x + square).min(width))
                    .all(|i| image[y * width + i] == image[y * width + x])
            })
        });
        if good {
            break;
        }
        best_bits -= 1;
    }
    if best_bits > bits {
        let square = 1 << (best_bits - bits);
        let new_width = full_width.div_ceil(1 << best_bits);
        let new_height = full_height.div_ceil(1 << best_bits);
        for y in 0..new_height {
            for x in 0..new_width {
                image[y * new_width + x] = image[square * (y * width + x)];
            }
        }
    }
    best_bits
}
