#![allow(non_snake_case, non_upper_case_globals, dead_code)]
#[path = "../../../libwebpop/src/dsp/lossless.rs"]
mod lossless;
use std::{hint::black_box, time::Instant};
#[inline(never)]
fn copied(mode: usize, current: &[u32], upper: &[u32], out: &mut [u32]) {
    let pred = lossless::VP8LPredictors[mode];
    for (i, o) in out.iter_mut().enumerate() {
        let x = i + 1;
        *o = pred(current[x - 1], &[upper[x - 1], upper[x], upper[x + 1]]);
    }
}
#[inline(never)]
fn borrowed(mode: usize, current: &[u32], upper: &[u32], out: &mut [u32]) {
    let pred = lossless::VP8LPredictors[mode];
    for (i, o) in out.iter_mut().enumerate() {
        let x = i + 1;
        *o = pred(current[x - 1], upper[x - 1..x + 2].try_into().unwrap());
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode: usize = args.get(1).map_or(10, |s| s.parse().unwrap());
    let which = args.get(2).map_or("both", String::as_str);
    let n = 4096;
    let current: Vec<u32> = (0..n + 2)
        .map(|i| (i as u32).wrapping_mul(0x9e3779b9))
        .collect();
    let upper: Vec<u32> = (0..n + 2)
        .map(|i| (i as u32).wrapping_mul(0x7f4a7c15))
        .collect();
    let mut a = vec![0; n];
    let mut b = vec![0; n];
    for m in 0..14 {
        copied(black_box(m), &current, &upper, &mut a);
        borrowed(black_box(m), &current, &upper, &mut b);
        assert_eq!(a, b);
    }
    let funcs: [(&str, fn(usize, &[u32], &[u32], &mut [u32])); 2] =
        [("copied", copied), ("borrowed", borrowed)];
    for rep in 0..6 {
        for j in 0..2 {
            let (label, f) = funcs[if rep % 2 == 0 { j } else { 1 - j }];
            if which != "both" && label != which {
                continue;
            }
            let start = Instant::now();
            for _ in 0..2000 {
                f(
                    black_box(mode),
                    black_box(&current),
                    black_box(&upper),
                    black_box(&mut a),
                );
                black_box(&a);
            }
            println!("mode={mode} {label} {:.6}", start.elapsed().as_secs_f64());
        }
    }
}
