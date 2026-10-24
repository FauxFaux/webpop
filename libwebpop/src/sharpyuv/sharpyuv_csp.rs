//! Direct port of `sharpyuv/sharpyuv_csp.c` and its public types.
// Copyright 2022 Google Inc. All Rights Reserved.
// Use of this source code is governed by the BSD-style license in COPYING.
// An additional intellectual property rights grant is in PATENTS.

use super::{SharpYuvConversionMatrix, SharpYuvError};

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharpYuvRange {
    kSharpYuvRangeFull,
    kSharpYuvRangeLimited,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SharpYuvColorSpace {
    pub kr: f32,
    pub kb: f32,
    pub bit_depth: u32,
    pub range: SharpYuvRange,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SharpYuvMatrixType {
    kSharpYuvMatrixWebp,
    kSharpYuvMatrixRec601Limited,
    kSharpYuvMatrixRec601Full,
    kSharpYuvMatrixRec709Limited,
    kSharpYuvMatrixRec709Full,
}

pub const MATRICES: [SharpYuvConversionMatrix; 5] = [
    SharpYuvConversionMatrix {
        rgb_to_y: [16839, 33059, 6420, 16 << 16],
        rgb_to_u: [-9719, -19081, 28800, 128 << 16],
        rgb_to_v: [28800, -24116, -4684, 128 << 16],
    },
    SharpYuvConversionMatrix {
        rgb_to_y: [16829, 33039, 6416, 16 << 16],
        rgb_to_u: [-9714, -19071, 28784, 128 << 16],
        rgb_to_v: [28784, -24103, -4681, 128 << 16],
    },
    SharpYuvConversionMatrix {
        rgb_to_y: [19595, 38470, 7471, 0],
        rgb_to_u: [-11058, -21710, 32768, 128 << 16],
        rgb_to_v: [32768, -27439, -5329, 128 << 16],
    },
    SharpYuvConversionMatrix {
        rgb_to_y: [11966, 40254, 4064, 16 << 16],
        rgb_to_u: [-6596, -22189, 28784, 128 << 16],
        rgb_to_v: [28784, -26145, -2639, 128 << 16],
    },
    SharpYuvConversionMatrix {
        rgb_to_y: [13933, 46871, 4732, 0],
        rgb_to_u: [-7509, -25259, 32768, 128 << 16],
        rgb_to_v: [32768, -29763, -3005, 128 << 16],
    },
];

pub fn SharpYuvGetConversionMatrix(
    matrix_type: SharpYuvMatrixType,
) -> &'static SharpYuvConversionMatrix {
    &MATRICES[matrix_type as usize]
}

fn ToFixed16(f: f32) -> i32 {
    f64::from(f * 65536.0 + 0.5).floor() as i32
}

/// Compute coefficients for physical Kr/Kb weights and YUV depth 8/10/12.
pub fn SharpYuvComputeConversionMatrix(
    csp: &SharpYuvColorSpace,
) -> Result<SharpYuvConversionMatrix, SharpYuvError> {
    if !matches!(csp.bit_depth, 8 | 10 | 12)
        || !csp.kr.is_finite()
        || !csp.kb.is_finite()
        || csp.kr < 0.0
        || csp.kb < 0.0
        || csp.kr + csp.kb >= 1.0
    {
        return Err(SharpYuvError::InvalidColorSpace);
    }
    let kr = csp.kr;
    let kb = csp.kb;
    let kg = 1.0 - kr - kb;
    let cb = 0.5 / (1.0 - kb);
    let cr = 0.5 / (1.0 - kr);
    let shift = csp.bit_depth - 8;
    let denom = ((1u32 << csp.bit_depth) - 1) as f32;
    let mut scale_y = 1.0;
    let mut add_y = 0.0;
    let mut scale_u = cb;
    let mut scale_v = cr;
    let add_uv = (128 << shift) as f32;
    if csp.range == SharpYuvRange::kSharpYuvRangeLimited {
        scale_y *= (219 << shift) as f32 / denom;
        scale_u *= (224 << shift) as f32 / denom;
        scale_v *= (224 << shift) as f32 / denom;
        add_y = (16 << shift) as f32;
    }
    Ok(SharpYuvConversionMatrix {
        rgb_to_y: [
            ToFixed16(kr * scale_y),
            ToFixed16(kg * scale_y),
            ToFixed16(kb * scale_y),
            ToFixed16(add_y),
        ],
        rgb_to_u: [
            ToFixed16(-kr * scale_u),
            ToFixed16(-kg * scale_u),
            ToFixed16((1.0 - kb) * scale_u),
            ToFixed16(add_uv),
        ],
        rgb_to_v: [
            ToFixed16((1.0 - kr) * scale_v),
            ToFixed16(-kg * scale_v),
            ToFixed16(-kb * scale_v),
            ToFixed16(add_uv),
        ],
    })
}
