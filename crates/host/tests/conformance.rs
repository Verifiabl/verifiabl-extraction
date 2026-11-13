//! Every host's conformance check: the fixtures read as fixtures/expected.json
//! says, the host enforces the size limits and the time budget, and each
//! stand-in module gives the status in stand-ins/expected.json. The committed
//! module must also import nothing and export only the interface.

mod common;

use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::time::Duration;

use common::{abi, read, Expectation, TIMEOUT};
use serde::Deserialize;
use serde_json::Value;
use verifiabl_extract_host::{Input, PixelShape, ReadKind, Sandbox};
use wasmtime::ExternType;

// Long enough for every stand-in that returns, short for the ones that never do.
const STAND_IN_TIMEOUT: Duration = Duration::from_secs(2);
// One pixel, which no buffer of two bytes or more holds.
const ONE_PIXEL: PixelShape = PixelShape {
    width: 1,
    height: 1,
    format: "gray8",
};

#[derive(Deserialize)]
struct StandIn {
    module: String,
    status: String,
}

// Compiling the module takes seconds, so the tests share one.
fn module() -> &'static Sandbox {
    static SANDBOX: OnceLock<Sandbox> = OnceLock::new();
    SANDBOX.get_or_init(|| Sandbox::new(&read("dist/verifiabl-extract.wasm"), abi()).unwrap())
}

#[test]
fn fixtures_read_as_expected() {
    for expectation in Expectation::all() {
        let name = expectation.name();
        let bytes = read(&format!("fixtures/{}", expectation.file));
        let outcome = module()
            .extract(expectation.input(&bytes), TIMEOUT)
            .unwrap();
        assert_eq!(
            outcome.status, expectation.status,
            "{name}: {:?}",
            outcome.fault
        );
        if expectation.status == "ok" {
            assert_eq!(
                outcome.result,
                Some(Value::Object(expectation.result)),
                "{name}"
            );
        }
    }
}

// The host refuses a file over the limit itself; one at the limit reaches the module.
#[test]
fn host_refuses_a_file_over_the_size_limit() {
    let max = abi().limits.max_file_bytes;
    let oversized = module()
        .extract(Input::Image(&vec![0; max + 1]), TIMEOUT)
        .unwrap();
    assert_eq!(oversized.status, "fileTooLarge");
    let largest = module()
        .extract(Input::Image(&vec![0; max]), TIMEOUT)
        .unwrap();
    assert_eq!(largest.status, "unreadable");
    assert_eq!(largest.fault, None);
}

// The host refuses pixels over the limit itself: the module would report
// these as unreadable, because their length does not match their size. A
// buffer at the limit reaches the module, which reports exactly that.
#[test]
fn host_refuses_pixels_over_the_size_limit() {
    let max = abi().limits.max_pixel_bytes;
    let oversized = module()
        .extract(Input::Pixels(&vec![0; max + 1], ONE_PIXEL), TIMEOUT)
        .unwrap();
    assert_eq!(oversized.status, "tooManyPixels");
    let largest = module()
        .extract(Input::Pixels(&vec![0; max], ONE_PIXEL), TIMEOUT)
        .unwrap();
    assert_eq!(largest.status, "unreadable");
    assert_eq!(largest.fault, None);
}

// The largest pixels the limits allow fit in the module's memory with the QR reader.
#[test]
fn module_reads_the_most_pixels_the_limits_allow() {
    let limits = abi().limits;
    let shape = PixelShape {
        width: limits.max_pixels / 5000,
        height: 5000,
        format: "rgba32",
    };
    let read = module()
        .extract(
            Input::Pixels(&vec![0xFF; limits.max_pixel_bytes], shape),
            TIMEOUT,
        )
        .unwrap();
    assert_eq!((read.status.as_str(), read.fault), ("ok", None));
}

// long-path.pdf inflates to 240 MB before PDFium reads it, which takes far
// longer than the one tick a 1 ms budget gets. A small image can finish first.
#[test]
fn host_stops_a_read_at_its_budget() {
    let slow = read("fixtures/long-path.pdf");
    let stopped = module()
        .extract(Input::Pdf(&slow), Duration::from_millis(1))
        .unwrap();
    assert_eq!(stopped.status, "timedOut");
}

#[test]
fn stand_ins_give_their_status() {
    let stand_ins: Vec<StandIn> = serde_json::from_slice(&read("stand-ins/expected.json")).unwrap();
    // A file small enough for every stand-in's single page of memory.
    let small = read("fixtures/not-a-file.bin");
    for stand_in in stand_ins {
        let sandbox =
            Sandbox::new(&read(&format!("stand-ins/{}", stand_in.module)), abi()).unwrap();
        for kind in ReadKind::ALL {
            let outcome = sandbox
                .extract(Input::of(kind, &small), STAND_IN_TIMEOUT)
                .unwrap();
            assert_eq!(
                (outcome.status.as_str(), outcome.result.is_none()),
                (stand_in.status.as_str(), true),
                "{} ({})",
                stand_in.module,
                kind.name()
            );
        }
    }
}

// The sandbox's guarantee: the module can reach nothing outside its own memory.
#[test]
fn module_imports_nothing() {
    let imports: Vec<String> = module()
        .module()
        .imports()
        .map(|import| format!("{}.{}", import.module(), import.name()))
        .collect();
    assert_eq!(imports, Vec::<String>::new());
}

// Hosts call exports by name, so the module exports the interface and nothing
// a host could mistake for part of it. The linker also exports the WASI calls
// the module defines for itself, and the callback ZXing-C++'s shim calls.
#[test]
fn module_exports_only_the_interface() {
    let abi = abi();
    let mut expected: BTreeSet<String> = abi
        .exports
        .keys()
        .map(|name| format!("func {name}"))
        .collect();
    expected.insert(format!("memory {}", abi.memory.export));
    let actual: BTreeSet<String> = module()
        .module()
        .exports()
        .filter(|export| !is_internal(export.name()))
        .map(|export| match export.ty() {
            ExternType::Func(_) => format!("func {}", export.name()),
            ExternType::Memory(_) => format!("memory {}", export.name()),
            _ => format!("other {}", export.name()),
        })
        .collect();
    assert_eq!(actual, expected);
}

fn is_internal(name: &str) -> bool {
    name == "vf_zxing_emit" || name.starts_with("__imported_wasi_snapshot_preview1_")
}
