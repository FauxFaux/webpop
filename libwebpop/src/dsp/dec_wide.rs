// Copyright 2010 Google Inc. All Rights Reserved.
// Portable counterpart of TransformWHT in upstream/src/dsp/dec_{sse2,neon}.c.
// See COPYING and PATENTS.

use wide::{i32x4, u32x4};

#[inline]
fn pass([v0, v1, v2, v3]: [i32x4; 4]) -> [i32x4; 4] {
    let a0 = v0 + v3;
    let a1 = v1 + v2;
    let a2 = v1 - v2;
    let a3 = v0 - v3;
    [a0 + a1, a3 + a2, a0 - a1, a3 - a2]
}

/// Inverse WHT; scatter DC slots without touching AC coefficients or padding.
pub fn TransformWHT_wide(input: &[i16; 16], out: &mut [i16]) {
    assert!(out.len() >= 241);
    let rows =
        std::array::from_fn(|y| i32x4::from(std::array::from_fn(|x| i32::from(input[4 * y + x]))));
    let mut columns: [i32x4; 4] =
        wide::bytemuck::cast(u32x4::transpose(wide::bytemuck::cast(pass(rows))));
    columns[0] += i32x4::splat(3);
    let rows: [i32x4; 4] =
        wide::bytemuck::cast(u32x4::transpose(wide::bytemuck::cast(pass(columns))));
    for (y, row) in rows.into_iter().enumerate() {
        for (x, value) in row.to_array().into_iter().enumerate() {
            out[64 * y + 16 * x] = (value >> 3) as i16;
        }
    }
}
