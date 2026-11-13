//! The module's exports. The host calls `vf_alloc` for the file or the
//! pixels, writes them, then calls `vf_read_image`, `vf_read_pdf` or
//! `vf_read_pixels`. On `OK` or `PARTIAL`, `vf_result_ptr` and
//! `vf_result_len` hold the result as UTF-8 JSON of at most `MAX_RESULT_BYTES`.

use std::alloc::{GlobalAlloc, Layout, System};

use crate::image::{self, Failure};
use crate::{json, pdf, qr, MAX_RESULT_BYTES};

/// Records a failed allocation before Rust aborts with a trap, so the host
/// can tell the memory cap apart from a damaged file.
struct FlaggingAllocator;

static mut OUT_OF_MEMORY: bool = false;

unsafe impl GlobalAlloc for FlaggingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        flag_failure(System.alloc(layout))
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        flag_failure(System.alloc_zeroed(layout))
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout)
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        flag_failure(System.realloc(pointer, layout, size))
    }
}

unsafe fn flag_failure(pointer: *mut u8) -> *mut u8 {
    if pointer.is_null() {
        OUT_OF_MEMORY = true;
    }
    pointer
}

#[global_allocator]
static ALLOCATOR: FlaggingAllocator = FlaggingAllocator;

const OK: i32 = 0;
/// The header was valid but the file ended early.
/// The rows decoded so far are kept and the rest are white.
const PARTIAL: i32 = 1;
const UNREADABLE: i32 = 2;
const TOO_MANY_PIXELS: i32 = 3;
/// A decoder ran out of memory and reported it, rather than trapping.
const OUT_OF_MEMORY_STATUS: i32 = 4;
/// A PDF has more than `pdf::MAX_PAGES` pages and no metadata payload.
const TOO_MANY_PAGES: i32 = 5;
/// A PDF holds more XMP metadata than a payslip needs.
const TOO_MUCH_METADATA: i32 = 6;
/// The QR codes hold more text than `MAX_RESULT_BYTES`.
const RESULT_TOO_LARGE: i32 = 7;

static mut RESULT: Vec<u8> = Vec::new();
static mut INITIALIZED: bool = false;

extern "C" {
    fn __wasm_call_ctors();
}

/// Runs the C++ static constructors once. Referencing `__wasm_call_ctors`
/// also stops wasm-ld wrapping every export to run them on each call.
fn initialize() {
    unsafe {
        if !INITIALIZED {
            INITIALIZED = true;
            __wasm_call_ctors();
        }
    }
}

/// The version of `abi/extraction-abi.json` this module implements.
const ABI_VERSION: u32 = 3;

#[no_mangle]
pub extern "C" fn vf_abi_version() -> u32 {
    ABI_VERSION
}

/// Allocates `len` bytes for the host to write the file or the pixels into.
#[no_mangle]
pub extern "C" fn vf_alloc(len: u32) -> *mut u8 {
    initialize();
    let mut buffer = vec![0u8; len as usize];
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

// The statics are safe to share: an instance is single-threaded and the host
// discards it after one read.

/// Whether an allocation failed; read by the host after a trap.
#[no_mangle]
pub extern "C" fn vf_out_of_memory() -> i32 {
    unsafe { OUT_OF_MEMORY as i32 }
}

/// Decodes the file and reads its QR codes; on `OK` or `PARTIAL`, `vf_result_ptr`
/// and `vf_result_len` hold `{"codes":[...]}` in UTF-8.
///
/// # Safety
///
/// `file` must point to `len` bytes, as returned by `vf_alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn vf_read_image(file: *const u8, len: u32, max_pixels: u32) -> i32 {
    initialize();
    let file = unsafe { std::slice::from_raw_parts(file, len as usize) };
    let decoded = match image::decode_file(file, max_pixels as u64) {
        Ok(decoded) => decoded,
        Err(failure) => return status_of(failure),
    };
    let status = if decoded.complete { OK } else { PARTIAL };
    read_codes(&decoded.gray, decoded.width, decoded.height, status)
}

/// Reads the QR codes in pixels that the host's caller decoded, with the
/// reader that `vf_read_image` uses; on `OK`, `vf_result_ptr` and
/// `vf_result_len` hold `{"codes":[...]}` in UTF-8. `format` is a code of
/// `pixelFormats` in `abi/extraction-abi.json`. The pixels become greyscale
/// in place.
///
/// # Safety
///
/// `pixels` must point to `len` bytes, as returned by `vf_alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn vf_read_pixels(
    pixels: *mut u8,
    len: u32,
    width: u32,
    height: u32,
    format: u32,
    max_pixels: u32,
) -> i32 {
    initialize();
    let pixels = unsafe { std::slice::from_raw_parts_mut(pixels, len as usize) };
    match image::gray_pixels(pixels, width, height, format, max_pixels as u64) {
        Ok(gray) => read_codes(gray, width, height, OK),
        Err(failure) => status_of(failure),
    }
}

fn status_of(failure: Failure) -> i32 {
    match failure {
        Failure::Unreadable => UNREADABLE,
        Failure::TooManyPixels => TOO_MANY_PIXELS,
    }
}

/// Publishes the QR codes in a greyscale image and returns `status`, or the
/// status of the failure.
fn read_codes(gray: &[u8], width: u32, height: u32, status: i32) -> i32 {
    match qr::read_codes(gray, width, height) {
        Ok(codes) => publish(json::codes(&codes), status, RESULT_TOO_LARGE),
        Err(qr::QrFailure::OutOfMemory) => OUT_OF_MEMORY_STATUS,
        Err(qr::QrFailure::Failed) => UNREADABLE,
    }
}

/// Reads a PDF's metadata payloads or, when it has none, the QR codes on its
/// pages; on `OK`, the result holds `{"metadata":[...],"codes":[...],"pages":n}`.
///
/// # Safety
///
/// `file` must point to `len` bytes, as returned by `vf_alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn vf_read_pdf(file: *const u8, len: u32, max_pixels: u32) -> i32 {
    initialize();
    let file = unsafe { std::slice::from_raw_parts(file, len as usize) };
    match pdf::read(file, max_pixels as u64) {
        Ok(payloads) => {
            let too_large = if payloads.metadata.is_empty() {
                RESULT_TOO_LARGE
            } else {
                TOO_MUCH_METADATA
            };
            let result = json::pdf(&payloads.metadata, &payloads.codes, payloads.pages);
            publish(result, OK, too_large)
        }
        Err(pdf::PdfFailure::Unreadable) => UNREADABLE,
        Err(pdf::PdfFailure::TooManyPages) => TOO_MANY_PAGES,
        Err(pdf::PdfFailure::TooMuchMetadata) => TOO_MUCH_METADATA,
        Err(pdf::PdfFailure::LimitExceeded) => OUT_OF_MEMORY_STATUS,
        Err(pdf::PdfFailure::ResultTooLarge) => RESULT_TOO_LARGE,
    }
}

/// Keeps `result` for the host and returns `status`, or returns `too_large`
/// when it is more than a host reads. Escaping can make the JSON larger than
/// the text it holds, so this is the check that binds.
fn publish(result: Vec<u8>, status: i32, too_large: i32) -> i32 {
    if result.len() > MAX_RESULT_BYTES {
        return too_large;
    }
    unsafe { RESULT = result };
    status
}

#[no_mangle]
pub extern "C" fn vf_result_ptr() -> *const u8 {
    unsafe { (*std::ptr::addr_of!(RESULT)).as_ptr() }
}

#[no_mangle]
pub extern "C" fn vf_result_len() -> u32 {
    unsafe { (*std::ptr::addr_of!(RESULT)).len() as u32 }
}
