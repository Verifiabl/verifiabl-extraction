//! A reference host for the extraction module. It runs the module under
//! Wasmtime as the SDK hosts do: a new store and instance for each read, no
//! imports, a memory cap, and an epoch deadline at the time budget. The
//! module's output is untrusted, so a module that breaks the interface reads
//! as `unreadable`, and the outcome names the fault.

pub mod abi;
pub mod cli;
pub mod findings;
pub mod size;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};
use wasmtime::{
    Config, Engine, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
    TypedFunc, WasmParams, WasmResults,
};

pub use abi::Abi;

/// How often the epoch advances. A read stops within one tick of its budget.
const TICK: Duration = Duration::from_millis(10);
/// The native stack the module's frames may use before they trap, as the
/// .NET host allows.
const MAX_WASM_STACK_BYTES: usize = 512 * 1024;

/// Reads a file of at most `max_bytes`, or gives None for a larger one, as a
/// host refuses an oversized file before it copies it. It reads at most
/// `max_bytes` + 1 bytes: the extra byte shows the file is too large. The file
/// is read once, so it cannot grow past the check.
pub fn read_bounded(path: &Path, max_bytes: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    std::fs::File::open(path)?
        .take(limit)
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() <= max_bytes).then_some(bytes))
}

/// The extraction folder: `abi/`, `dist/`, `fixtures/` and `stand-ins/`.
pub fn extraction_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadKind {
    Image,
    Pdf,
    Pixels,
}

impl ReadKind {
    pub const ALL: [ReadKind; 3] = [ReadKind::Image, ReadKind::Pdf, ReadKind::Pixels];

    pub fn parse(name: &str) -> Option<ReadKind> {
        ReadKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            ReadKind::Image => "image",
            ReadKind::Pdf => "pdf",
            ReadKind::Pixels => "pixels",
        }
    }

    fn export(self) -> &'static str {
        match self {
            ReadKind::Image => "vf_read_image",
            ReadKind::Pdf => "vf_read_pdf",
            ReadKind::Pixels => "vf_read_pixels",
        }
    }
}

/// The size of an image that the caller decoded, and how each pixel is
/// stored: a name in the interface's `pixelFormats`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelShape<'a> {
    pub width: u32,
    pub height: u32,
    pub format: &'a str,
}

/// What a read is given: a file, or pixels that the caller decoded.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    Image(&'a [u8]),
    Pdf(&'a [u8]),
    Pixels(&'a [u8], PixelShape<'a>),
}

impl<'a> Input<'a> {
    /// `bytes` as the input of a `kind` read. A pixel read takes them as the
    /// greyscale pixels of the squarest image that they fill, and leaves out
    /// the bytes after it.
    pub fn of(kind: ReadKind, bytes: &'a [u8]) -> Input<'a> {
        match kind {
            ReadKind::Image => Input::Image(bytes),
            ReadKind::Pdf => Input::Pdf(bytes),
            ReadKind::Pixels => {
                let width = bytes.len().isqrt();
                let height = bytes.len().checked_div(width).unwrap_or(0);
                let shape = PixelShape {
                    width: u32::try_from(width).unwrap_or(u32::MAX),
                    height: u32::try_from(height).unwrap_or(u32::MAX),
                    format: "gray8",
                };
                Input::Pixels(&bytes[..width * height], shape)
            }
        }
    }

    pub fn kind(&self) -> ReadKind {
        match self {
            Input::Image(_) => ReadKind::Image,
            Input::Pdf(_) => ReadKind::Pdf,
            Input::Pixels(..) => ReadKind::Pixels,
        }
    }

    fn bytes(&self) -> &'a [u8] {
        match self {
            Input::Image(bytes) | Input::Pdf(bytes) | Input::Pixels(bytes, _) => bytes,
        }
    }
}

/// What a read gave: a status name from the interface, the result for `ok`
/// and `partial`, and how the module broke the interface, if it did.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub status: String,
    pub result: Option<Value>,
    pub fault: Option<String>,
}

impl Outcome {
    fn status(name: &str) -> Outcome {
        Outcome {
            status: name.to_string(),
            result: None,
            fault: None,
        }
    }

    fn broken(fault: impl Into<String>) -> Outcome {
        Outcome {
            status: "unreadable".to_string(),
            result: None,
            fault: Some(fault.into()),
        }
    }
}

/// The exports a read calls, with the types the interface gives them.
struct Exports {
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    read: ReadExport,
    result_ptr: TypedFunc<(), i32>,
    result_len: TypedFunc<(), i32>,
    out_of_memory: TypedFunc<(), i32>,
}

/// The export that reads the input, with what it takes after the input's
/// address and length.
enum ReadExport {
    /// The pixel limit.
    File(TypedFunc<(i32, i32, i32), i32>),
    /// The width, the height, the format's code and the pixel limit.
    Pixels(TypedFunc<(i32, i32, i32, i32, i32, i32), i32>, [i32; 3]),
}

/// A compiled module, and the engine whose epoch a background thread advances.
pub struct Sandbox {
    abi: Abi,
    engine: Engine,
    module: Module,
    stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl Sandbox {
    /// Compiles `wasm` in an engine with the features the interface needs.
    pub fn new(wasm: &[u8], abi: Abi) -> Result<Sandbox, String> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        config.max_wasm_stack(MAX_WASM_STACK_BYTES);
        // ZXing-C++ catches C++ exceptions, which compile to wasm exceptions.
        config.wasm_exceptions(true);
        config.wasm_threads(false);
        let engine = Engine::new(&config).map_err(|error| format!("{error:#}"))?;
        let module = Module::new(&engine, wasm)
            .map_err(|error| format!("the module does not compile: {error:#}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let ticker = {
            let engine = engine.clone();
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("extraction epoch".to_string())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(TICK);
                        engine.increment_epoch();
                    }
                })
                .map_err(|error| format!("cannot start the epoch thread: {error}"))?
        };
        Ok(Sandbox {
            abi,
            engine,
            module,
            stop,
            ticker: Some(ticker),
        })
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn abi(&self) -> &Abi {
        &self.abi
    }

    /// The most bytes a `kind` read takes, and the status of an input with
    /// more. Pixels over `maxPixelBytes` are more than `maxPixels` pixels in
    /// every format.
    pub fn input_limit(&self, kind: ReadKind) -> (usize, &'static str) {
        match kind {
            ReadKind::Image | ReadKind::Pdf => (self.abi.limits.max_file_bytes, "fileTooLarge"),
            ReadKind::Pixels => (self.abi.limits.max_pixel_bytes, "tooManyPixels"),
        }
    }

    /// Reads one input. An error is a fault in this host's setup, such as a
    /// module with another ABI version; anything the input or the module
    /// does is an outcome.
    pub fn extract(&self, input: Input<'_>, timeout: Duration) -> Result<Outcome, String> {
        // Before an instance exists, so the module never allocates for an oversized input.
        let (limit, refusal) = self.input_limit(input.kind());
        if input.bytes().len() > limit {
            return Ok(Outcome::status(refusal));
        }
        let shape = self.shape_arguments(&input)?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.abi.memory.max_bytes)
            .instances(1)
            .tables(1)
            .memories(1)
            .build();
        let mut store = Store::new(&self.engine, limits);
        store.limiter(|limits| limits);
        // Whole ticks, rounded down so a read stops within its budget. A
        // deadline of zero has already passed, so a shorter budget gets one tick.
        let ticks = (timeout.as_millis() / TICK.as_millis()).max(1);
        store.set_epoch_deadline(u64::try_from(ticks).unwrap_or(u64::MAX));

        // The budget covers starting the instance too: running out of time
        // there is a timed-out read, not a fault in this host's setup.
        let instance = match Instance::new(&mut store, &self.module, &[]) {
            Ok(instance) => instance,
            Err(error) if interrupted(&error) => return Ok(Outcome::status("timedOut")),
            Err(error) => return Err(format!("the module does not instantiate: {error:#}")),
        };
        let version: TypedFunc<(), i32> = typed_func(&mut store, &instance, "vf_abi_version")?;
        let actual = match version.call(&mut store, ()) {
            Ok(actual) => actual,
            Err(error) if interrupted(&error) => return Ok(Outcome::status("timedOut")),
            Err(error) => return Err(format!("vf_abi_version failed: {error:#}")),
        };
        if actual as u32 != self.abi.abi_version {
            return Err(format!(
                "the module implements ABI {actual}; this host needs {}",
                self.abi.abi_version
            ));
        }
        let exports = self.exports(&mut store, &instance, input.kind(), shape)?;
        Ok(self.run(&mut store, &exports, input))
    }

    // What vf_read_pixels takes for the shape of the pixels: the width, the
    // height and the format's code. A format that the interface does not
    // name is a fault in the caller, not in the pixels.
    fn shape_arguments(&self, input: &Input<'_>) -> Result<Option<[i32; 3]>, String> {
        let Input::Pixels(_, shape) = input else {
            return Ok(None);
        };
        let format = self
            .abi
            .pixel_formats
            .get(shape.format)
            .ok_or_else(|| format!("the interface has no pixel format {}", shape.format))?;
        Ok(Some([
            shape.width as i32,
            shape.height as i32,
            *format as i32,
        ]))
    }

    // A module without the interface is a fault in this host's setup, not in the input.
    fn exports(
        &self,
        store: &mut Store<StoreLimits>,
        instance: &Instance,
        kind: ReadKind,
        shape: Option<[i32; 3]>,
    ) -> Result<Exports, String> {
        let memory = instance
            .get_memory(&mut *store, &self.abi.memory.export)
            .ok_or_else(|| format!("the module exports no memory {}", self.abi.memory.export))?;
        let read = match shape {
            Some(shape) => ReadExport::Pixels(typed_func(store, instance, kind.export())?, shape),
            None => ReadExport::File(typed_func(store, instance, kind.export())?),
        };
        Ok(Exports {
            memory,
            alloc: typed_func(store, instance, "vf_alloc")?,
            read,
            result_ptr: typed_func(store, instance, "vf_result_ptr")?,
            result_len: typed_func(store, instance, "vf_result_len")?,
            out_of_memory: typed_func(store, instance, "vf_out_of_memory")?,
        })
    }

    // Anything the module does wrong while reading reports the input as
    // unreadable: the input may have taken the module over, and the host must
    // not fail with it.
    fn run(&self, store: &mut Store<StoreLimits>, exports: &Exports, input: Input<'_>) -> Outcome {
        let bytes = input.bytes();
        // At most maxFileBytes or maxPixelBytes, which fit an i32.
        let length = bytes.len() as i32;
        let address = match exports.alloc.call(&mut *store, length) {
            Ok(address) => address as u32 as usize,
            Err(error) => return self.after_failure(store, exports, error),
        };
        let in_memory = address
            .checked_add(bytes.len())
            .is_some_and(|end| end <= exports.memory.data_size(&*store));
        if !in_memory {
            return Outcome::broken("vf_alloc returned a buffer outside the memory");
        }
        exports.memory.data_mut(&mut *store)[address..address + bytes.len()].copy_from_slice(bytes);

        let max_pixels = self.abi.limits.max_pixels as i32;
        let address = address as i32;
        let returned = match &exports.read {
            ReadExport::File(read) => read.call(&mut *store, (address, length, max_pixels)),
            ReadExport::Pixels(read, [width, height, format]) => read.call(
                &mut *store,
                (address, length, *width, *height, *format, max_pixels),
            ),
        };
        let code = match returned {
            Ok(code) => code as u32,
            Err(error) => return self.after_failure(store, exports, error),
        };
        let Some(name) = self.abi.status_name(code) else {
            return Outcome::broken(format!("status {code} is not in the interface"));
        };
        if name != "ok" && name != "partial" {
            return Outcome::status(name);
        }
        let name = name.to_string();
        match self.read_result(store, exports, input.kind()) {
            Ok(result) => Outcome {
                status: name,
                result: Some(result),
                fault: None,
            },
            Err(outcome) => outcome,
        }
    }

    // A trap is how a decoder gives up on a damaged file. The module records
    // whether it followed a failed allocation. Anything else that leaves the
    // module, such as an uncaught exception, breaks the interface.
    fn after_failure(
        &self,
        store: &mut Store<StoreLimits>,
        exports: &Exports,
        error: wasmtime::Error,
    ) -> Outcome {
        if interrupted(&error) {
            return Outcome::status("timedOut");
        }
        if error.downcast_ref::<Trap>().is_none() {
            return Outcome::broken(format!("the read failed without a trap: {error:#}"));
        }
        match exports.out_of_memory.call(&mut *store, ()) {
            Ok(0) => Outcome::status("unreadable"),
            Ok(_) => Outcome::status("outOfMemory"),
            Err(error) if interrupted(&error) => Outcome::status("timedOut"),
            Err(_) => Outcome::status("unreadable"),
        }
    }

    // The length is checked before anything is read. A result outside the
    // memory, over the limit, not JSON or not the documented one breaks the
    // interface, as does a module that fails when asked where its result is.
    fn read_result(
        &self,
        store: &mut Store<StoreLimits>,
        exports: &Exports,
        kind: ReadKind,
    ) -> Result<Value, Outcome> {
        let location = |store: &mut Store<StoreLimits>, func: &TypedFunc<(), i32>| {
            func.call(store, ())
                .map(|value| value as u32 as usize)
                .map_err(|error| {
                    if interrupted(&error) {
                        Outcome::status("timedOut")
                    } else {
                        Outcome::broken("the module failed when asked for its result")
                    }
                })
        };
        let pointer = location(store, &exports.result_ptr)?;
        let length = location(store, &exports.result_len)?;
        if length > self.abi.limits.max_result_bytes {
            return Err(Outcome::broken(format!(
                "a result of {length} bytes is over the limit"
            )));
        }
        let data = exports.memory.data(&*store);
        let Some(bytes) = pointer
            .checked_add(length)
            .and_then(|end| data.get(pointer..end))
        else {
            return Err(Outcome::broken("the result is outside the memory"));
        };
        let parsed: Value = std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| serde_json::from_str(text).ok())
            .ok_or_else(|| Outcome::broken("the result is not JSON"))?;
        self.shaped(kind, &parsed)
            .ok_or_else(|| Outcome::broken("the result is not the documented one"))
    }

    // The documented result for this kind of read, rebuilt from its own
    // fields, or None for any other JSON.
    fn shaped(&self, kind: ReadKind, parsed: &Value) -> Option<Value> {
        let codes = texts(parsed.get("codes")?)?;
        if kind != ReadKind::Pdf {
            return Some(json!({ "codes": codes }));
        }
        let metadata = texts(parsed.get("metadata")?)?;
        // No more pages than the module renders.
        let pages = parsed
            .get("pages")?
            .as_u64()
            .filter(|pages| *pages <= u64::from(self.abi.limits.max_pdf_pages))?;
        Some(json!({ "metadata": metadata, "codes": codes, "pages": pages }))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}

fn typed_func<Params: WasmParams, Results: WasmResults>(
    store: &mut Store<StoreLimits>,
    instance: &Instance,
    name: &str,
) -> Result<TypedFunc<Params, Results>, String> {
    instance.get_typed_func(store, name).map_err(|error| {
        format!("the module has no export {name} of the interface's type: {error:#}")
    })
}

fn interrupted(error: &wasmtime::Error) -> bool {
    error.downcast_ref::<Trap>() == Some(&Trap::Interrupt)
}

fn texts(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|text| text.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_bytes_as_the_squarest_greyscale_image_they_fill() {
        let Input::Pixels(pixels, shape) = Input::of(ReadKind::Pixels, &[0; 11]) else {
            panic!("not a pixel read");
        };
        assert_eq!((pixels.len(), shape.width, shape.height), (9, 3, 3));
        assert_eq!(shape.format, "gray8");

        let Input::Pixels(pixels, shape) = Input::of(ReadKind::Pixels, &[]) else {
            panic!("not a pixel read");
        };
        assert_eq!((pixels.len(), shape.width, shape.height), (0, 0, 0));
    }

    #[test]
    fn reads_a_file_only_up_to_the_limit() {
        let path = std::env::temp_dir().join(format!("vf-bounded-{}", std::process::id()));
        std::fs::write(&path, b"12345").unwrap();
        assert_eq!(read_bounded(&path, 5).unwrap(), Some(b"12345".to_vec()));
        assert_eq!(read_bounded(&path, 4).unwrap(), None);
        std::fs::remove_file(path).unwrap();
    }
}
