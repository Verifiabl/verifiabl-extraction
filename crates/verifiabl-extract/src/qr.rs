//! Reads QR codes with ZXing-C++, through native/zxing_shim.cpp.

use std::ffi::c_void;

extern "C" {
    fn vf_zxing_read(gray: *const u8, width: i32, height: i32, context: *mut c_void) -> i32;
}

const OUT_OF_MEMORY: i32 = -2;

pub enum QrFailure {
    Failed,
    OutOfMemory,
}

/// The text of every valid QR code in a greyscale image, in either polarity.
pub fn read_codes(gray: &[u8], width: u32, height: u32) -> Result<Vec<String>, QrFailure> {
    let mut codes: Vec<String> = Vec::new();
    let context = &mut codes as *mut Vec<String> as *mut c_void;
    let status = unsafe { vf_zxing_read(gray.as_ptr(), width as i32, height as i32, context) };
    match status {
        0 => Ok(codes),
        OUT_OF_MEMORY => Err(QrFailure::OutOfMemory),
        _ => Err(QrFailure::Failed),
    }
}

#[no_mangle]
unsafe extern "C" fn vf_zxing_emit(context: *mut c_void, text: *const u8, length: usize) {
    let codes = &mut *(context as *mut Vec<String>);
    let bytes = std::slice::from_raw_parts(text, length);
    // Payloads are ASCII; a code that is not UTF-8 cannot be one.
    if let Ok(text) = std::str::from_utf8(bytes) {
        codes.push(text.to_owned());
    }
}
