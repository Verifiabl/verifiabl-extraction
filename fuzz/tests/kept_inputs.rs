//! Runs each kept input once and whole through each fuzz target's check: the
//! inputs in `regressions/<target>/` and every file in `../fixtures/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use verifiabl_extract_fuzz::{Target, TARGETS};

/// The limit that the nightly fuzzing gives each input. Every target reads a
/// small input and should take milliseconds.
const INPUT_TIME_LIMIT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
enum Outcome {
    Passed,
    Panicked,
    TimedOut,
}

/// Runs the check on its own thread, so an input that never finishes fails
/// at the deadline. A thread cannot be stopped: one that times out runs
/// until the test process exits.
fn run_with_deadline(check: fn(&[u8]), input: Vec<u8>, limit: Duration) -> Outcome {
    let (finished, outcome) = mpsc::channel();
    thread::spawn(move || {
        check(&input);
        // The receiver is gone only after a timeout, which is already reported.
        let _ = finished.send(());
    });
    match outcome.recv_timeout(limit) {
        Ok(()) => Outcome::Passed,
        Err(RecvTimeoutError::Disconnected) => Outcome::Panicked,
        Err(RecvTimeoutError::Timeout) => Outcome::TimedOut,
    }
}

fn files_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.expect("a readable folder entry").path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

fn kept_inputs(target: &Target) -> Vec<PathBuf> {
    let fuzz = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut inputs = files_in(&fuzz.join("regressions").join(target.name));
    inputs.extend(files_in(&fuzz.join("..").join("fixtures")));
    inputs
}

#[test]
fn kept_inputs_pass_every_fuzz_target() {
    let mut failures = Vec::new();
    for target in TARGETS {
        let inputs = kept_inputs(target);
        assert!(!inputs.is_empty(), "no kept inputs for {}", target.name);
        for input in &inputs {
            let bytes = fs::read(input).expect("a readable input");
            match run_with_deadline(target.check, bytes, INPUT_TIME_LIMIT) {
                Outcome::Passed => {}
                Outcome::Panicked => {
                    failures.push(format!("{}: {} panicked", target.name, input.display()))
                }
                // The check still runs and would slow each later input, so stop here.
                Outcome::TimedOut => panic!(
                    "{}: {} took more than {INPUT_TIME_LIMIT:?}. Other failures:\n{}",
                    target.name,
                    input.display(),
                    failures.join("\n")
                ),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "kept inputs failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn a_check_past_the_deadline_times_out() {
    fn never_finishes(_: &[u8]) {
        thread::sleep(Duration::from_secs(3600));
    }
    fn panics(_: &[u8]) {
        panic!("a deliberate failure");
    }
    fn passes(_: &[u8]) {}

    let limit = Duration::from_millis(100);
    assert_eq!(
        run_with_deadline(never_finishes, Vec::new(), limit),
        Outcome::TimedOut
    );
    assert_eq!(
        run_with_deadline(panics, Vec::new(), limit),
        Outcome::Panicked
    );
    assert_eq!(
        run_with_deadline(passes, Vec::new(), limit),
        Outcome::Passed
    );
}

#[test]
fn every_fuzz_target_has_its_check_listed() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz_targets");
    let mut targets: Vec<String> = files_in(&folder)
        .iter()
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    targets.sort();
    let mut listed: Vec<&str> = TARGETS.iter().map(|target| target.name).collect();
    listed.sort();
    assert_eq!(targets, listed);
}
