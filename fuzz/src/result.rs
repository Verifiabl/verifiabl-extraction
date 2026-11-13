//! The result writer. Hosts parse what it writes, and its texts come from the
//! file, so any text must come back from a JSON parser unchanged.

use serde_json::{json, Value};
use verifiabl_extract::json;

const PAGES: i32 = 7;

pub fn check(data: &[u8]) {
    // Texts are separated by 0xFF, which no UTF-8 text contains.
    let texts: Vec<String> = data
        .split(|&byte| byte == 0xFF)
        .map(|text| String::from_utf8_lossy(text).into_owned())
        .collect();
    let (metadata, codes) = texts.split_at(texts.len() / 2);

    let pdf: Value =
        serde_json::from_slice(&json::pdf(metadata, codes, PAGES)).expect("a PDF result is JSON");
    assert_eq!(
        pdf,
        json!({ "metadata": metadata, "codes": codes, "pages": PAGES })
    );

    let image: Value =
        serde_json::from_slice(&json::codes(codes)).expect("an image result is JSON");
    assert_eq!(image, json!({ "codes": codes }));
}
