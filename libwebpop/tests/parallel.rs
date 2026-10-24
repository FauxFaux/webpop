#[path = "support/parallel.rs"]
mod parallel;

#[test]
fn empty_singleton_and_ordered_results_visit_each_case_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    assert!(parallel::map::<usize, usize>(&[], |_| panic!("empty batch")).is_empty());
    assert_eq!(parallel::map(&[7], |value| value * 2), [14]);

    let cases: Vec<_> = (0..257).collect();
    let visits: Vec<_> = cases.iter().map(|_| AtomicUsize::new(0)).collect();
    let results = parallel::map(&cases, |&index| {
        assert_eq!(visits[index].fetch_add(1, Ordering::Relaxed), 0);
        std::thread::yield_now();
        index * 3
    });
    assert_eq!(
        results,
        cases.iter().map(|index| index * 3).collect::<Vec<_>>()
    );
    assert!(
        visits
            .iter()
            .all(|count| count.load(Ordering::Relaxed) == 1)
    );
}

#[test]
#[should_panic(expected = "case failed")]
fn worker_failure_fails_the_test_with_the_original_message() {
    parallel::map(&[0, 1, 2], |&index| assert_ne!(index, 1, "case failed"));
}
