//! Reads one file with the module and prints the status and what it found.
//! Payload text is printed only with --show-texts, so the output of a real
//! payslip can be shared without it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use verifiabl_extract_host::cli::{self, Failure};
use verifiabl_extract_host::{read_bounded, Input, PixelShape, ReadKind};

const USAGE: &str = "usage: read <image|pdf> <file> [--show-texts] [--timeout-ms <n>] [--module <path>]
       read pixels <file> --width <n> --height <n> --format <name> [--show-texts] [--timeout-ms <n>] [--module <path>]";

fn main() -> ExitCode {
    cli::exit(USAGE, run(std::env::args().skip(1).collect()))
}

/// The shape of a pixel read, as far as the options gave it.
#[derive(Default)]
struct ShapeOptions {
    width: Option<u32>,
    height: Option<u32>,
    format: Option<String>,
}

impl ShapeOptions {
    fn is_empty(&self) -> bool {
        self.width.is_none() && self.height.is_none() && self.format.is_none()
    }

    fn shape(&self) -> Option<PixelShape<'_>> {
        Some(PixelShape {
            width: self.width?,
            height: self.height?,
            format: self.format.as_deref()?,
        })
    }
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let mut positionals = Vec::new();
    let mut show_texts = false;
    let mut timeout = cli::DEFAULT_TIMEOUT;
    let mut module = None;
    let mut shape = ShapeOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--show-texts" => show_texts = true,
            "--timeout-ms" => timeout = cli::timeout(&value(&mut args, &arg)?)?,
            "--module" => module = Some(PathBuf::from(value(&mut args, &arg)?)),
            "--width" => shape.width = Some(size(&value(&mut args, &arg)?, &arg)?),
            "--height" => shape.height = Some(size(&value(&mut args, &arg)?, &arg)?),
            "--format" => shape.format = Some(value(&mut args, &arg)?),
            flag if flag.starts_with("--") => {
                return Err(Failure::Usage(format!("unknown option {flag}")))
            }
            _ => positionals.push(arg),
        }
    }
    let [kind, file] = positionals.as_slice() else {
        return Err(Failure::Usage(
            "name a kind of read and one file".to_string(),
        ));
    };
    let kind = ReadKind::parse(kind)
        .ok_or_else(|| Failure::Usage(format!("{kind} is not image, pdf or pixels")))?;
    let sandbox = cli::sandbox(module)?;
    let (limit, refusal) = sandbox.input_limit(kind);
    let bytes = read_bounded(Path::new(file), limit)
        .map_err(|error| Failure::Error(format!("cannot read {file}: {error}")))?;
    let Some(bytes) = bytes else {
        println!("status: {refusal}");
        return Ok(());
    };
    let input = input(kind, &bytes, &shape)?;
    let outcome = sandbox.extract(input, timeout).map_err(Failure::Error)?;
    println!("status: {}", outcome.status);
    if let Some(fault) = &outcome.fault {
        println!("fault: {fault}");
    }
    if let Some(result) = &outcome.result {
        for field in ["metadata", "codes"] {
            let Some(texts) = result.get(field).and_then(|value| value.as_array()) else {
                continue;
            };
            println!("{field}: {}", texts.len());
            if show_texts {
                for text in texts {
                    println!("  {}", text.as_str().unwrap_or_default());
                }
            }
        }
        if let Some(pages) = result.get("pages") {
            println!("pages rendered: {pages}");
        }
    }
    Ok(())
}

// Pixels have no header, so a pixel read needs their shape, and a file read takes none.
fn input<'a>(
    kind: ReadKind,
    bytes: &'a [u8],
    shape: &'a ShapeOptions,
) -> Result<Input<'a>, Failure> {
    if kind != ReadKind::Pixels {
        if !shape.is_empty() {
            return Err(Failure::Usage(
                "--width, --height and --format are for a pixel read".to_string(),
            ));
        }
        return Ok(Input::of(kind, bytes));
    }
    let shape = shape.shape().ok_or_else(|| {
        Failure::Usage("a pixel read needs --width, --height and --format".to_string())
    })?;
    Ok(Input::Pixels(bytes, shape))
}

fn size(value: &str, flag: &str) -> Result<u32, Failure> {
    value
        .parse()
        .map_err(|_| Failure::Usage(format!("{flag} must be a whole number")))
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, Failure> {
    args.next()
        .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))
}
