//! Reads the Verifiabl payload candidates in an untrusted image or PDF, or in
//! pixels that the host's caller decoded.
//!
//! The module imports nothing, so a bug in a decoder can reach only this
//! instance's linear memory. The host gives each read a new instance with a
//! memory cap and stops it at its time budget.
//!
//! The exports, in `abi.rs`, are described in `abi/extraction-abi.json`.
//! `image`, `json` and `metadata` are plain Rust, so they also build for the
//! host platform, where their tests and fuzz targets run; the rest calls
//! ZXing-C++ and PDFium and builds only as WebAssembly.

pub mod image;
pub mod json;
pub mod metadata;

#[cfg(target_arch = "wasm32")]
mod abi;
#[cfg(target_arch = "wasm32")]
mod pdf;
#[cfg(target_arch = "wasm32")]
mod qr;
#[cfg(target_arch = "wasm32")]
mod wasi_stubs;

/// The largest result a host reads: `maxResultBytes` in `abi/extraction-abi.json`.
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;

/// The longest payload, in UTF-16 code units: `maxPayloadLength` in
/// `abi/extraction-abi.json`, the Verifier API's barcode limit. A longer
/// metadata value cannot be verified, so it does not stop a PDF's pages being read.
pub const MAX_PAYLOAD_LENGTH: usize = 13_000;
