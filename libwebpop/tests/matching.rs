use libwebpop::internal::enc::backward_references_cost_enc::*;
use libwebpop::internal::enc::backward_references_enc::*;
use libwebpop::internal::webp::format_constants::MAX_LENGTH;

#[test]
fn saturated_runs_retain_upstream_last_pixel_boundary() {
    let pixels = vec![0xff123456; MAX_LENGTH as usize * 2 + 3];
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    VP8LHashChainFill(&mut chain, 70, &pixels, 1, pixels.len(), false).unwrap();
    assert_eq!(VP8LHashChainFindCopy(&chain, 0), (0, 0));
    for i in 1..pixels.len() - 1 {
        assert_eq!(
            VP8LHashChainFindCopy(&chain, i),
            (1, (pixels.len() - 1 - i).min(MAX_LENGTH as usize))
        );
    }
    assert_eq!(VP8LHashChainFindCopy(&chain, pixels.len() - 1), (0, 0));
    let mut refs = VP8LBackwardRefsInit(256);
    BackwardReferencesRle(1, pixels.len(), &pixels, 0, &mut refs).unwrap();
    let tokens: Vec<_> = refs.iter().copied().collect();
    assert_eq!(
        tokens,
        [
            PixOrCopyCreateLiteral(pixels[0]),
            PixOrCopyCreateCopy(1, MAX_LENGTH as u16),
            PixOrCopyCreateCopy(1, MAX_LENGTH as u16),
            PixOrCopyCreateLiteral(pixels[0]),
            PixOrCopyCreateLiteral(pixels[0])
        ]
    );
}

#[test]
fn hash_storage_reuses_capacity_for_different_geometries() {
    let pixels: Vec<_> = (0..1024).map(|i| (i % 17) * 0x01010101).collect();
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    for (width, height, low) in [
        (32, 32, false),
        (1, 2, true),
        (1, 1, false),
        (17, 31, true),
        (16, 64, false),
    ] {
        VP8LHashChainFill(&mut chain, 70, &pixels, width, height, low).unwrap();
        let mut fresh = VP8LHashChainInit(width * height).unwrap();
        VP8LHashChainFill(&mut fresh, 70, &pixels, width, height, low).unwrap();
        assert_eq!(&chain.offset_length[..width * height], fresh.offset_length);
    }
    VP8LHashChainClear(&mut chain);
    assert!(chain.offset_length.is_empty());
}

#[test]
fn cache_selection_disables_cache_at_low_quality_and_models_singletons() {
    let pixels = [0; 8];
    let mut refs = VP8LBackwardRefsInit(256);
    for &p in &pixels {
        VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateLiteral(p)).unwrap();
    }
    let mut bits = 10;
    CalculateBestCacheSize(&pixels, 25, &refs, &mut bits).unwrap();
    assert_eq!(bits, 0);
    BackwardRefsWithLocalCache(&pixels, 1, &mut refs).unwrap();
    assert!(refs.iter().all(|p| *p == PixOrCopyCreateCacheIdx(0)));
    let model = CostModelBuild(8, 1, &refs).unwrap();
    assert_eq!(GetCacheCost(&model, 0), 0);
    let mut costs = [99; 4];
    ConvertPopulationCountTableToBitEstimates(&[0, 7, 0, 0], &mut costs);
    assert_eq!(costs, [0; 4]);
    ConvertPopulationCountTableToBitEstimates(&[1, 1, 0, 0], &mut costs);
    assert_eq!(costs[0], 1 << 23);
    assert_eq!(costs[1], 1 << 23);
}
