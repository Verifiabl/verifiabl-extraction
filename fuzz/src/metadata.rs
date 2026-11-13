//! The PDF metadata scanner, which reads untrusted bytes with rules of our
//! own. It must not panic or run long, and what it finds must fit in a result.

use verifiabl_extract::{metadata::payloads, MAX_PAYLOAD_LENGTH, MAX_RESULT_BYTES};

pub fn check(pdf: &[u8]) {
    if let Ok(payloads) = payloads(pdf) {
        let bytes: usize = payloads.iter().map(String::len).sum();
        assert!(
            bytes <= MAX_RESULT_BYTES,
            "more payload text than a result holds"
        );
        for payload in &payloads {
            assert!(
                !payload.is_empty() && payload.trim() == payload,
                "a payload is not trimmed"
            );
            assert!(
                payload.encode_utf16().count() <= MAX_PAYLOAD_LENGTH,
                "a payload is too long"
            );
        }
    }
}
