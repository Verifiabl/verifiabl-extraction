//! Prints the third-party notices for the extraction module. nix/notices.nix
//! runs it with the build's inputs; every option is required.

use std::path::PathBuf;
use std::process::ExitCode;

use verifiabl_extract_notices::{third_party_notices, Inputs};

const USAGE: &str = "usage: verifiabl-extract-notices --cargo-metadata <file> --pdfium <dir> --zxing <dir> --wasi-sdk <dir> --licenses <dir> --rustc-version <text>";

fn main() -> ExitCode {
    let inputs = match parse(std::env::args().skip(1).collect()) {
        Ok(inputs) => inputs,
        Err(reason) => {
            eprintln!("{reason}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match third_party_notices(&inputs) {
        Ok(notices) => {
            print!("{notices}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: Vec<String>) -> Result<Inputs, String> {
    let mut values = std::collections::HashMap::new();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument {flag}"))?
            .to_string();
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        if values.insert(name, value).is_some() {
            return Err(format!("{flag} is given twice"));
        }
    }
    let mut take = |name: &str| {
        values
            .remove(name)
            .ok_or_else(|| format!("--{name} is required"))
    };
    let inputs = Inputs {
        cargo_metadata: PathBuf::from(take("cargo-metadata")?),
        pdfium: PathBuf::from(take("pdfium")?),
        zxing: PathBuf::from(take("zxing")?),
        wasi_sdk: PathBuf::from(take("wasi-sdk")?),
        licenses: PathBuf::from(take("licenses")?),
        rustc_version: take("rustc-version")?,
    };
    if let Some(unknown) = values.keys().next() {
        return Err(format!("unknown option --{unknown}"));
    }
    Ok(inputs)
}
