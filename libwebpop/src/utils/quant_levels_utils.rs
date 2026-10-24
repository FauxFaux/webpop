// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/quant_levels_utils.c. See COPYING and PATENTS.

/// Centroid quantization with fixed extrema. SSE is the truncated convergence
/// error before centroids are rounded, rather than the final integer-plane SSE.
/// Invalid geometry, buffers or levels leave the input unchanged.
pub fn QuantizeLevels(
    data: &mut [u8],
    width: usize,
    height: usize,
    num_levels: usize,
) -> Option<u64> {
    let size = width.checked_mul(height)?;
    if width == 0 || height == 0 || data.len() < size || !(2..=256).contains(&num_levels) {
        return None;
    }
    // Wide frequencies/products avoid signed C aggregate overflow on big planes.
    let mut freq = [0u64; 256];
    let mut q_level = [0usize; 256];
    let mut inv_q_level = [0f64; 256];
    let (mut min_s, mut max_s, mut num_levels_in) = (255usize, 0usize, 0);
    for &v in &data[..size] {
        let s = usize::from(v);
        num_levels_in += usize::from(freq[s] == 0);
        freq[s] += 1;
        min_s = min_s.min(s);
        max_s = max_s.max(s);
    }
    let mut err = 0.;
    if num_levels_in > num_levels {
        for (i, centroid) in inv_q_level[..num_levels].iter_mut().enumerate() {
            *centroid = min_s as f64 + (max_s - min_s) as f64 * i as f64 / (num_levels - 1) as f64;
        }
        q_level[min_s] = 0;
        q_level[max_s] = num_levels - 1;
        let mut last_err = 1e38;
        let err_threshold = 1e-4 * size as f64;
        for _ in 0..6 {
            let (mut q_sum, mut q_count) = ([0f64; 256], [0f64; 256]);
            let mut slot = 0;
            for s in min_s..=max_s {
                while slot < num_levels - 1
                    && (2 * s) as f64 > inv_q_level[slot] + inv_q_level[slot + 1]
                {
                    slot += 1;
                }
                if freq[s] > 0 {
                    q_sum[slot] += (s as u64 * freq[s]) as f64;
                    q_count[slot] += freq[s] as f64;
                }
                q_level[s] = slot;
            }
            for slot in 1..num_levels - 1 {
                if q_count[slot] > 0. {
                    inv_q_level[slot] = q_sum[slot] / q_count[slot];
                }
            }
            err = 0.;
            for s in min_s..=max_s {
                let error = s as f64 - inv_q_level[q_level[s]];
                err += freq[s] as f64 * error * error;
            }
            if last_err - err < err_threshold {
                break;
            }
            last_err = err;
        }
        let mut map = [0u8; 256];
        for s in min_s..=max_s {
            map[s] = (inv_q_level[q_level[s]] + 0.5) as u8;
        }
        for v in &mut data[..size] {
            *v = map[usize::from(*v)];
        }
    }
    Some(err as u64)
}
