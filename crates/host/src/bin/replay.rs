//! Runs every file in the given folders through the built module, as an
//! image, as a PDF and as greyscale pixels, and fails unless the module kept
//! to its interface each time: a status the interface defines, within the
//! time budget, with a result of the documented shape. A trap is allowed; it is how a decoder gives up on
//! a damaged file. This is how a fuzzing corpus reaches the C++ in the module,
//! which the native fuzz targets cannot. A file that breaks the interface is
//! copied to fuzz/artifacts/replay/, because the corpus it came from is not
//! kept for long.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use verifiabl_extract_host::cli::{self, Failure};
use verifiabl_extract_host::findings::keep_finding;
use verifiabl_extract_host::{extraction_root, read_bounded, Input, Outcome, ReadKind, Sandbox};

const USAGE: &str = "usage: replay [--timeout-ms <n>] [--module <path>] <folder>...";

fn main() -> ExitCode {
    cli::exit(USAGE, run(std::env::args().skip(1).collect()))
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let mut folders = Vec::new();
    let mut timeout = cli::DEFAULT_TIMEOUT;
    let mut module = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--timeout-ms" => timeout = cli::timeout(&value(&mut args, &arg)?)?,
            "--module" => module = Some(PathBuf::from(value(&mut args, &arg)?)),
            flag if flag.starts_with("--") => {
                return Err(Failure::Usage(format!("unknown option {flag}")))
            }
            _ => folders.push(PathBuf::from(arg)),
        }
    }
    if folders.is_empty() {
        return Err(Failure::Usage("name at least one folder".to_string()));
    }

    let mut files = Vec::new();
    for folder in &folders {
        files_in(folder, &mut files).map_err(|error| {
            Failure::Error(format!("cannot list {}: {error}", folder.display()))
        })?;
    }
    files.sort();
    let reads: Vec<(PathBuf, ReadKind)> = files
        .into_iter()
        .flat_map(|file| ReadKind::ALL.map(|kind| (file.clone(), kind)))
        .collect();

    let sandbox = cli::sandbox(module)?;
    let problems = replay(&sandbox, &reads, timeout)?;
    if !problems.is_empty() {
        return Err(Failure::Error(format!(
            "The module broke its interface for {} of {} reads; the files are in fuzz/artifacts/replay/:\n{}",
            problems.len(),
            reads.len(),
            problems.join("\n")
        )));
    }
    println!(
        "The module kept to its interface for {} reads of {} files.",
        reads.len(),
        reads.len() / ReadKind::ALL.len()
    );
    Ok(())
}

// Each read has its own store, so run as many at once as there are processors.
fn replay(
    sandbox: &Sandbox,
    reads: &[(PathBuf, ReadKind)],
    timeout: Duration,
) -> Result<Vec<String>, Failure> {
    let findings = extraction_root().join("fuzz/artifacts/replay");
    let next = AtomicUsize::new(0);
    let problems = Mutex::new(Vec::new());
    let errors = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some((file, kind)) = reads.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let max_bytes = sandbox.abi().limits.max_file_bytes;
                    let found = read_bounded(file, max_bytes)
                        .map_err(|error| format!("cannot read {}: {error}", file.display()))
                        .and_then(|bytes| {
                            // A file over the limit says nothing about the module.
                            let Some(bytes) = bytes else {
                                return Ok(None);
                            };
                            let outcome = sandbox.extract(Input::of(*kind, &bytes), timeout)?;
                            let Some(found) = problem(&outcome) else {
                                return Ok(None);
                            };
                            keep_finding(&findings, file, &bytes, *kind, &found).map_err(
                                |error| format!("cannot keep {}: {error}", file.display()),
                            )?;
                            Ok(Some(format!(
                                "{} ({}): {found}",
                                file.display(),
                                kind.name()
                            )))
                        });
                    match found {
                        Ok(Some(problem)) => problems.lock().unwrap().push(problem),
                        Ok(None) => {}
                        Err(error) => errors.lock().unwrap().push(error),
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    if !errors.is_empty() {
        return Err(Failure::Error(errors.join("\n")));
    }
    let mut problems = problems.into_inner().unwrap();
    problems.sort();
    Ok(problems)
}

// Why this read broke the interface, or None when it kept to it. The host
// already checked the status and the result's shape. Its other statuses,
// such as a file over the size limit, say nothing about the module.
fn problem(outcome: &Outcome) -> Option<String> {
    if let Some(fault) = &outcome.fault {
        return Some(fault.clone());
    }
    if outcome.status == "timedOut" {
        return Some("the read did not finish within the time budget".to_string());
    }
    None
}

fn files_in(folder: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            files_in(&entry.path(), files)?;
        } else if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, Failure> {
    args.next()
        .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))
}
