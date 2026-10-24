use std::sync::atomic::{AtomicUsize, Ordering};

/// Evaluate independent cases with bounded workers, retaining input order.
/// Claim work dynamically so expensive images do not hold up a fixed shard.
pub fn map<T: Sync, R: Send>(cases: &[T], check: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8)
        .min(cases.len());
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let (next, check) = (&next, &check);
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(case) = cases.get(index) else {
                            break;
                        };
                        results.push((index, check(case)));
                    }
                    results
                })
            })
            .collect();
        let mut results: Vec<_> = handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect();
        results.sort_unstable_by_key(|(index, _)| *index);
        results.into_iter().map(|(_, result)| result).collect()
    })
}
