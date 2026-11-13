//! What the binaries share: usage errors, the default module and budget.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use crate::{extraction_root, Abi, Sandbox};

/// The readers' default budget in the SDKs.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Usage errors exit 2; other failures exit 1.
pub enum Failure {
    Usage(String),
    Error(String),
}

pub fn exit(usage: &str, result: Result<(), Failure>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(reason)) => {
            eprintln!("{reason}\n{usage}");
            ExitCode::from(2)
        }
        Err(Failure::Error(message)) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

pub fn timeout(value: &str) -> Result<Duration, Failure> {
    value
        .parse::<u64>()
        .ok()
        .filter(|milliseconds| *milliseconds > 0)
        .map(Duration::from_millis)
        .ok_or_else(|| Failure::Usage("--timeout-ms must be a positive whole number".to_string()))
}

/// The committed module, or the one named, with the interface it implements.
pub fn sandbox(module: Option<PathBuf>) -> Result<Sandbox, Failure> {
    let root = extraction_root();
    let module = module.unwrap_or_else(|| root.join("dist/verifiabl-extract.wasm"));
    let wasm = std::fs::read(&module)
        .map_err(|error| Failure::Error(format!("cannot read {}: {error}", module.display())))?;
    let abi = Abi::read(&root.join("abi/extraction-abi.json")).map_err(Failure::Error)?;
    Sandbox::new(&wasm, abi).map_err(Failure::Error)
}
