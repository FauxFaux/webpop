// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8i_enc.h, encoder foundations.
// See COPYING and PATENTS.

pub use super::token_enc::VP8TBuffer;
use crate::dsp::enc::BPS;

pub const MAX_LEVEL: i32 = 2047;
pub const MAX_VARIABLE_LEVEL: usize = 67;
pub const NUM_TYPES: usize = 4;
pub const NUM_BANDS: usize = 8;
pub const NUM_CTX: usize = 3;
pub const NUM_PROBAS: usize = 11;
pub const QFIX: u32 = 17;
pub const MAX_COST: i64 = 0x7fffffffffffff;

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VP8RDLevel {
    RD_OPT_NONE,
    RD_OPT_BASIC,
    RD_OPT_TRELLIS,
    RD_OPT_TRELLIS_ALL,
}

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

pub const YUV_SIZE_ENC: usize = 16 * BPS;
pub const Y_OFF_ENC: usize = 0;
pub const U_OFF_ENC: usize = 16;
pub const V_OFF_ENC: usize = 24;
pub type DError = [[i8; 2]; 2];
pub const NUM_MB_SEGMENTS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8EncFilterHeader {
    pub simple: bool,
    pub level: i32,
    pub sharpness: i32,
    pub i4x4_lf_delta: i32,
}

impl Default for VP8EncFilterHeader {
    fn default() -> Self {
        Self {
            simple: true,
            level: 0,
            sharpness: 0,
            i4x4_lf_delta: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8EncSegmentHeader {
    pub num_segments: usize,
    pub update_map: bool,
    pub size: i32,
}

impl Default for VP8EncSegmentHeader {
    fn default() -> Self {
        Self {
            num_segments: 4,
            update_map: true,
            size: 0,
        }
    }
}

/// Segment susceptibility and quantizer state. Alpha measures coding
/// susceptibility here, independently of image transparency.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VP8SegmentInfo {
    pub y1: VP8Matrix,
    pub y2: VP8Matrix,
    pub uv: VP8Matrix,
    pub alpha: i32,
    pub beta: i32,
    pub quant: i32,
    pub fstrength: i32,
    pub max_edge: i32,
    pub min_disto: i32,
    pub lambda_i16: i32,
    pub lambda_i4: i32,
    pub lambda_uv: i32,
    pub lambda_mode: i32,
    pub tlambda: i32,
    pub lambda_trellis_i16: i32,
    pub lambda_trellis_i4: i32,
    pub lambda_trellis_uv: i32,
    pub i4_penalty: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VP8MBInfo {
    pub r#type: u8,
    pub uv_mode: u8,
    pub skip: bool,
    pub segment: u8,
    pub alpha: u8,
}

/// Candidate state. Coefficients and scores belong to a trial until its mode
/// wins; storing them here does not commit iterator borders or contexts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VP8ModeScore {
    pub D: i64,
    pub SD: i64,
    pub H: i64,
    pub R: i64,
    pub score: i64,
    pub y_dc_levels: [i16; 16],
    pub y_ac_levels: [[i16; 16]; 16],
    pub uv_levels: [[i16; 16]; 8],
    pub mode_i16: i32,
    pub modes_i4: [u8; 16],
    pub mode_uv: i32,
    pub nz: u32,
    pub derr: [[i8; 3]; 2],
}

/// Frame-persistent encoder foundations. Bitstream state follows in the
/// corresponding port milestones.
/// Prediction storage includes the constant top row and left column; nz[0]
/// is the constant left sentinel, followed by one packed context per column.
pub struct VP8Encoder {
    pub max_i4_header_bits: i32,
    pub mb_header_limit: i64,
    pub rd_opt_level: VP8RDLevel,
    pub use_tokens: bool,
    pub config: super::config_enc::VP8EncoderConfig,
    pub filter_hdr: VP8EncFilterHeader,
    pub base_quant: i32,
    pub dq_y1_dc: i32,
    pub dq_y2_dc: i32,
    pub dq_y2_ac: i32,
    pub dq_uv_dc: i32,
    pub dq_uv_ac: i32,
    pub segment_hdr: VP8EncSegmentHeader,
    pub dqm: [VP8SegmentInfo; NUM_MB_SEGMENTS],
    pub alpha: i32,
    pub uv_alpha: i32,
    pub mb_w: usize,
    pub mb_h: usize,
    pub preds_w: usize,
    pub mb_info: Vec<VP8MBInfo>,
    pub preds: Vec<u8>,
    pub nz: Vec<u32>,
    pub y_top: Vec<u8>,
    pub uv_top: Vec<u8>,
    pub top_derr: Option<Vec<DError>>,
    pub proba: VP8EncProba,
}

impl VP8Encoder {
    pub fn new(
        width: u32,
        height: u32,
        diffusion: bool,
    ) -> Result<Self, crate::webp::encode::LosslessEncodeError> {
        Self::new_with_method(width, height, diffusion, 4)
    }

    /// Initialize method-dependent analysis/quantization state. Frame coding
    /// currently implements method 4; other methods are available for staged
    /// leaf ports and setup comparisons, not complete-file encoding yet.
    /// Resource policy remains normal memory and one coefficient partition.
    pub fn new_with_method(
        width: u32,
        height: u32,
        diffusion: bool,
        method: i32,
    ) -> Result<Self, crate::webp::encode::LosslessEncodeError> {
        use crate::enc::picture_csp_enc::RgbaInputError;
        super::config_enc::validate_vp8_method(method)?;
        if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
            return Err(RgbaInputError::InvalidDimensions.into());
        }
        let (mb_w, mb_h) = (
            (width as usize).div_ceil(16),
            (height as usize).div_ceil(16),
        );
        let preds_w = mb_w
            .checked_mul(4)
            .and_then(|v| v.checked_add(1))
            .ok_or(RgbaInputError::SizeOverflow)?;
        let preds_h = mb_h
            .checked_mul(4)
            .and_then(|v| v.checked_add(1))
            .ok_or(RgbaInputError::SizeOverflow)?;
        let count = mb_w.checked_mul(mb_h).ok_or(RgbaInputError::SizeOverflow)?;
        let preds_size = preds_w
            .checked_mul(preds_h)
            .ok_or(RgbaInputError::SizeOverflow)?;
        let top_size = mb_w.checked_mul(16).ok_or(RgbaInputError::SizeOverflow)?;
        fn allocate<T: Default + Clone>(
            len: usize,
        ) -> Result<Vec<T>, std::collections::TryReserveError> {
            let mut v = Vec::new();
            v.try_reserve_exact(len)?;
            v.resize(len, T::default());
            Ok(v)
        }
        let mut proba = VP8EncProba::default();
        super::tree_enc::VP8DefaultProbas(&mut proba);
        let mut enc = Self {
            max_i4_header_bits: 0,
            mb_header_limit: 0,
            rd_opt_level: VP8RDLevel::RD_OPT_NONE,
            use_tokens: false,
            config: super::config_enc::VP8EncoderConfig {
                method,
                ..Default::default()
            },
            filter_hdr: VP8EncFilterHeader::default(),
            base_quant: 0,
            dq_y1_dc: 0,
            dq_y2_dc: 0,
            dq_y2_ac: 0,
            dq_uv_dc: 0,
            dq_uv_ac: 0,
            segment_hdr: VP8EncSegmentHeader::default(),
            dqm: std::array::from_fn(|_| VP8SegmentInfo::default()),
            alpha: 0,
            uv_alpha: 0,
            mb_w,
            mb_h,
            preds_w,
            mb_info: allocate(count)?,
            preds: allocate(preds_size)?,
            nz: allocate(mb_w + 1)?,
            y_top: allocate(top_size)?,
            uv_top: allocate(top_size)?,
            top_derr: if diffusion {
                Some(allocate(mb_w)?)
            } else {
                None
            },
            proba,
        };
        super::webp_enc::MapConfigToTools(&mut enc);
        Ok(enc)
    }

    pub fn preds_offset(&self, x: usize, y: usize) -> usize {
        (4 * y + 1) * self.preds_w + 4 * x + 1
    }
}

/// Scratch and position without C back-pointers. Functions receive the frame
/// explicitly, and candidate output slots can be swapped with std::mem::swap.
/// Left buffers include the corner at index zero. Chroma V's corner is at 16,
/// matching the DSP prediction layout. i4_top is an offset into i4_boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8EncIterator {
    pub x: usize,
    pub y: usize,
    pub yuv_in: [u8; YUV_SIZE_ENC],
    pub yuv_out: [u8; YUV_SIZE_ENC],
    pub yuv_out2: [u8; YUV_SIZE_ENC],
    pub yuv_p: [u8; PRED_SIZE_ENC],
    pub y_left: [u8; 17],
    pub uv_left: [u8; 25],
    pub i4_boundary: [u8; 37],
    pub i4_top: usize,
    pub i4: usize,
    pub top_nz: [u8; 9],
    pub left_nz: [u8; 9],
    pub bit_count: [[u64; 3]; 4],
    pub luma_bits: u64,
    pub uv_bits: u64,
    pub do_trellis: bool,
    pub count_down: i32,
    pub count_down0: i32,
    pub left_derr: DError,
    pub source_boundary: Option<[u8; 32]>,
}

impl Default for VP8EncIterator {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            yuv_in: [0; YUV_SIZE_ENC],
            yuv_out: [0; YUV_SIZE_ENC],
            yuv_out2: [0; YUV_SIZE_ENC],
            yuv_p: [0; PRED_SIZE_ENC],
            y_left: [0; 17],
            uv_left: [0; 25],
            i4_boundary: [0; 37],
            i4_top: 0,
            i4: 0,
            top_nz: [0; 9],
            left_nz: [0; 9],
            bit_count: [[0; 3]; 4],
            luma_bits: 0,
            uv_bits: 0,
            do_trellis: false,
            count_down: 0,
            count_down0: 0,
            left_derr: [[0; 2]; 2],
            source_boundary: None,
        }
    }
}

impl VP8EncIterator {
    pub fn y_top<'a>(&'a self, enc: &'a VP8Encoder) -> &'a [u8] {
        self.source_boundary
            .as_ref()
            .map_or_else(|| &enc.y_top[16 * self.x..], |v| &v[..])
    }
    pub fn uv_top<'a>(&'a self, enc: &'a VP8Encoder) -> &'a [u8] {
        self.source_boundary
            .as_ref()
            .map_or_else(|| &enc.uv_top[16 * self.x..], |v| &v[16..])
    }
    pub fn i4_window(&self) -> &[u8; 13] {
        self.i4_boundary[self.i4_top - 5..self.i4_top + 8]
            .try_into()
            .unwrap()
    }
}
