// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8i_enc.h, encoder foundations.
// See COPYING and PATENTS.

use crate::dsp::enc::BPS;

pub const MAX_LEVEL: i32 = 2047;
pub const MAX_VARIABLE_LEVEL: usize = 67;
pub const NUM_TYPES: usize = 4;
pub const NUM_BANDS: usize = 8;
pub const NUM_CTX: usize = 3;
pub const NUM_PROBAS: usize = 11;
pub const QFIX: u32 = 17;

#[allow(non_camel_case_types)]
pub type proba_t = u32;
pub type ProbaArray = [[u8; NUM_PROBAS]; NUM_CTX];
pub type StatsArray = [[proba_t; NUM_PROBAS]; NUM_CTX];
pub type CostArray = [[u16; MAX_VARIABLE_LEVEL + 1]; NUM_CTX];

/// Frame-persistent probabilities and cost cache. Band-remapped cost pointers
/// are resolved on access, avoiding self references when this value moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8EncProba {
    pub segments: [u8; 3],
    pub skip_proba: u8,
    pub coeffs: [[ProbaArray; NUM_BANDS]; NUM_TYPES],
    pub stats: [[StatsArray; NUM_BANDS]; NUM_TYPES],
    pub level_cost: [[CostArray; NUM_BANDS]; NUM_TYPES],
    pub dirty: bool,
    pub use_skip_proba: bool,
    pub nb_skip: i32,
}

impl Default for VP8EncProba {
    fn default() -> Self {
        Self {
            segments: [0; 3],
            skip_proba: 0,
            coeffs: [[[[0; NUM_PROBAS]; NUM_CTX]; NUM_BANDS]; NUM_TYPES],
            stats: [[[[0; NUM_PROBAS]; NUM_CTX]; NUM_BANDS]; NUM_TYPES],
            level_cost: [[[[0; 68]; NUM_CTX]; NUM_BANDS]; NUM_TYPES],
            dirty: false,
            use_skip_proba: false,
            nb_skip: 0,
        }
    }
}

impl VP8EncProba {
    #[inline]
    pub fn remapped_costs(&self, coeff_type: usize, n: usize, ctx: usize) -> &[u16; 68] {
        assert!(n < 16);
        &self.level_cost[coeff_type][crate::dsp::cost::VP8EncBands[n]][ctx]
    }
}

#[inline]
pub fn BIAS(b: u32) -> u32 {
    b.wrapping_shl(QFIX - 8)
}

#[inline]
pub fn QUANTDIV(n: u32, iQ: u32, B: u32) -> i32 {
    (n.wrapping_mul(iQ).wrapping_add(B) >> QFIX) as i32
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VP8Matrix {
    pub q: [u16; 16],
    pub iq: [u16; 16],
    pub bias: [u32; 16],
    pub zthresh: [u32; 16],
    pub sharpen: [u16; 16],
}

pub const PRED_SIZE_ENC: usize = (32 + 16 + 8) * BPS;

pub const I16DC16: usize = 0;
pub const I16TM16: usize = I16DC16 + 16;
pub const I16VE16: usize = 16 * BPS;
pub const I16HE16: usize = I16VE16 + 16;

pub const C8DC8: usize = 32 * BPS;
pub const C8TM8: usize = C8DC8 + 16;
pub const C8VE8: usize = C8DC8 + 8 * BPS;
pub const C8HE8: usize = C8VE8 + 16;

pub const I4DC4: usize = 48 * BPS;
pub const I4TM4: usize = I4DC4 + 4;
pub const I4VE4: usize = I4DC4 + 8;
pub const I4HE4: usize = I4DC4 + 12;
pub const I4RD4: usize = I4DC4 + 16;
pub const I4VR4: usize = I4DC4 + 20;
pub const I4LD4: usize = I4DC4 + 24;
pub const I4VL4: usize = I4DC4 + 28;
pub const I4HD4: usize = I4DC4 + 4 * BPS;
pub const I4HU4: usize = I4HD4 + 4;
pub const I4TMP: usize = I4HD4 + 8;
