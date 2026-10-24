//! Direct port of `sharpyuv/sharpyuv_gamma.c`.
// Copyright 2022 Google Inc. All Rights Reserved.
// Use of this source code is governed by the BSD-style license in COPYING.
// An additional intellectual property rights grant is in PATENTS.

use super::SharpYuvTransferFunctionType::{self, *};
use std::sync::OnceLock;

pub struct SharpYuvGammaTables {
    pub gamma_to_linear: [u32; 1026],
    pub linear_to_gamma: [u32; 514],
}

pub fn SharpYuvInitGammaTables() -> &'static SharpYuvGammaTables {
    static TABLES: OnceLock<SharpYuvGammaTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut tables = SharpYuvGammaTables {
            gamma_to_linear: [0; 1026],
            linear_to_gamma: [0; 514],
        };
        let gamma_f = 1.0 / 0.45;
        let a = 0.09929682680944;
        let thresh = 0.018053968510807;
        let final_scale = 65536.0;
        let norm = 1.0 / 1024.0;
        let a_rec = 1.0 / (1.0 + a);
        for v in 0..=1024 {
            let g = norm * v as f64;
            let value = if g <= thresh * 4.5 {
                g / 4.5
            } else {
                (a_rec * (g + a)).powf(gamma_f)
            };
            tables.gamma_to_linear[v] = (value * final_scale + 0.5) as u32;
        }
        tables.gamma_to_linear[1025] = tables.gamma_to_linear[1024];
        let scale = 1.0 / 512.0;
        for v in 0..=512 {
            let g = scale * v as f64;
            let value = if g <= thresh {
                4.5 * g
            } else {
                (1.0 + a) * g.powf(1.0 / gamma_f) - a
            };
            tables.linear_to_gamma[v] = (final_scale * value + 0.5) as u32;
        }
        tables.linear_to_gamma[513] = tables.linear_to_gamma[512];
        tables
    })
}

#[inline]
fn Shift(v: i32, shift: i32) -> i32 {
    if shift >= 0 { v << shift } else { v >> -shift }
}

#[inline]
fn FixedPointInterpolation(v: u32, tab: &[u32], pos_shift: u32, value_shift: i32) -> u32 {
    let pos = (v >> pos_shift) as usize;
    let x = v - ((pos as u32) << pos_shift);
    let v0 = Shift(tab[pos] as i32, value_shift) as u32;
    let v1 = Shift(tab[pos + 1] as i32, value_shift) as u32;
    let half = if pos_shift > 0 {
        1 << (pos_shift - 1)
    } else {
        0
    };
    v0 + (((v1 - v0) * x + half) >> pos_shift)
}

fn ToLinearSrgb(v: u16, bit_depth: u32) -> u32 {
    assert!(u32::from(v) < (1 << bit_depth));
    let table = &SharpYuvInitGammaTables().gamma_to_linear;
    if bit_depth <= 10 {
        table[(v as usize) << (10 - bit_depth)]
    } else {
        FixedPointInterpolation(u32::from(v), table, bit_depth - 10, 0)
    }
}

fn FromLinearSrgb(value: u32, bit_depth: u32) -> u16 {
    assert!(value <= 65536);
    FixedPointInterpolation(
        value,
        &SharpYuvInitGammaTables().linear_to_gamma,
        7,
        bit_depth as i32 - 16,
    ) as u16
}

#[inline]
pub fn Roundf(x: f32) -> f32 {
    if x < 0.0 {
        f64::from(x - 0.5).ceil() as f32
    } else {
        f64::from(x + 0.5).floor() as f32
    }
}

#[inline]
pub fn Powf(base: f32, exp: f32) -> f32 {
    f64::from(base).powf(f64::from(exp)) as f32
}

#[inline]
pub fn Log10f(x: f32) -> f32 {
    f64::from(x).log10() as f32
}

pub fn ToLinear709(gamma: f32) -> f32 {
    if gamma < 0.0 {
        0.0
    } else if gamma < 4.5 * 0.018053968510807f32 {
        gamma / 4.5
    } else if gamma < 1.0 {
        Powf((gamma + 0.09929682680944) / 1.09929682680944, 1.0 / 0.45)
    } else {
        1.0
    }
}

pub fn FromLinear709(linear: f32) -> f32 {
    if linear < 0.0 {
        0.0
    } else if linear < 0.018053968510807 {
        linear * 4.5
    } else if linear < 1.0 {
        1.09929682680944 * Powf(linear, 0.45) - 0.09929682680944
    } else {
        1.0
    }
}

pub fn ToLinear470M(gamma: f32) -> f32 {
    Powf(gamma.clamp(0.0, 1.0), 2.2)
}
pub fn FromLinear470M(linear: f32) -> f32 {
    Powf(linear.clamp(0.0, 1.0), 1.0 / 2.2)
}
pub fn ToLinear470Bg(gamma: f32) -> f32 {
    Powf(gamma.clamp(0.0, 1.0), 2.8)
}
pub fn FromLinear470Bg(linear: f32) -> f32 {
    Powf(linear.clamp(0.0, 1.0), 1.0 / 2.8)
}

pub fn ToLinearSmpte240(gamma: f32) -> f32 {
    if gamma < 0.0 {
        0.0
    } else if gamma < 4.0 * 0.022821585529445f32 {
        gamma / 4.0
    } else if gamma < 1.0 {
        Powf((gamma + 0.111572195921731) / 1.111572195921731, 1.0 / 0.45)
    } else {
        1.0
    }
}

pub fn FromLinearSmpte240(linear: f32) -> f32 {
    if linear < 0.0 {
        0.0
    } else if linear < 0.022821585529445 {
        linear * 4.0
    } else if linear < 1.0 {
        1.111572195921731 * Powf(linear, 0.45) - 0.111572195921731
    } else {
        1.0
    }
}

pub fn ToLinearLog100(gamma: f32) -> f32 {
    if gamma <= 0.0 {
        0.01 / 2.0
    } else {
        Powf(10.0, 2.0 * (gamma.min(1.0) - 1.0))
    }
}
pub fn FromLinearLog100(linear: f32) -> f32 {
    if linear < 0.01 {
        0.0
    } else {
        1.0 + Log10f(linear.min(1.0)) / 2.0
    }
}
pub fn ToLinearLog100Sqrt10(gamma: f32) -> f32 {
    if gamma <= 0.0 {
        0.00316227766 / 2.0
    } else {
        Powf(10.0, 2.5 * (gamma.min(1.0) - 1.0))
    }
}
pub fn FromLinearLog100Sqrt10(linear: f32) -> f32 {
    if linear < 0.00316227766 {
        0.0
    } else {
        1.0 + Log10f(linear.min(1.0)) / 2.5
    }
}

pub fn ToLinearIec61966(gamma: f32) -> f32 {
    if gamma <= -4.5 * 0.018053968510807f32 {
        Powf((-gamma + 0.09929682680944) / -1.09929682680944, 1.0 / 0.45)
    } else if gamma < 4.5 * 0.018053968510807f32 {
        gamma / 4.5
    } else {
        Powf((gamma + 0.09929682680944) / 1.09929682680944, 1.0 / 0.45)
    }
}
pub fn FromLinearIec61966(linear: f32) -> f32 {
    if linear <= -0.018053968510807 {
        -1.09929682680944 * Powf(-linear, 0.45) + 0.09929682680944
    } else if linear < 0.018053968510807 {
        linear * 4.5
    } else {
        1.09929682680944 * Powf(linear, 0.45) - 0.09929682680944
    }
}

pub fn ToLinearBt1361(gamma: f32) -> f32 {
    if gamma < -0.25 {
        -0.25
    } else if gamma < 0.0 {
        Powf((gamma - 0.02482420670236) / -0.27482420670236, 1.0 / 0.45) / -4.0
    } else if gamma < 4.5 * 0.018053968510807f32 {
        gamma / 4.5
    } else if gamma < 1.0 {
        Powf((gamma + 0.09929682680944) / 1.09929682680944, 1.0 / 0.45)
    } else {
        1.0
    }
}
pub fn FromLinearBt1361(linear: f32) -> f32 {
    if linear < -0.25 {
        -0.25
    } else if linear < 0.0 {
        -0.27482420670236 * Powf(-4.0 * linear, 0.45) + 0.02482420670236
    } else if linear < 0.018053968510807 {
        linear * 4.5
    } else if linear < 1.0 {
        1.09929682680944 * Powf(linear, 0.45) - 0.09929682680944
    } else {
        1.0
    }
}

pub fn ToLinearPq(gamma: f32) -> f32 {
    if gamma > 0.0 {
        let pow_gamma = Powf(gamma, 32.0 / 2523.0);
        let num = (pow_gamma - 107.0 / 128.0).max(0.0);
        let den = (2413.0 / 128.0 - 2392.0 / 128.0 * pow_gamma).max(f32::MIN_POSITIVE);
        Powf(num / den, 4096.0 / 653.0)
    } else {
        0.0
    }
}
pub fn FromLinearPq(linear: f32) -> f32 {
    if linear > 0.0 {
        let pow_linear = Powf(linear, 653.0 / 4096.0);
        let num = 107.0 / 128.0 + 2413.0 / 128.0 * pow_linear;
        let den = 1.0 + 2392.0 / 128.0 * pow_linear;
        Powf(num / den, 2523.0 / 32.0)
    } else {
        0.0
    }
}
pub fn ToLinearSmpte428(gamma: f32) -> f32 {
    Powf(gamma.max(0.0), 2.6) / 0.91655527974030934
}
pub fn FromLinearSmpte428(linear: f32) -> f32 {
    Powf(0.91655527974030934 * linear.max(0.0), 1.0 / 2.6)
}

pub fn ToLinearHlg(gamma: f32) -> f32 {
    if gamma < 0.0 {
        0.0
    } else if gamma <= 0.5 {
        Powf((gamma * gamma) * (1.0 / 3.0), 1.2)
    } else {
        Powf(
            (((gamma - 0.55991073) / 0.17883277).exp() + 0.28466892) / 12.0,
            1.2,
        )
    }
}
pub fn FromLinearHlg(linear: f32) -> f32 {
    let linear = Powf(linear, 1.0 / 1.2);
    if linear < 0.0 {
        0.0
    } else if linear <= 1.0 / 12.0 {
        (3.0 * linear).sqrt()
    } else {
        0.17883277 * (12.0 * linear - 0.28466892).ln() + 0.55991073
    }
}

/// Gamma samples must fit bit_depth (8..=16).
pub fn SharpYuvGammaToLinear(
    v: u16,
    bit_depth: u32,
    transfer_type: SharpYuvTransferFunctionType,
) -> u32 {
    if transfer_type == kSharpYuvTransferFunctionSrgb {
        return ToLinearSrgb(v, bit_depth);
    }
    let v_float = f32::from(v) / ((1u32 << bit_depth) - 1) as f32;
    let linear = match transfer_type {
        kSharpYuvTransferFunctionBt709
        | kSharpYuvTransferFunctionBt601
        | kSharpYuvTransferFunctionBt2020_10Bit
        | kSharpYuvTransferFunctionBt2020_12Bit => ToLinear709(v_float),
        kSharpYuvTransferFunctionBt470M => ToLinear470M(v_float),
        kSharpYuvTransferFunctionBt470Bg => ToLinear470Bg(v_float),
        kSharpYuvTransferFunctionSmpte240 => ToLinearSmpte240(v_float),
        kSharpYuvTransferFunctionLinear => return u32::from(v),
        kSharpYuvTransferFunctionLog100 => ToLinearLog100(v_float),
        kSharpYuvTransferFunctionLog100_Sqrt10 => ToLinearLog100Sqrt10(v_float),
        kSharpYuvTransferFunctionIec61966 => ToLinearIec61966(v_float),
        kSharpYuvTransferFunctionBt1361 => ToLinearBt1361(v_float),
        kSharpYuvTransferFunctionSmpte2084 => ToLinearPq(v_float),
        kSharpYuvTransferFunctionSmpte428 => ToLinearSmpte428(v_float),
        kSharpYuvTransferFunctionHlg => ToLinearHlg(v_float),
        kSharpYuvTransferFunctionSrgb => unreachable!(),
    };
    Roundf(linear * 65535.0) as u32
}

/// sRGB linear input must be at most 65536; other transfers can exceed this.
pub fn SharpYuvLinearToGamma(
    v: u32,
    bit_depth: u32,
    transfer_type: SharpYuvTransferFunctionType,
) -> u16 {
    if transfer_type == kSharpYuvTransferFunctionSrgb {
        return FromLinearSrgb(v, bit_depth);
    }
    let v_float = v as f32 / 65535.0;
    let linear = match transfer_type {
        kSharpYuvTransferFunctionBt709
        | kSharpYuvTransferFunctionBt601
        | kSharpYuvTransferFunctionBt2020_10Bit
        | kSharpYuvTransferFunctionBt2020_12Bit => FromLinear709(v_float),
        kSharpYuvTransferFunctionBt470M => FromLinear470M(v_float),
        kSharpYuvTransferFunctionBt470Bg => FromLinear470Bg(v_float),
        kSharpYuvTransferFunctionSmpte240 => FromLinearSmpte240(v_float),
        kSharpYuvTransferFunctionLinear => return v as u16,
        kSharpYuvTransferFunctionLog100 => FromLinearLog100(v_float),
        kSharpYuvTransferFunctionLog100_Sqrt10 => FromLinearLog100Sqrt10(v_float),
        kSharpYuvTransferFunctionIec61966 => FromLinearIec61966(v_float),
        kSharpYuvTransferFunctionBt1361 => FromLinearBt1361(v_float),
        kSharpYuvTransferFunctionSmpte2084 => FromLinearPq(v_float),
        kSharpYuvTransferFunctionSmpte428 => FromLinearSmpte428(v_float),
        kSharpYuvTransferFunctionHlg => FromLinearHlg(v_float),
        kSharpYuvTransferFunctionSrgb => unreachable!(),
    };
    // Narrow after rounding, matching C's uint16_t conversion on its valid domain.
    (Roundf(linear * ((1u32 << bit_depth) - 1) as f32) as u32) as u16
}
