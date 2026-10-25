use super::{Case, vp8_loop as frames, vp8_reconstruct as reconstruct, vp8_search as search};

pub fn request(method: u32, name: &str, base: Case) -> Case {
    let mut a = vec![method];
    a.extend(base.input);
    Case::new(name, a)
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in [5, 6] {
        for kind in 0..4 {
            for flags in [0, 7] {
                cases.push(request(
                    method,
                    "VP8TrellisReconstruct",
                    reconstruct::request(kind, 75, flags, if kind == 1 { 9 } else { 3 }, 6),
                ));
            }
        }
        for stage in 0..=6 {
            for flags in [0, 7] {
                cases.push(request(
                    method,
                    "VP8TrellisSearch",
                    search::search_case(stage, 75, flags, 6),
                ));
            }
        }
        for mode in [0, 3] {
            let mut base = search::search_case(6, 37, 7, 2);
            base.input[0] = 1;
            base.input[4] = mode;
            cases.push(request(method, "VP8TrellisSearch", base));
        }
        for (w, h, q, p) in [
            (1, 1, 0., 0),
            (17, 19, 75., 4),
            (17, 19, 98.5, 2),
            (97 * 16, 1, 75., 4),
        ] {
            let mut base = frames::frame_case(w, h, q, 4, p, true);
            if w == 97 * 16 {
                base.input[4] = 1;
                base.input[5] = 0;
            }
            cases.push(request(method, "VP8MethodLoopTrace", base));
        }
    }
    cases
}

pub fn reconstruction_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in [5, 6] {
        for base in reconstruct::reconstruction_cases() {
            cases.push(request(method, "VP8TrellisReconstruct", base));
        }
    }
    cases
}

pub fn search_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in [5, 6] {
        for q in [0, 1, 17, 37, 63, 75, 98, 126, 127] {
            for stage in 0..=6 {
                for flags in [0, 3, 4, 7] {
                    for pattern in 0..8 {
                        cases.push(request(
                            method,
                            "VP8TrellisSearch",
                            search::search_case(stage, q, flags, pattern),
                        ));
                    }
                }
            }
        }
        for flags in 0..8 {
            for budget in [0, 1, 211, 640, 1000, 4000, 65536] {
                for stage in [1, 2, 4] {
                    let mut base = search::search_case(stage, 37 + flags, flags, 2);
                    base.input[635] = budget;
                    cases.push(request(method, "VP8TrellisSearch", base));
                }
            }
        }
        for mode in 0..4 {
            for q in 0..128 {
                let mut base = search::search_case(6, q, q % 8, 6);
                base.input[0] = 1;
                base.input[4] = mode;
                cases.push(request(method, "VP8TrellisSearch", base));
            }
        }
    }
    cases
}

pub fn loop_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in [5, 6] {
        // Reuse the full driver geometry/probe/refresh/retry corpus for one
        // method, replacing its selector without changing source generation.
        for base in super::lossy_methods::driver_cases()
            .into_iter()
            .filter(|c| c.input[0] == 3)
        {
            let mut base = base;
            base.input[0] = method;
            cases.push(base);
        }
    }
    cases
}

pub fn public_cases() -> Vec<Case> {
    super::lossy_methods::driver_rgba_cases()
        .into_iter()
        .filter(|c| c.input[0] == 3)
        .flat_map(|c| {
            [5, 6].into_iter().map(move |method| {
                let mut c = c.clone();
                c.input[0] = method;
                c
            })
        })
        .collect()
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let method = case.input[0] as i32;
    let a = &case.input[1..];
    match case.function.as_str() {
        "VP8MethodLoopTrace" => super::lossy_methods::driver_output(case),
        "VP8TrellisReconstruct" => reconstruct::evaluate_with_method(a, method),
        "VP8TrellisSearch" => {
            let (mut enc, mut it) = search::initialized_with_method(a, method);
            it.do_trellis = method == 6;
            let mut rd = reconstruct::seed_score();
            let mut calls = Vec::new();
            let result = search::run_with_observer(a, &mut enc, &mut it, &mut rd, &mut |_, r| {
                calls.extend([
                    u32::from(r.kind),
                    u32::from(r.trellis),
                    r.block as u32,
                    r.mode as u32,
                ]);
            });
            let mut out = search::state(a, &enc, &it, &rd, result);
            out.push(calls.len() as u32 / 4);
            out.extend(calls);
            out
        }
        _ => unreachable!(),
    }
}
