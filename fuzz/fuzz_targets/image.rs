#![no_main]

libfuzzer_sys::fuzz_target!(|input: &[u8]| verifiabl_extract_fuzz::image::check(input));
