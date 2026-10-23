// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/token_enc.c. See COPYING and PATENTS.

use crate::{
    dsp::cost::VP8EncBands,
    enc::{cost_enc::*, frame_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};
use std::collections::TryReserveError;

const MIN_PAGE_SIZE: usize = 8192;
const FIXED_PROBA_BIT: u16 = 1 << 14;

/// Paginated coefficient tokens. Pages store tokens in emission order rather
/// than C's reverse-indexed slots. Allocation errors propagate immediately;
/// callers must abandon the recording on error. Dropping the buffer frees it.
#[derive(Debug)]
pub struct VP8TBuffer {
    pages: Vec<Vec<u16>>,
    page_size: usize,
    len: usize,
}
impl Default for VP8TBuffer {
    fn default() -> Self {
        VP8TBufferInit(0)
    }
}
impl VP8TBuffer {
    pub fn page_size(&self) -> usize {
        self.page_size
    }
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn left(&self) -> usize {
        self.pages.last().map_or(0, |p| self.page_size - p.len())
    }
    pub fn tokens(&self) -> impl Iterator<Item = u16> + '_ {
        self.pages.iter().flatten().copied()
    }
}

pub fn VP8TBufferInit(page_size: usize) -> VP8TBuffer {
    VP8TBuffer {
        pages: Vec::new(),
        page_size: page_size.max(MIN_PAGE_SIZE),
        len: 0,
    }
}

pub fn VP8TBufferClear(b: &mut VP8TBuffer) {
    b.pages = Vec::new();
    b.len = 0;
}

fn TBufferNewPage(b: &mut VP8TBuffer) -> Result<(), TryReserveError> {
    b.pages.try_reserve(1)?;
    let mut page = Vec::new();
    page.try_reserve_exact(b.page_size)?;
    b.pages.push(page);
    Ok(())
}

fn push_token(b: &mut VP8TBuffer, token: u16) -> Result<(), TryReserveError> {
    if b.pages.last().is_none_or(|p| p.len() == b.page_size) {
        TBufferNewPage(b)?;
    }
    b.pages.last_mut().unwrap().push(token);
    b.len += 1;
    Ok(())
}

#[inline]
fn AddToken(
    b: &mut VP8TBuffer,
    bit: bool,
    proba_idx: usize,
    stats: &mut proba_t,
) -> Result<bool, TryReserveError> {
    assert!(proba_idx < usize::from(FIXED_PROBA_BIT));
    push_token(b, (u16::from(bit) << 15) | proba_idx as u16)?;
    VP8RecordStats(u32::from(bit), stats);
    Ok(bit)
}

#[inline]
fn AddConstantToken(b: &mut VP8TBuffer, bit: bool, proba: u8) -> Result<(), TryReserveError> {
    push_token(
        b,
        (u16::from(bit) << 15) | FIXED_PROBA_BIT | u16::from(proba),
    )
}

// Upstream TOKEN_ID macro, expressed as an inline index helper.
#[inline]
fn TOKEN_ID(t: usize, band: usize, ctx: usize) -> usize {
    NUM_PROBAS * (ctx + NUM_CTX * (band + NUM_BANDS * t))
}

pub fn VP8RecordCoeffTokens(
    ctx: usize,
    res: &VP8Residual<'_>,
    tokens: &mut VP8TBuffer,
    proba: &mut VP8EncProba,
) -> Result<bool, TryReserveError> {
    assert!(ctx < NUM_CTX);
    let coeff_type = res.coeff_type;
    let stats = &mut proba.stats[coeff_type];
    let mut n = res.first;
    let (mut band, mut context) = (n, ctx);
    let mut base_id = TOKEN_ID(coeff_type, band, context);
    if !AddToken(tokens, res.last >= 0, base_id, &mut stats[band][context][0])? {
        return Ok(false);
    }
    while n < 16 {
        let c = res.coeffs[n];
        n += 1;
        let v = u32::from(c.unsigned_abs());
        if !AddToken(tokens, v != 0, base_id + 1, &mut stats[band][context][1])? {
            band = VP8EncBands[n];
            context = 0;
            base_id = TOKEN_ID(coeff_type, band, context);
            continue;
        }
        if !AddToken(tokens, v > 1, base_id + 2, &mut stats[band][context][2])? {
            context = 1;
        } else {
            if !AddToken(tokens, v > 4, base_id + 3, &mut stats[band][context][3])? {
                if AddToken(tokens, v != 2, base_id + 4, &mut stats[band][context][4])? {
                    AddToken(tokens, v == 4, base_id + 5, &mut stats[band][context][5])?;
                }
            } else if !AddToken(tokens, v > 10, base_id + 6, &mut stats[band][context][6])? {
                if !AddToken(tokens, v > 6, base_id + 7, &mut stats[band][context][7])? {
                    AddConstantToken(tokens, v == 6, 159)?;
                } else {
                    AddConstantToken(tokens, v >= 9, 165)?;
                    AddConstantToken(tokens, v & 1 == 0, 145)?;
                }
            } else {
                let mut residue = v - 3;
                let tab: &[u8];
                if residue < 8 << 1 {
                    AddToken(tokens, false, base_id + 8, &mut stats[band][context][8])?;
                    AddToken(tokens, false, base_id + 9, &mut stats[band][context][9])?;
                    residue -= 8;
                    tab = &VP8Cat3;
                } else if residue < 8 << 2 {
                    AddToken(tokens, false, base_id + 8, &mut stats[band][context][8])?;
                    AddToken(tokens, true, base_id + 9, &mut stats[band][context][9])?;
                    residue -= 8 << 1;
                    tab = &VP8Cat4;
                } else if residue < 8 << 3 {
                    AddToken(tokens, true, base_id + 8, &mut stats[band][context][8])?;
                    AddToken(tokens, false, base_id + 10, &mut stats[band][context][10])?;
                    residue -= 8 << 2;
                    tab = &VP8Cat5;
                } else {
                    AddToken(tokens, true, base_id + 8, &mut stats[band][context][8])?;
                    AddToken(tokens, true, base_id + 10, &mut stats[band][context][10])?;
                    residue -= 8 << 3;
                    tab = &VP8Cat6;
                }
                for (i, &p) in tab.iter().enumerate() {
                    AddConstantToken(tokens, residue & (1 << (tab.len() - 1 - i)) != 0, p)?;
                }
            }
            context = 2;
        }
        band = VP8EncBands[n];
        base_id = TOKEN_ID(coeff_type, band, context);
        AddConstantToken(tokens, c < 0, 128)?;
        if n == 16
            || !AddToken(
                tokens,
                n as i32 <= res.last,
                base_id,
                &mut stats[band][context][0],
            )?
        {
            return Ok(true);
        }
    }
    Ok(true)
}

#[inline]
fn TokenProbability(token: u16, probas: &[u8]) -> u8 {
    if token & FIXED_PROBA_BIT != 0 {
        token as u8
    } else {
        probas[usize::from(token & 0x3fff)]
    }
}

/// Trial emissions retain all tokens. A successful final emission releases all
/// pages and resets cursors; clear/reuse is safe without C's dangling cursors.
pub fn VP8EmitTokens(
    b: &mut VP8TBuffer,
    bw: &mut VP8BitWriter,
    probas: &[u8],
    final_pass: bool,
) -> Result<(), TryReserveError> {
    for token in b.tokens() {
        VP8PutBit(bw, i32::from(token >> 15), TokenProbability(token, probas))?;
    }
    if final_pass {
        VP8TBufferClear(b);
    }
    Ok(())
}

pub fn VP8EstimateTokenSize(b: &VP8TBuffer, probas: &[u8]) -> usize {
    b.tokens().fold(0usize, |size, token| {
        size.wrapping_add(
            VP8BitCost(i32::from(token >> 15), TokenProbability(token, probas)) as usize,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_failure_keeps_token_and_counter_state() {
        let mut b = VP8TBufferInit(usize::MAX);
        let mut stats = 123;
        assert!(AddToken(&mut b, true, 0, &mut stats).is_err());
        assert_eq!(stats, 123);
        assert!(b.is_empty());
        assert_eq!(b.page_count(), 0);
        VP8TBufferClear(&mut b);
        assert_eq!(b.page_size(), usize::MAX);
    }
}
