//! Writes the results the host reads, described in `abi/extraction-abi.json`.

/// `{"codes":[...]}`, with each code escaped as a JSON string.
pub fn codes(codes: &[String]) -> Vec<u8> {
    let mut json = String::from("{\"codes\":");
    push_array(&mut json, codes);
    json.push('}');
    json.into_bytes()
}

/// `{"metadata":[...],"codes":[...],"pages":n}`.
pub fn pdf(metadata: &[String], codes: &[String], pages: i32) -> Vec<u8> {
    let mut json = String::from("{\"metadata\":");
    push_array(&mut json, metadata);
    json.push_str(",\"codes\":");
    push_array(&mut json, codes);
    json.push_str(&format!(",\"pages\":{pages}}}"));
    json.into_bytes()
}

fn push_array(json: &mut String, texts: &[String]) {
    json.push('[');
    for (index, text) in texts.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        push_string(json, text);
    }
    json.push(']');
}

fn push_string(json: &mut String, text: &str) {
    json.push('"');
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            character if (character as u32) < 0x20 => {
                json.push_str(&format!("\\u{:04x}", character as u32))
            }
            character => json.push(character),
        }
    }
    json.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(json: Vec<u8>) -> String {
        String::from_utf8(json).expect("results are UTF-8")
    }

    #[test]
    fn writes_each_result_shape() {
        let found = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(text(codes(&[])), r#"{"codes":[]}"#);
        assert_eq!(text(codes(&found)), r#"{"codes":["a","b"]}"#);
        assert_eq!(
            text(pdf(&found, &[], 0)),
            r#"{"metadata":["a","b"],"codes":[],"pages":0}"#
        );
    }

    #[test]
    fn escapes_what_json_strings_cannot_hold() {
        let hostile = vec!["\"],\"codes\":[\"x\\\n\u{0}\u{1f}é\u{2028}".to_owned()];
        assert_eq!(
            text(codes(&hostile)),
            "{\"codes\":[\"\\\"],\\\"codes\\\":[\\\"x\\\\\\u000a\\u0000\\u001fé\u{2028}\"]}"
        );
    }
}
