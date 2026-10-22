// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/lossless.{c,h}, functions of the same names.
// See COPYING and PATENTS.

// Retain upstream dispatch/table names.
#![allow(non_upper_case_globals)]

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VP8LMultipliers {
    // Byte representations of signed coefficients, as in upstream.
    pub green_to_red: u8,
    pub green_to_blue: u8,
    pub red_to_blue: u8,
}

// top is [top-left, top, top-right]; left is the preceding pixel.
pub type VP8LPredictorFunc = fn(u32, &[u32; 3]) -> u32;

fn Average2(a0: u32, a1: u32) -> u32 {
    (((a0 ^ a1) & 0xfefefefe) >> 1) + (a0 & a1)
}
fn Average3(a0: u32, a1: u32, a2: u32) -> u32 {
    Average2(Average2(a0, a2), a1)
}
fn Average4(a0: u32, a1: u32, a2: u32, a3: u32) -> u32 {
    Average2(Average2(a0, a1), Average2(a2, a3))
}
fn AddSubtractComponentFull(a: i32, b: i32, c: i32) -> u32 {
    (a + b - c).clamp(0, 255) as u32
}
fn AddSubtractComponentHalf(a: i32, b: i32) -> u32 {
    // Signed division must truncate toward zero, rather than shifting.
    (a + (a - b) / 2).clamp(0, 255) as u32
}
fn ClampedAddSubtractFull(c0: u32, c1: u32, c2: u32) -> u32 {
    let mut out = 0;
    for shift in [0, 8, 16, 24] {
        out |= AddSubtractComponentFull(
            ((c0 >> shift) & 255) as i32,
            ((c1 >> shift) & 255) as i32,
            ((c2 >> shift) & 255) as i32,
        ) << shift;
    }
    out
}
fn ClampedAddSubtractHalf(c0: u32, c1: u32, c2: u32) -> u32 {
    let ave = Average2(c0, c1);
    let mut out = 0;
    for shift in [0, 8, 16, 24] {
        out |=
            AddSubtractComponentHalf(((ave >> shift) & 255) as i32, ((c2 >> shift) & 255) as i32)
                << shift;
    }
    out
}
fn Sub3(a: i32, b: i32, c: i32) -> i32 {
    (b - c).abs() - (a - c).abs()
}
fn Select(a: u32, b: u32, c: u32) -> u32 {
    let mut pa_minus_pb = 0;
    for shift in [0, 8, 16, 24] {
        pa_minus_pb += Sub3(
            ((a >> shift) & 255) as i32,
            ((b >> shift) & 255) as i32,
            ((c >> shift) & 255) as i32,
        );
    }
    if pa_minus_pb <= 0 { a } else { b }
}

pub fn VP8LPredictor0_C(_left: u32, _top: &[u32; 3]) -> u32 {
    0xff000000
}

pub fn VP8LPredictor1_C(left: u32, _top: &[u32; 3]) -> u32 {
    left
}

pub fn VP8LPredictor2_C(_left: u32, top: &[u32; 3]) -> u32 {
    top[1]
}

pub fn VP8LPredictor3_C(_left: u32, top: &[u32; 3]) -> u32 {
    top[2]
}

pub fn VP8LPredictor4_C(_left: u32, top: &[u32; 3]) -> u32 {
    top[0]
}

pub fn VP8LPredictor5_C(left: u32, top: &[u32; 3]) -> u32 {
    Average3(left, top[1], top[2])
}

pub fn VP8LPredictor6_C(left: u32, top: &[u32; 3]) -> u32 {
    Average2(left, top[0])
}

pub fn VP8LPredictor7_C(left: u32, top: &[u32; 3]) -> u32 {
    Average2(left, top[1])
}

pub fn VP8LPredictor8_C(_left: u32, top: &[u32; 3]) -> u32 {
    Average2(top[0], top[1])
}

pub fn VP8LPredictor9_C(_left: u32, top: &[u32; 3]) -> u32 {
    Average2(top[1], top[2])
}

pub fn VP8LPredictor10_C(left: u32, top: &[u32; 3]) -> u32 {
    Average4(left, top[0], top[1], top[2])
}

pub fn VP8LPredictor11_C(left: u32, top: &[u32; 3]) -> u32 {
    Select(top[1], left, top[0])
}

pub fn VP8LPredictor12_C(left: u32, top: &[u32; 3]) -> u32 {
    ClampedAddSubtractFull(left, top[1], top[0])
}

pub fn VP8LPredictor13_C(left: u32, top: &[u32; 3]) -> u32 {
    ClampedAddSubtractHalf(left, top[1], top[0])
}

pub const VP8LPredictors: [VP8LPredictorFunc; 14] = [
    VP8LPredictor0_C,
    VP8LPredictor1_C,
    VP8LPredictor2_C,
    VP8LPredictor3_C,
    VP8LPredictor4_C,
    VP8LPredictor5_C,
    VP8LPredictor6_C,
    VP8LPredictor7_C,
    VP8LPredictor8_C,
    VP8LPredictor9_C,
    VP8LPredictor10_C,
    VP8LPredictor11_C,
    VP8LPredictor12_C,
    VP8LPredictor13_C,
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VP8LStreaks {
    pub counts: [i32; 2],
    // [zero/nonzero][length <= 3 / length > 3]
    pub streaks: [[i32; 2]; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8LBitEntropy {
    pub entropy: u64,
    pub sum: u32,
    pub nonzeros: i32,
    pub max_val: u32,
    pub nonzero_code: u32,
}

impl Default for VP8LBitEntropy {
    fn default() -> Self {
        Self {
            entropy: 0,
            sum: 0,
            nonzeros: 0,
            max_val: 0,
            nonzero_code: 0xffff,
        }
    }
}
