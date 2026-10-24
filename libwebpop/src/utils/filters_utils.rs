// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/filters_utils.c. See COPYING and PATENTS.
use crate::dsp::filters::{GradientPredictor_C, WEBP_FILTER_LAST};

pub fn WebPEstimateBestFilter(data: &[u8], width: usize, height: usize) -> usize {
    assert!(width > 0 && height > 0 && width.checked_mul(height).is_some_and(|n| data.len() >= n));
    let mut bins = [[false; 16]; WEBP_FILTER_LAST];
    for y in (2..height.saturating_sub(1)).step_by(2) {
        let at = y * width;
        let mut mean = i32::from(data[at]);
        for x in (2..width.saturating_sub(1)).step_by(2) {
            let value = i32::from(data[at + x]);
            let predictions = [
                mean,
                i32::from(data[at + x - 1]),
                i32::from(data[at + x - width]),
                i32::from(GradientPredictor_C(
                    data[at + x - 1],
                    data[at + x - width],
                    data[at + x - width - 1],
                )),
            ];
            for (bin, pred) in bins.iter_mut().zip(predictions) {
                bin[((value - pred).abs() >> 4) as usize] = true;
            }
            mean = (3 * mean + value + 2) >> 2;
        }
    }
    (0..WEBP_FILTER_LAST)
        .min_by_key(|&filter| {
            bins[filter]
                .iter()
                .enumerate()
                .filter(|(_, present)| **present)
                .map(|(i, _)| i)
                .sum::<usize>()
        })
        .unwrap()
}
