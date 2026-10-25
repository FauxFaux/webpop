mod support;

#[test]
fn committed_token_streams_match_scalar_c() {
    for line in include_str!("fixtures/token_streams.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        let function = words.next().unwrap();
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(support::Case::new(function, input).rust(), expected);
    }
}

#[test]
fn group_selection_uses_token_start_after_crossing_rows() {
    use libwebpop::internal::enc::backward_references_enc::*;
    use libwebpop::internal::enc::vp8l_enc::StoreImageToBitMask;
    use libwebpop::internal::utils::bit_writer_utils::*;
    use libwebpop::internal::utils::huffman_encode_utils::HuffmanTreeCode;
    // Deliberately supplied codes make group selection visible independently
    // of histogram construction. Cache index zero encodes as 0 in group zero,
    // 1 in group one. Copy length 7 crosses two rows of width 3.
    let mut codes: Vec<_> = (0..10)
        .map(|_| HuffmanTreeCode {
            code_lengths: vec![1; 282],
            codes: vec![0; 282],
        })
        .collect();
    codes[5].codes[280] = 1;
    let mut refs = VP8LBackwardRefsInit(256);
    VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateCopy(1, 7)).unwrap();
    VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateCacheIdx(0)).unwrap();
    VP8LBackwardRefsCursorAdd(&mut refs, PixOrCopyCreateCacheIdx(0)).unwrap();
    let mut bw = VP8LBitWriterInit(0).unwrap();
    // Four tiles: rows 0..1 group zero, rows 2..3 group one at x<2.
    StoreImageToBitMask(&mut bw, 3, 1, &refs, &[0, 0, 0x100, 0], &codes).unwrap();
    // Copy prefix: zero Huffman bit, zero extra bit (length 7), zero
    // distance bit; then group-one cache (1) and group-zero cache (0).
    assert_eq!(VP8LBitWriterFinish(&mut bw).unwrap(), &[0b01000]);
}
