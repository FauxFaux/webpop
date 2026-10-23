// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/lossless_enc.c, functions of the same names.
// See COPYING and PATENTS.

// Retain upstream dispatch/table names.
#![allow(non_upper_case_globals)]

use super::lossless::{VP8LBitEntropy, VP8LMultipliers, VP8LPredictors, VP8LStreaks};
use super::lossless_common::{VP8LFastSLog2, VP8LSubPixels};
use crate::utils::utils::BitsLog2Floor;

pub fn VP8LSubtractGreenFromBlueAndRed_C(argb_data: &mut [u32]) {
    for argb in argb_data {
        let green = (*argb >> 8) as u8;
        let new_r = (*argb >> 16) as u8;
        let new_b = *argb as u8;
        *argb = (*argb & 0xff00ff00)
            | (u32::from(new_r.wrapping_sub(green)) << 16)
            | u32::from(new_b.wrapping_sub(green));
    }
}

#[inline]
fn ColorTransformDelta(color_pred: i8, color: i8) -> i32 {
    (i32::from(color_pred) * i32::from(color)) >> 5
}

#[inline]
fn U32ToS8(v: u32) -> i8 {
    v as u8 as i8
}

pub fn VP8LTransformColor_C(m: &VP8LMultipliers, data: &mut [u32]) {
    for argb in data {
        let green = U32ToS8(*argb >> 8);
        let red = U32ToS8(*argb >> 16);
        let new_red =
            ((i32::from(red) & 0xff) - ColorTransformDelta(m.green_to_red as i8, green)) & 0xff;
        // The blue prediction uses the original red, before its transform.
        let new_blue = ((*argb & 0xff) as i32
            - ColorTransformDelta(m.green_to_blue as i8, green)
            - ColorTransformDelta(m.red_to_blue as i8, red))
            & 0xff;
        *argb = (*argb & 0xff00ff00) | ((new_red as u32) << 16) | new_blue as u32;
    }
}

// Copy the pinned integer tables verbatim; do not regenerate with host floats.
pub const kLog2Table: [u32; 256] = [
    0, 0, 8388608, 13295629, 16777216, 19477745, 21684237, 23549800, 25165824, 26591258, 27866353,
    29019816, 30072845, 31041538, 31938408, 32773374, 33554432, 34288123, 34979866, 35634199,
    36254961, 36845429, 37408424, 37946388, 38461453, 38955489, 39430146, 39886887, 40327016,
    40751698, 41161982, 41558811, 41943040, 42315445, 42676731, 43027545, 43368474, 43700062,
    44022807, 44337167, 44643569, 44942404, 45234037, 45518808, 45797032, 46069003, 46334996,
    46595268, 46850061, 47099600, 47344097, 47583753, 47818754, 48049279, 48275495, 48497560,
    48715624, 48929828, 49140306, 49347187, 49550590, 49750631, 49947419, 50141058, 50331648,
    50519283, 50704053, 50886044, 51065339, 51242017, 51416153, 51587818, 51757082, 51924012,
    52088670, 52251118, 52411415, 52569616, 52725775, 52879946, 53032177, 53182516, 53331012,
    53477707, 53622645, 53765868, 53907416, 54047327, 54185640, 54322389, 54457611, 54591338,
    54723604, 54854440, 54983876, 55111943, 55238669, 55364082, 55488208, 55611074, 55732705,
    55853126, 55972361, 56090432, 56207362, 56323174, 56437887, 56551524, 56664103, 56775645,
    56886168, 56995691, 57104232, 57211808, 57318436, 57424133, 57528914, 57632796, 57735795,
    57837923, 57939198, 58039632, 58139239, 58238033, 58336027, 58433234, 58529666, 58625336,
    58720256, 58814437, 58907891, 59000628, 59092661, 59183999, 59274652, 59364632, 59453947,
    59542609, 59630625, 59718006, 59804761, 59890898, 59976426, 60061354, 60145690, 60229443,
    60312620, 60395229, 60477278, 60558775, 60639726, 60720140, 60800023, 60879382, 60958224,
    61036555, 61114383, 61191714, 61268554, 61344908, 61420785, 61496188, 61571124, 61645600,
    61719620, 61793189, 61866315, 61939001, 62011253, 62083076, 62154476, 62225457, 62296024,
    62366182, 62435935, 62505289, 62574248, 62642816, 62710997, 62778797, 62846219, 62913267,
    62979946, 63046260, 63112212, 63177807, 63243048, 63307939, 63372484, 63436687, 63500551,
    63564080, 63627277, 63690146, 63752690, 63814912, 63876816, 63938405, 63999682, 64060650,
    64121313, 64181673, 64241734, 64301498, 64360969, 64420148, 64479040, 64537646, 64595970,
    64654014, 64711782, 64769274, 64826495, 64883447, 64940132, 64996553, 65052711, 65108611,
    65164253, 65219641, 65274776, 65329662, 65384299, 65438691, 65492840, 65546747, 65600416,
    65653847, 65707044, 65760008, 65812741, 65865245, 65917522, 65969575, 66021404, 66073013,
    66124403, 66175575, 66226531, 66277275, 66327806, 66378127, 66428240, 66478146, 66527847,
    66577345, 66626641, 66675737, 66724635, 66773336, 66821842, 66870154, 66918274, 66966204,
    67013944, 67061497,
];
pub const kSLog2Table: [u64; 256] = [
    0,
    0,
    16777216,
    39886887,
    67108864,
    97388723,
    130105423,
    164848600,
    201326592,
    239321324,
    278663526,
    319217973,
    360874141,
    403539997,
    447137711,
    491600606,
    536870912,
    582898099,
    629637592,
    677049776,
    725099212,
    773754010,
    822985323,
    872766924,
    923074875,
    973887230,
    1025183802,
    1076945958,
    1129156447,
    1181799249,
    1234859451,
    1288323135,
    1342177280,
    1396409681,
    1451008871,
    1505964059,
    1561265072,
    1616902301,
    1672866655,
    1729149526,
    1785742744,
    1842638548,
    1899829557,
    1957308741,
    2015069397,
    2073105127,
    2131409817,
    2189977618,
    2248802933,
    2307880396,
    2367204859,
    2426771383,
    2486575220,
    2546611805,
    2606876748,
    2667365819,
    2728074942,
    2789000187,
    2850137762,
    2911484006,
    2973035382,
    3034788471,
    3096739966,
    3158886666,
    3221225472,
    3283753383,
    3346467489,
    3409364969,
    3472443085,
    3535699182,
    3599130679,
    3662735070,
    3726509920,
    3790452862,
    3854561593,
    3918833872,
    3983267519,
    4047860410,
    4112610476,
    4177515704,
    4242574127,
    4307783833,
    4373142952,
    4438649662,
    4504302186,
    4570098787,
    4636037770,
    4702117480,
    4768336298,
    4834692645,
    4901184974,
    4967811774,
    5034571569,
    5101462912,
    5168484389,
    5235634615,
    5302912235,
    5370315922,
    5437844376,
    5505496324,
    5573270518,
    5641165737,
    5709180782,
    5777314477,
    5845565671,
    5913933235,
    5982416059,
    6051013057,
    6119723161,
    6188545324,
    6257478518,
    6326521733,
    6395673979,
    6464934282,
    6534301685,
    6603775250,
    6673354052,
    6743037185,
    6812823756,
    6882712890,
    6952703725,
    7022795412,
    7092987118,
    7163278025,
    7233667324,
    7304154222,
    7374737939,
    7445417707,
    7516192768,
    7587062379,
    7658025806,
    7729082328,
    7800231234,
    7871471825,
    7942803410,
    8014225311,
    8085736859,
    8157337394,
    8229026267,
    8300802839,
    8372666477,
    8444616560,
    8516652476,
    8588773618,
    8660979393,
    8733269211,
    8805642493,
    8878098667,
    8950637170,
    9023257446,
    9095958945,
    9168741125,
    9241603454,
    9314545403,
    9387566451,
    9460666086,
    9533843800,
    9607099093,
    9680431471,
    9753840445,
    9827325535,
    9900886263,
    9974522161,
    10048232765,
    10122017615,
    10195876260,
    10269808253,
    10343813150,
    10417890516,
    10492039919,
    10566260934,
    10640553138,
    10714916116,
    10789349456,
    10863852751,
    10938425600,
    11013067604,
    11087778372,
    11162557513,
    11237404645,
    11312319387,
    11387301364,
    11462350205,
    11537465541,
    11612647010,
    11687894253,
    11763206912,
    11838584638,
    11914027082,
    11989533899,
    12065104750,
    12140739296,
    12216437206,
    12292198148,
    12368021795,
    12443907826,
    12519855920,
    12595865759,
    12671937032,
    12748069427,
    12824262637,
    12900516358,
    12976830290,
    13053204134,
    13129637595,
    13206130381,
    13282682202,
    13359292772,
    13435961806,
    13512689025,
    13589474149,
    13666316903,
    13743217014,
    13820174211,
    13897188225,
    13974258793,
    14051385649,
    14128568535,
    14205807192,
    14283101363,
    14360450796,
    14437855239,
    14515314443,
    14592828162,
    14670396151,
    14748018167,
    14825693972,
    14903423326,
    14981205995,
    15059041743,
    15136930339,
    15214871554,
    15292865160,
    15370910930,
    15449008641,
    15527158071,
    15605359001,
    15683611210,
    15761914485,
    15840268608,
    15918673369,
    15997128556,
    16075633960,
    16154189373,
    16232794589,
    16311449405,
    16390153617,
    16468907026,
    16547709431,
    16626560636,
    16705460444,
    16784408661,
    16863405094,
    16942449552,
    17021541845,
    17100681785,
];

pub fn FastSLog2Slow_C(mut v: u32) -> u64 {
    use super::lossless_common::*;
    assert!(v >= LOG_LOOKUP_IDX_MAX);
    if v < APPROX_LOG_WITH_CORRECTION_MAX {
        let orig_v = u64::from(v);
        let log_cnt = BitsLog2Floor(v) - 7;
        let y = 1_u32 << log_cnt;
        v >>= log_cnt;
        let correction = LOG_2_RECIPROCAL_FIXED * (orig_v & u64::from(y - 1));
        orig_v * (u64::from(kLog2Table[v as usize]) + (u64::from(log_cnt) << LOG_2_PRECISION_BITS))
            + correction
    } else {
        (LOG_2_RECIPROCAL_FIXED_DOUBLE * f64::from(v) * f64::from(v).ln() + 0.5) as u64
    }
}

pub fn FastLog2Slow_C(mut v: u32) -> u32 {
    use super::lossless_common::*;
    assert!(v >= LOG_LOOKUP_IDX_MAX);
    if v < APPROX_LOG_WITH_CORRECTION_MAX {
        let orig_v = v;
        let log_cnt = BitsLog2Floor(v) - 7;
        let y = 1_u32 << log_cnt;
        v >>= log_cnt;
        let mut log_2 = kLog2Table[v as usize] + (log_cnt << LOG_2_PRECISION_BITS);
        if orig_v >= APPROX_LOG_MAX {
            let correction = LOG_2_RECIPROCAL_FIXED * u64::from(orig_v & (y - 1));
            log_2 += DivRound(correction as i64, i64::from(orig_v)) as u32;
        }
        log_2
    } else {
        (LOG_2_RECIPROCAL_FIXED_DOUBLE * f64::from(v).ln() + 0.5) as u32
    }
}

/// Fixed-point Shannon entropy. Histogram populations must fit the upstream
/// image domain (at most 2^28 pixels total).
pub fn ShannonEntropy_C(X: &[u32]) -> u64 {
    let mut retval = 0_u64;
    let mut sumX = 0_u32;
    for &x in X {
        sumX = sumX.wrapping_add(x);
        retval += VP8LFastSLog2(x);
    }
    VP8LFastSLog2(sumX).wrapping_sub(retval)
}

/// Sum of the entropies of X and X+Y, rather than just X+Y.
pub fn CombinedShannonEntropy_C(X: &[u32; 256], Y: &[u32; 256]) -> u64 {
    let mut sumX = 0_u32;
    let mut sumXY = 0_u32;
    let mut retval = 0_u64;
    for (&x, &y) in X.iter().zip(Y) {
        let xy = x.wrapping_add(y);
        sumX = sumX.wrapping_add(x);
        sumXY = sumXY.wrapping_add(xy);
        retval += VP8LFastSLog2(x) + VP8LFastSLog2(xy);
    }
    (VP8LFastSLog2(sumX) + VP8LFastSLog2(sumXY)).wrapping_sub(retval)
}

pub fn VP8LBitEntropyInit(entropy: &mut VP8LBitEntropy) {
    *entropy = VP8LBitEntropy::default();
}

pub fn VP8LBitsEntropyUnrefined(array: &[u32], entropy: &mut VP8LBitEntropy) {
    VP8LBitEntropyInit(entropy);
    for (i, &val) in array.iter().enumerate() {
        if val != 0 {
            entropy.sum = entropy.sum.wrapping_add(val);
            entropy.nonzero_code = i as u32;
            entropy.nonzeros += 1;
            entropy.entropy += VP8LFastSLog2(val);
            entropy.max_val = entropy.max_val.max(val);
        }
    }
    entropy.entropy = VP8LFastSLog2(entropy.sum).wrapping_sub(entropy.entropy);
}

#[inline]
fn GetEntropyUnrefinedHelper(
    val_prev: u32,
    i: usize,
    i_prev: usize,
    bit_entropy: &mut VP8LBitEntropy,
    stats: &mut VP8LStreaks,
) {
    let streak = i - i_prev;
    let nonzero = usize::from(val_prev != 0);
    if val_prev != 0 {
        bit_entropy.sum = bit_entropy
            .sum
            .wrapping_add(val_prev.wrapping_mul(streak as u32));
        bit_entropy.nonzeros += streak as i32;
        // Preserve the start of the last nonzero run (not its last index).
        bit_entropy.nonzero_code = i_prev as u32;
        bit_entropy.entropy += VP8LFastSLog2(val_prev) * streak as u64;
        bit_entropy.max_val = bit_entropy.max_val.max(val_prev);
    }
    stats.counts[nonzero] += i32::from(streak > 3);
    stats.streaks[nonzero][usize::from(streak > 3)] += streak as i32;
}

pub fn GetEntropyUnrefined_C(X: &[u32], bit_entropy: &mut VP8LBitEntropy, stats: &mut VP8LStreaks) {
    assert!(!X.is_empty());
    *stats = VP8LStreaks::default();
    VP8LBitEntropyInit(bit_entropy);
    let mut i_prev = 0;
    let mut x_prev = X[0];
    for (i, &x) in X.iter().enumerate().skip(1) {
        if x != x_prev {
            GetEntropyUnrefinedHelper(x_prev, i, i_prev, bit_entropy, stats);
            x_prev = x;
            i_prev = i;
        }
    }
    GetEntropyUnrefinedHelper(x_prev, X.len(), i_prev, bit_entropy, stats);
    bit_entropy.entropy = VP8LFastSLog2(bit_entropy.sum).wrapping_sub(bit_entropy.entropy);
}

pub fn GetCombinedEntropyUnrefined_C(
    X: &[u32],
    Y: &[u32],
    bit_entropy: &mut VP8LBitEntropy,
    stats: &mut VP8LStreaks,
) {
    assert_eq!(X.len(), Y.len());
    assert!(!X.is_empty());
    *stats = VP8LStreaks::default();
    VP8LBitEntropyInit(bit_entropy);
    let mut i_prev = 0;
    let mut xy_prev = X[0].wrapping_add(Y[0]);
    for i in 1..X.len() {
        let xy = X[i].wrapping_add(Y[i]);
        if xy != xy_prev {
            GetEntropyUnrefinedHelper(xy_prev, i, i_prev, bit_entropy, stats);
            xy_prev = xy;
            i_prev = i;
        }
    }
    GetEntropyUnrefinedHelper(xy_prev, X.len(), i_prev, bit_entropy, stats);
    bit_entropy.entropy = VP8LFastSLog2(bit_entropy.sum).wrapping_sub(bit_entropy.entropy);
}

pub fn ExtraCost_C(population: &[u32]) -> u32 {
    assert!(population.len() >= 6 && population.len().is_multiple_of(2));
    let mut cost = population[4].wrapping_add(population[5]);
    for i in 2..population.len() / 2 - 1 {
        cost = cost.wrapping_add(
            (i as u32).wrapping_mul(population[2 * i + 2].wrapping_add(population[2 * i + 3])),
        );
    }
    cost
}

pub fn AddVector_C(a: &[u32], b: &[u32], out: &mut [u32]) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), out.len());
    for ((&a, &b), out) in a.iter().zip(b).zip(out) {
        *out = a.wrapping_add(b);
    }
}
pub fn AddVectorEq_C(a: &[u32], out: &mut [u32]) {
    assert_eq!(a.len(), out.len());
    for (&a, out) in a.iter().zip(out) {
        *out = out.wrapping_add(a);
    }
}
pub fn VectorMismatch_C(array1: &[u32], array2: &[u32]) -> usize {
    assert_eq!(array1.len(), array2.len());
    array1
        .iter()
        .zip(array2)
        .position(|(a, b)| a != b)
        .unwrap_or(array1.len())
}

pub fn VP8LBundleColorMap_C(row: &[u8], xbits: u32, dst: &mut [u32]) {
    assert!(xbits <= 3);
    assert_eq!(dst.len(), row.len().div_ceil(1 << xbits));
    let bit_depth = 1 << (3 - xbits);
    let mask = (1 << xbits) - 1;
    let mut code = 0xff000000;
    for (x, &pixel) in row.iter().enumerate() {
        let xsub = x & mask;
        if xsub == 0 {
            code = 0xff000000;
        }
        code |= u32::from(pixel) << (8 + bit_depth * xsub);
        dst[x >> xbits] = code;
    }
}

#[inline]
fn TransformColorRed(green_to_red: u8, argb: u32) -> u8 {
    ((argb >> 16) as i32 - ColorTransformDelta(green_to_red as i8, U32ToS8(argb >> 8))) as u8
}
#[inline]
fn TransformColorBlue(green_to_blue: u8, red_to_blue: u8, argb: u32) -> u8 {
    ((argb & 255) as i32
        - ColorTransformDelta(green_to_blue as i8, U32ToS8(argb >> 8))
        - ColorTransformDelta(red_to_blue as i8, U32ToS8(argb >> 16))) as u8
}

// Validate the tile's last row with checked offsets, allowing omitted padding
// after that row. Stride is measured in pixels.
fn CheckTile(argb: &[u32], stride: usize, width: usize, height: usize) {
    assert!(stride >= width);
    let required = if height == 0 || width == 0 {
        0
    } else {
        (height - 1)
            .checked_mul(stride)
            .and_then(|v| v.checked_add(width))
            .expect("tile size overflow")
    };
    assert!(argb.len() >= required);
}
pub fn VP8LCollectColorRedTransforms_C(
    argb: &[u32],
    stride: usize,
    tile_width: usize,
    tile_height: usize,
    green_to_red: i32,
    histo: &mut [u32; 256],
) {
    CheckTile(argb, stride, tile_width, tile_height);
    if tile_width == 0 {
        return;
    }
    for y in 0..tile_height {
        for x in 0..tile_width {
            let bin = TransformColorRed(green_to_red as u8, argb[y * stride + x]) as usize;
            histo[bin] = histo[bin].wrapping_add(1);
        }
    }
}
pub fn VP8LCollectColorBlueTransforms_C(
    argb: &[u32],
    stride: usize,
    tile_width: usize,
    tile_height: usize,
    green_to_blue: i32,
    red_to_blue: i32,
    histo: &mut [u32; 256],
) {
    CheckTile(argb, stride, tile_width, tile_height);
    if tile_width == 0 {
        return;
    }
    for y in 0..tile_height {
        for x in 0..tile_width {
            let bin =
                TransformColorBlue(green_to_blue as u8, red_to_blue as u8, argb[y * stride + x])
                    as usize;
            histo[bin] = histo[bin].wrapping_add(1);
        }
    }
}

/// Input includes the preceding pixel at index 0; upper includes top-left at
/// index 0 and top-right after the final pixel. out holds just the residuals.
/// At a row's right edge the caller supplies the next row's first pixel as
/// top-right, matching upstream's contiguous image convention.
fn PredictorSub(mode: usize, input: &[u32], upper: &[u32], out: &mut [u32]) {
    assert!(input.len() > out.len());
    if mode >= 2 {
        assert!(upper.len() >= out.len() + 2);
    }
    for (x, residual) in out.iter_mut().enumerate() {
        let top = if mode >= 2 {
            [upper[x], upper[x + 1], upper[x + 2]]
        } else {
            [0; 3]
        };
        *residual = VP8LSubPixels(input[x + 1], VP8LPredictors[mode](input[x], &top));
    }
}
pub type VP8LPredictorSubFunc = fn(&[u32], &[u32], &mut [u32]);

pub fn PredictorSub0_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(0, input, upper, out);
}

pub fn PredictorSub1_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(1, input, upper, out);
}

pub fn PredictorSub2_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(2, input, upper, out);
}

pub fn PredictorSub3_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(3, input, upper, out);
}

pub fn PredictorSub4_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(4, input, upper, out);
}

pub fn PredictorSub5_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(5, input, upper, out);
}

pub fn PredictorSub6_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(6, input, upper, out);
}

pub fn PredictorSub7_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(7, input, upper, out);
}

pub fn PredictorSub8_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(8, input, upper, out);
}

pub fn PredictorSub9_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(9, input, upper, out);
}

pub fn PredictorSub10_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(10, input, upper, out);
}

pub fn PredictorSub11_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(11, input, upper, out);
}

pub fn PredictorSub12_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(12, input, upper, out);
}

pub fn PredictorSub13_C(input: &[u32], upper: &[u32], out: &mut [u32]) {
    PredictorSub(13, input, upper, out);
}

pub const VP8LPredictorsSub: [VP8LPredictorSubFunc; 14] = [
    PredictorSub0_C,
    PredictorSub1_C,
    PredictorSub2_C,
    PredictorSub3_C,
    PredictorSub4_C,
    PredictorSub5_C,
    PredictorSub6_C,
    PredictorSub7_C,
    PredictorSub8_C,
    PredictorSub9_C,
    PredictorSub10_C,
    PredictorSub11_C,
    PredictorSub12_C,
    PredictorSub13_C,
];

// Scalar bindings replace upstream's mutable DSP dispatch globals.
pub use AddVector_C as VP8LAddVector;
pub use AddVectorEq_C as VP8LAddVectorEq;
pub use CombinedShannonEntropy_C as VP8LCombinedShannonEntropy;
pub use ExtraCost_C as VP8LExtraCost;
pub use FastLog2Slow_C as VP8LFastLog2Slow;
pub use FastSLog2Slow_C as VP8LFastSLog2Slow;
pub use GetCombinedEntropyUnrefined_C as VP8LGetCombinedEntropyUnrefined;
pub use GetEntropyUnrefined_C as VP8LGetEntropyUnrefined;
pub use ShannonEntropy_C as VP8LShannonEntropy;
pub use VP8LBundleColorMap_C as VP8LBundleColorMap;
pub use VP8LCollectColorBlueTransforms_C as VP8LCollectColorBlueTransforms;
pub use VP8LCollectColorRedTransforms_C as VP8LCollectColorRedTransforms;
pub use VP8LSubtractGreenFromBlueAndRed_C as VP8LSubtractGreenFromBlueAndRed;
pub use VP8LTransformColor_C as VP8LTransformColor;
pub use VectorMismatch_C as VP8LVectorMismatch;
