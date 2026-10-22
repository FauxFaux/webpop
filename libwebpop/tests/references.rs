mod support;

use libwebpop::dsp::lossless_common::LOG_2_PRECISION_BITS;
use libwebpop::enc::{backward_references_enc::*, histogram_enc::*};
use libwebpop::utils::color_cache_utils::*;

#[test]
fn hand_built_tokens_accumulate_without_reset() {
    let mut refs = VP8LBackwardRefsInit(1);
    for v in [
        PixOrCopyCreateLiteral(0x12345678),
        PixOrCopyCreateLiteral(0xff345600),
        PixOrCopyCreateCacheIdx(7),
        PixOrCopyCreateCopy(17, 5),
    ] {
        VP8LBackwardRefsCursorAdd(&mut refs, v).unwrap();
    }
    let mut h = VP8LAllocateHistogram(3).unwrap();
    VP8LHistogramStoreRefs(&refs, None, 0, &mut h);
    assert_eq!(h.literal[0x56], 2);
    assert_eq!(h.literal[256 + 4], 1); // length 5: prefix 4, one extra bit
    assert_eq!(h.literal[256 + 24 + 7], 1);
    assert_eq!(h.red[0x34], 2);
    assert_eq!(h.blue[0x78], 1);
    assert_eq!(h.blue[0], 1);
    assert_eq!(h.alpha[0x12], 1);
    assert_eq!(h.alpha[0xff], 1);
    assert_eq!(h.distance[8], 1); // distance 17: prefix 8, three extra bits
    for i in 0..5 {
        assert_eq!(
            GetPopulationInfo(&h, i).iter().sum::<u32>(),
            [4, 2, 2, 2, 1][i]
        );
    }
    ComputeHistogramCost(&mut h);
    assert_eq!(h.trivial_symbol[1], 0x34);
    assert_eq!(h.trivial_symbol[4], 8);
    assert_eq!(
        VP8LHistogramEstimateBits(&h),
        h.bit_cost + (4 << LOG_2_PRECISION_BITS)
    );
    VP8LHistogramStoreRefs(&refs, None, 0, &mut h);
    assert_eq!(h.literal[0x56], 4);
    assert_eq!(h.distance[8], 2);
    VP8LHistogramCreate(&mut h, &refs, None).unwrap();
    assert_eq!(h.literal[0x56], 2);
    assert_eq!(h.distance[8], 1);
    assert_eq!(h.bit_cost, 0);
}

#[test]
fn modified_distance_matches_converted_refs() {
    let mut raw = VP8LBackwardRefsInit(256);
    for distance in 1..2048 {
        VP8LBackwardRefsCursorAdd(&mut raw, PixOrCopyCreateCopy(distance, 4095)).unwrap();
    }
    let mut converted = VP8LBackwardRefsInit(256);
    BackwardRefsClone(&raw, &mut converted).unwrap();
    BackwardReferences2DLocality(17, &mut converted);
    let mut before = VP8LAllocateHistogram(0).unwrap();
    let mut after = VP8LAllocateHistogram(0).unwrap();
    VP8LHistogramStoreRefs(&raw, Some(VP8LDistanceToPlaneCode), 17, &mut before);
    VP8LHistogramStoreRefs(&converted, None, 0, &mut after);
    assert_eq!(before, after);
    assert_eq!(VP8LDistanceToPlaneCode(17, 17), 1);
    assert_eq!(VP8LDistanceToPlaneCode(17, 1), 2);
}

#[test]
fn reference_storage_reuses_and_swaps_empty_and_nonempty_trials() {
    let mut refs = VP8LBackwardRefsInit(1);
    let mut saved = VP8LBackwardRefsInit(257);
    for i in 0..1025 {
        VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateLiteral(i)).unwrap();
    }
    BackwardRefsClone(&refs, &mut saved).unwrap();
    VP8LClearBackwardRefs(&mut refs);
    VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateCacheIdx(1023)).unwrap();
    BackwardRefsSwap(&mut refs, &mut saved);
    assert_eq!(refs.len(), 1025);
    let mut c = VP8LRefsCursorInit(&refs);
    for i in 0..1025 {
        assert!(VP8LRefsCursorOk(&c));
        let p = c.cur_pos.unwrap();
        assert_eq!(PixOrCopyLength(p), 1);
        assert_eq!(PixOrCopyLiteral(p, 0), i & 255);
        assert_eq!(PixOrCopyLiteral(p, 1), i >> 8);
        VP8LRefsCursorNext(&mut c);
    }
    assert!(!VP8LRefsCursorOk(&c));
    assert_eq!(PixOrCopyCacheIdx(saved.iter().next().unwrap()), 1023);
    VP8LBackwardRefsClear(&mut refs);
    BackwardRefsClone(&refs, &mut saved).unwrap();
    assert!(saved.is_empty());
    assert!(!VP8LRefsCursorOk(&VP8LRefsCursorInit(&saved)));
    BackwardRefsSwap(&mut refs, &mut saved);
    VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateCopy(1, 4095)).unwrap();
    assert_eq!(refs.len(), 1);
}

#[test]
fn histogram_reinitialization_preserves_counts_and_resets_cached_statistics() {
    let mut h = VP8LAllocateHistogram(1).unwrap();
    h.literal[0] = 9;
    h.literal[281] = 7;
    h.red[5] = 4;
    h.bin_id = 99;
    ComputeHistogramCost(&mut h);
    VP8LHistogramInit(&mut h, 1, false).unwrap();
    assert_eq!(h.literal[281], 7);
    assert_eq!(h.red[5], 4);
    assert_eq!(h.bin_id, 99);
    assert_eq!(h.costs, [0; 5]);
    assert_eq!(h.trivial_symbol, [VP8L_NON_TRIVIAL_SYM; 5]);
    VP8LHistogramInit(&mut h, 11, false).unwrap();
    assert_eq!(h.literal.len(), 2328);
    assert_eq!(h.literal[281], 7);
    assert!(h.literal[282..].iter().all(|&v| v == 0));
    VP8LHistogramInit(&mut h, 0, true).unwrap();
    assert_eq!(h.literal.len(), 280);
    assert_eq!(h.bin_id, 0);
    assert_eq!(h.is_used, [1; 5]);
    assert!((0..5).all(|i| GetPopulationInfo(&h, i).iter().all(|&v| v == 0)));
    let mut set = VP8LAllocateHistogramSet(3, 4).unwrap();
    set.histograms[2].alpha[255] = 100;
    set.size = 1;
    VP8LHistogramSetClear(&mut set);
    assert_eq!(set.size, 3);
    for h in &set.histograms {
        assert_eq!(h.palette_code_bits, 4);
        assert_eq!(h.is_used, [0; 5]);
        assert_eq!(h.trivial_symbol, [0; 5]);
        assert!(h.alpha.iter().all(|&v| v == 0));
    }
}

#[test]
fn oversized_allocations_return_errors() {
    let mut refs = VP8LBackwardRefsInit(usize::MAX);
    assert!(VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateLiteral(0)).is_err());
    assert!(refs.is_empty());
    assert!(VP8LAllocateHistogramSet(usize::MAX, 0).is_err());
}

#[test]
fn token_stream_replays_overlap_rows_and_cache_updates() {
    let first = 0xff123456;
    let second = 0x8000aa77;
    let mut refs = VP8LBackwardRefsInit(256);
    let mut cache = VP8LColorCacheInit(4).unwrap();
    let index = VP8LColorCacheGetIndex(&cache, second);
    for p in [
        PixOrCopyCreateLiteral(first),
        PixOrCopyCreateCopy(1, 8), // overlaps and crosses a 3-pixel row
        PixOrCopyCreateLiteral(second),
        PixOrCopyCreateCacheIdx(index),
        PixOrCopyCreateCopy(2, 7),
    ] {
        VP8LBackwardRefsCursorAdd(&mut refs, p).unwrap();
    }
    let mut pixels = Vec::new();
    for p in refs.iter() {
        for _ in 0..PixOrCopyLength(p) {
            let pixel = if PixOrCopyIsLiteral(p) {
                u32::from_le_bytes(std::array::from_fn(|i| PixOrCopyLiteral(p, i as u32) as u8))
            } else if PixOrCopyIsCacheIdx(p) {
                VP8LColorCacheLookup(&cache, PixOrCopyCacheIdx(p))
            } else {
                pixels[pixels.len() - PixOrCopyDistance(p) as usize]
            };
            pixels.push(pixel);
            VP8LColorCacheInsert(&mut cache, pixel);
        }
    }
    assert_eq!(pixels, [vec![first; 9], vec![second; 9]].concat());
}
