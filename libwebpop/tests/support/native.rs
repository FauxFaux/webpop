use crate::support::{Case, SEED, UPSTREAM_REVISION};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

fn checked_output(command: &mut Command) -> std::process::Output {
    let result = command
        .output()
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
    assert!(
        result.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

pub fn build_dir() -> &'static PathBuf {
    static BUILD: OnceLock<PathBuf> = OnceLock::new();
    BUILD.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let upstream = root.join("upstream");
        let revision = checked_output(
            Command::new("git")
                .arg("-C")
                .arg(&upstream)
                .args(["rev-parse", "HEAD"]),
        );
        assert_eq!(
            String::from_utf8_lossy(&revision.stdout).trim(),
            UPSTREAM_REVISION,
            "reference source revision changed; review pin and fixtures first"
        );
        let dirty = checked_output(Command::new("git").arg("-C").arg(&upstream).args([
            "status",
            "--porcelain",
            "--untracked-files=all",
        ]));
        assert!(
            dirty.stdout.is_empty(),
            "oracle requires an unmodified upstream checkout"
        );
        let build = root.join("target/native-oracle");
        checked_output(
            Command::new("cmake")
                .arg("-S")
                .arg(root.join("libwebpop/tests/reference"))
                .arg("-B")
                .arg(&build)
                .arg("-DCMAKE_BUILD_TYPE=Release"),
        );
        checked_output(Command::new("cmake").arg("--build").arg(&build).args([
            "--target",
            "webpop_oracle",
            "--config",
            "Release",
            "--parallel",
            "4",
        ]));
        // Preserve actual flags, compiler path/version and host details, including
        // user-supplied CMake/compiler environment, alongside every live run.
        let mut provenance = format!("upstream={UPSTREAM_REVISION}\nseed={SEED:08x}\n");
        provenance.push_str(&fs::read_to_string(build.join("CMakeCache.txt")).unwrap());
        provenance.push_str("\ncompile_commands.json:\n");
        provenance.push_str(&fs::read_to_string(build.join("compile_commands.json")).unwrap());
        for (program, args) in [("cmake", vec!["--version"]), ("rustc", vec!["-vV"])] {
            let output = checked_output(Command::new(program).args(args));
            provenance.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        // CMake records the configured C compiler, which need not be `cc`.
        let cache = fs::read_to_string(build.join("CMakeCache.txt")).unwrap();
        let compiler = cache
            .lines()
            .find_map(|line| line.strip_prefix("CMAKE_C_COMPILER:FILEPATH="))
            .unwrap();
        let version = checked_output(Command::new(compiler).arg("--version"));
        provenance.push_str(&String::from_utf8_lossy(&version.stdout));
        fs::write(build.join("provenance.txt"), provenance).unwrap();
        build
    })
}

pub fn request_text(case: &Case) -> String {
    let mut line = format!("{} {}", case.function, case.input.len());
    for word in &case.input {
        write!(line, " {word:08x}").unwrap();
    }
    line.push('\n');
    line
}

/// Batch requests through files to avoid stdin/stdout pipe deadlocks. Retain
/// complete requests/results and a one-line mismatch reproducer for debugging.
pub fn run(label: &str, cases: &[Case]) -> Vec<Vec<u32>> {
    let build = build_dir();
    let input_path = build.join(format!("{label}.requests.txt"));
    let mut input = BufWriter::new(File::create(&input_path).unwrap());
    for case in cases {
        input.write_all(request_text(case).as_bytes()).unwrap();
    }
    input.flush().unwrap();
    drop(input);
    let executable = format!("webpop_oracle{}", std::env::consts::EXE_SUFFIX);
    let mut exe = build.join(&executable);
    if !exe.is_file() {
        exe = build.join("Release").join(executable);
    }
    let result = Command::new(exe)
        .stdin(Stdio::from(File::open(&input_path).unwrap()))
        .output()
        .expect("run native oracle");
    fs::write(build.join(format!("{label}.responses.txt")), &result.stdout).unwrap();
    assert!(
        result.status.success(),
        "oracle failed for {}: {}",
        input_path.display(),
        String::from_utf8_lossy(&result.stderr)
    );
    let response = String::from_utf8(result.stdout).unwrap();
    let outputs: Vec<_> = response
        .lines()
        .map(|line| {
            let mut words = line.split_whitespace();
            let count: usize = words.next().unwrap().parse().unwrap();
            let output: Vec<_> = words
                .map(|word| u32::from_str_radix(word, 16).unwrap())
                .collect();
            assert_eq!(output.len(), count, "truncated oracle response");
            output
        })
        .collect();
    assert_eq!(outputs.len(), cases.len(), "missing oracle responses");
    outputs
}

pub fn compare(case: &Case, expected: &[u32], actual: &[u32]) {
    if expected != actual {
        let index = expected
            .iter()
            .zip(actual)
            .position(|(a, b)| a != b)
            .unwrap_or(expected.len().min(actual.len()));
        let reproducer = build_dir().join("mismatch.request.txt");
        fs::write(&reproducer, request_text(case)).unwrap();
        panic!(
            "{} differs at output word {index}: expected {:?}, actual {:?}; \
            lengths {} / {}; replay {} (upstream {}, seed {:08x})",
            case.function,
            expected.get(index),
            actual.get(index),
            expected.len(),
            actual.len(),
            reproducer.display(),
            UPSTREAM_REVISION,
            SEED
        );
    }
}

// Use several batches per worker so expensive cases do not hold up a fixed
// shard. Keep every frame sequence within its case, and restore input order.
pub fn parallel_cases<T: Send>(
    label: &str,
    cases: &[Case],
    check: impl Fn(&Case, &[u32]) -> T + Sync,
) -> Vec<T> {
    if cases.is_empty() {
        return Vec::new();
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    // Amortize process startup for small leaf operations, while giving each
    // worker several batches to claim in the large encoder sweeps.
    let batch_count = (workers * 4).min(cases.len().div_ceil(16));
    let batches: Vec<_> = (0..batch_count).collect();
    let results = crate::support::parallel::map(&batches, |&batch| {
        let cases: Vec<_> = cases
            .iter()
            .skip(batch)
            .step_by(batch_count)
            .cloned()
            .collect();
        let outputs = run(&format!("{label}-{batch}"), &cases);
        cases
            .iter()
            .zip(outputs)
            .enumerate()
            .map(|(index, (case, expected))| (batch + index * batch_count, check(case, &expected)))
            .collect::<Vec<_>>()
    });
    let mut results: Vec<_> = results.into_iter().flatten().collect();
    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
