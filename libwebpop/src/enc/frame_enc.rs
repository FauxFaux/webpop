// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/frame_enc.c, probability-fitting foundations.
// See COPYING and PATENTS.
#![allow(non_upper_case_globals)]

use crate::enc::{cost_enc::*, tree_enc::*, vp8i_enc::*};

pub const VP8Cat3: [u8; 3] = [173, 148, 140];
pub const VP8Cat4: [u8; 4] = [176, 155, 140, 135];
pub const VP8Cat5: [u8; 5] = [180, 157, 141, 134, 130];
pub const VP8Cat6: [u8; 11] = [254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129];

pub fn ResetStats(proba: &mut VP8EncProba) {
    VP8CalculateLevelCosts(proba);
    proba.nb_skip = 0;
}

pub fn ResetTokenStats(proba: &mut VP8EncProba) {
    proba.stats = [[[[0; NUM_PROBAS]; NUM_CTX]; NUM_BANDS]; NUM_TYPES];
}

pub fn CalcTokenProba(nb: u32, total: u32) -> u8 {
    assert!(nb <= total && total <= 65535);
    if nb == 0 {
        255
    } else {
        (255 - nb * 255 / total) as u8
    }
}

pub fn BranchCost(nb: u32, total: u32, proba: u8) -> i32 {
    assert!(nb <= total && total <= 65535);
    nb as i32 * VP8BitCost(1, proba) + (total - nb) as i32 * VP8BitCost(0, proba)
}

/// Fit against the default probabilities, preserving the pin's strict
/// cost comparison and its dirty flag relative to those defaults.
pub fn FinalizeTokenProbas(proba: &mut VP8EncProba) -> i32 {
    let mut has_changed = false;
    let mut size = 0;
    for t in 0..NUM_TYPES {
        for b in 0..NUM_BANDS {
            for c in 0..NUM_CTX {
                for p in 0..NUM_PROBAS {
                    let stats = proba.stats[t][b][c][p];
                    let nb = stats & 65535;
                    let total = stats >> 16;
                    let update_proba = VP8CoeffsUpdateProba[t][b][c][p];
                    let old_p = VP8CoeffsProba0[t][b][c][p];
                    let new_p = CalcTokenProba(nb, total);
                    let old_cost = BranchCost(nb, total, old_p) + VP8BitCost(0, update_proba);
                    let new_cost =
                        BranchCost(nb, total, new_p) + VP8BitCost(1, update_proba) + 8 * 256;
                    let use_new_p = old_cost > new_cost;
                    size += VP8BitCost(i32::from(use_new_p), update_proba);
                    if use_new_p {
                        proba.coeffs[t][b][c][p] = new_p;
                        has_changed |= new_p != old_p;
                        size += 8 * 256;
                    } else {
                        proba.coeffs[t][b][c][p] = old_p;
                    }
                }
            }
        }
    }
    proba.dirty = has_changed;
    size
}
