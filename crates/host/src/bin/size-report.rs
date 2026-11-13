//! Prints what takes up a module's size: each section, the code size of each
//! linked library and the largest functions. Give it the debug build, whose
//! name section names the functions:
//! `nix build ../..#module-debug`, then `result/verifiabl-extract.wasm`.

use std::process::ExitCode;

use verifiabl_extract_host::cli::{self, Failure};
use verifiabl_extract_host::size::{library_of, module_sizes, sizes_by_library};

const USAGE: &str = "usage: size-report [--top <count>] <module.wasm>";
const DEFAULT_TOP: usize = 30;
const KIB: f64 = 1024.0;

fn main() -> ExitCode {
    cli::exit(USAGE, run(std::env::args().skip(1).collect()))
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let mut top = DEFAULT_TOP;
    let mut modules = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--top" => {
                top = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| Failure::Usage("--top needs a whole number".to_string()))?;
            }
            flag if flag.starts_with("--") => {
                return Err(Failure::Usage(format!("unknown option {flag}")))
            }
            _ => modules.push(arg),
        }
    }
    let [module] = modules.as_slice() else {
        return Err(Failure::Usage("name one module".to_string()));
    };
    let wasm = std::fs::read(module)
        .map_err(|error| Failure::Error(format!("cannot read {module}: {error}")))?;
    let sizes = module_sizes(&wasm)
        .map_err(|error| Failure::Error(format!("{module} is not a module: {error}")))?;

    println!("Sections of {module} ({}):", kib(sizes.total));
    for (name, bytes) in &sizes.sections {
        println!(
            "  {}  {}  {name}",
            kib(*bytes),
            percent(*bytes, sizes.total)
        );
    }

    let code: usize = sizes.functions.iter().map(|function| function.bytes).sum();
    if sizes
        .functions
        .iter()
        .all(|function| function.name.starts_with("function "))
    {
        println!("\nThe module has no function names; use the debug build to attribute its code.");
        return Ok(());
    }
    println!("\nCode by library ({}):", kib(code));
    for (library, bytes) in sizes_by_library(&sizes.functions) {
        println!("  {}  {}  {library}", kib(bytes), percent(bytes, code));
    }

    let mut largest: Vec<_> = sizes.functions.iter().collect();
    largest.sort_by_key(|function| std::cmp::Reverse(function.bytes));
    println!("\nLargest {top} functions:");
    for function in largest.into_iter().take(top) {
        println!(
            "  {}  {:<22}  {}",
            kib(function.bytes),
            library_of(&function.name),
            function.name
        );
    }
    Ok(())
}

fn kib(bytes: usize) -> String {
    format!("{:>9.1} KiB", bytes as f64 / KIB)
}

fn percent(part: usize, whole: usize) -> String {
    format!("{:>5.1}%", part as f64 * 100.0 / whole.max(1) as f64)
}
