//! Finds `payload` properties in a PDF's XMP metadata streams by scanning its
//! bytes: issuers write metadata with their own tools, so a stream counts
//! whether or not the catalog refers to it.
//!
//! The PDF is untrusted: the scan only moves forward, so it is linear in the
//! file's size; decoded metadata and the payloads found have budgets for the
//! whole PDF; DTDs are refused.

use std::borrow::Cow;

/// A metadata stream larger than this, before or after inflating, is skipped.
const MAX_METADATA_BYTES: usize = 1024 * 1024;
/// Decoded metadata for the whole PDF. A payslip has one small packet.
const MAX_TOTAL_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_STREAMS: usize = 64;
/// Payload properties in the whole PDF, empty ones included. A payslip has one, perhaps repeated by each save.
const MAX_PAYLOADS: usize = 256;
/// Payload text in the whole PDF: more could not fit in a result.
const MAX_PAYLOAD_BYTES: usize = crate::MAX_RESULT_BYTES;
/// How far before a `stream` keyword its dictionary may start.
const MAX_DICTIONARY_BYTES: usize = 4 * 1024;
/// Whitespace allowed between a stream's declared end and `endstream`.
const MAX_END_STREAM_GAP: usize = 32;

const PAYLOAD_NAMESPACE: &str = "https://verifiabl.io/ns/";
const PAYLOAD_PROPERTY: &str = "payload";

/// More metadata than a payslip needs.
#[derive(Debug, PartialEq, Eq)]
pub struct TooMuchMetadata;

/// Every non-empty, trimmed `payload` value of at most `MAX_PAYLOAD_LENGTH`
/// UTF-16 code units, in file order. Longer values still count against the
/// budgets.
pub fn payloads(pdf: &[u8]) -> Result<Vec<String>, TooMuchMetadata> {
    let mut scanner = StreamScanner::new(pdf);
    let mut found = Payloads::default();
    let mut streams = 0;
    let mut decoded_bytes = 0;
    while let Some((dictionary, data)) = scanner.next_metadata_stream() {
        streams += 1;
        if streams > MAX_METADATA_STREAMS {
            return Err(TooMuchMetadata);
        }
        if let Some(xmp) = decode(dictionary, data, &mut decoded_bytes)? {
            payloads_in_xmp(&xmp, &mut found)?;
        }
    }
    Ok(found.values)
}

/// The payloads found so far, within their budget.
#[derive(Default)]
struct Payloads {
    values: Vec<String>,
    bytes: usize,
    properties: usize,
}

impl Payloads {
    /// Counts one more `payload` property. Called before an element's text is
    /// collected, so nested elements without text are bounded too: each costs
    /// a walk of everything inside it.
    fn count_property(&mut self) -> Result<(), TooMuchMetadata> {
        self.properties += 1;
        if self.properties > MAX_PAYLOADS {
            return Err(TooMuchMetadata);
        }
        Ok(())
    }

    /// Keeps `value`, trimmed, unless it is empty or too long to be a payload.
    fn push(&mut self, value: &str) -> Result<(), TooMuchMetadata> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.bytes += trimmed.len();
        if self.bytes > MAX_PAYLOAD_BYTES {
            return Err(TooMuchMetadata);
        }
        // The SDKs measure text in UTF-16 code units, as the API does.
        if trimmed.encode_utf16().count() <= crate::MAX_PAYLOAD_LENGTH {
            self.values.push(trimmed.to_owned());
        }
        Ok(())
    }
}

/// The stream's data, decoded, or `None` for an unsupported filter or invalid data.
fn decode(
    dictionary: &[u8],
    data: &[u8],
    decoded_bytes: &mut usize,
) -> Result<Option<Vec<u8>>, TooMuchMetadata> {
    let decoded = if find_name(dictionary, b"/Filter").is_none() {
        *decoded_bytes += data.len();
        Some(data.to_vec())
    } else if is_flate_only(dictionary) && find(dictionary, b"/Predictor", 0).is_none() {
        let (inflated, work) = inflate(data);
        *decoded_bytes += work;
        inflated
    } else {
        // Other filters and predictors do not occur in XMP metadata in practice.
        return Ok(None);
    };
    if *decoded_bytes > MAX_TOTAL_METADATA_BYTES {
        return Err(TooMuchMetadata);
    }
    Ok(decoded)
}

/// zlib data inflated, or `None` when invalid or too large; and the work done either way.
fn inflate(zlib: &[u8]) -> (Option<Vec<u8>>, usize) {
    const ZLIB_HEADER_LENGTH: usize = 2;
    if zlib.len() <= ZLIB_HEADER_LENGTH {
        return (None, 0);
    }
    match miniz_oxide::inflate::decompress_to_vec_with_limit(
        &zlib[ZLIB_HEADER_LENGTH..],
        MAX_METADATA_BYTES,
    ) {
        Ok(inflated) => {
            let length = inflated.len();
            (Some(inflated), length)
        }
        Err(error) => {
            let work = error.output.len();
            (None, work)
        }
    }
}

/// Adds the payloads in one XMP packet. A `payload` element's value is all the
/// text inside it, so nested elements each walk and repeat what is inside
/// them: the budgets in `Payloads` are what keep that from growing with the
/// square of the packet's size.
fn payloads_in_xmp(xmp: &[u8], found: &mut Payloads) -> Result<(), TooMuchMetadata> {
    let Some(text) = xmp_text(xmp) else {
        return Ok(());
    };
    // DTDs can expand entities; XMP never needs one.
    if text.contains("<!DOCTYPE") {
        return Ok(());
    }
    let Ok(tree) = roxmltree::Document::parse(&text) else {
        return Ok(());
    };
    for node in tree.descendants().filter(roxmltree::Node::is_element) {
        for attribute in node.attributes() {
            if attribute.namespace() == Some(PAYLOAD_NAMESPACE)
                && attribute.name() == PAYLOAD_PROPERTY
            {
                found.count_property()?;
                found.push(attribute.value())?;
            }
        }
        if node.tag_name().namespace() == Some(PAYLOAD_NAMESPACE)
            && node.tag_name().name() == PAYLOAD_PROPERTY
        {
            found.count_property()?;
            let content: String = node
                .descendants()
                .filter(roxmltree::Node::is_text)
                .filter_map(|text| text.text())
                .collect();
            found.push(&content)?;
        }
    }
    Ok(())
}

/// The packet's text, or `None` when it is neither UTF-8 nor UTF-16. Some
/// tools write XMP as UTF-16, which starts with a byte order mark or with `<`
/// as a 16-bit unit.
fn xmp_text(xmp: &[u8]) -> Option<Cow<'_, str>> {
    let text = match xmp {
        [0xFF, 0xFE, rest @ ..] => Cow::Owned(utf16_text(rest, u16::from_le_bytes)?),
        [0xFE, 0xFF, rest @ ..] => Cow::Owned(utf16_text(rest, u16::from_be_bytes)?),
        [b'<', 0, ..] => Cow::Owned(utf16_text(xmp, u16::from_le_bytes)?),
        [0, b'<', ..] => Cow::Owned(utf16_text(xmp, u16::from_be_bytes)?),
        _ => Cow::Borrowed(std::str::from_utf8(xmp).ok()?),
    };
    Some(text)
}

/// UTF-16 bytes as text, or `None` for an odd length or an unpaired surrogate.
fn utf16_text(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Option<String> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    let units = pairs.iter().map(|&pair| unit(pair));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

/// Walks a PDF's stream objects front to back. Every search moves forward and
/// each dictionary is read from bytes no earlier dictionary used.
struct StreamScanner<'a> {
    pdf: &'a [u8],
    position: usize,
    dictionary_floor: usize,
    /// The last `endstream` found and where that search started, so later searches never rescan.
    end_stream: Option<(usize, Option<usize>)>,
}

impl<'a> StreamScanner<'a> {
    fn new(pdf: &'a [u8]) -> Self {
        StreamScanner {
            pdf,
            position: 0,
            dictionary_floor: 0,
            end_stream: None,
        }
    }

    fn next_metadata_stream(&mut self) -> Option<(&'a [u8], &'a [u8])> {
        loop {
            let keyword = find(self.pdf, b"stream", self.position)?;
            self.position = keyword + b"stream".len();
            let Some(data_start) = self.stream_data_start(self.position) else {
                continue;
            };
            if keyword >= 3 && self.pdf[keyword - 3..].starts_with(b"endstream") {
                continue;
            }
            let candidate = self.dictionary_before(keyword);
            self.dictionary_floor = self.position;
            let Some(dictionary) = candidate else {
                continue;
            };
            if !is_metadata_type(dictionary) {
                continue;
            }
            let Some(data) = self.stream_data(dictionary, data_start) else {
                continue;
            };
            self.position = data_start + data.len();
            self.dictionary_floor = self.position;
            return Some((dictionary, data));
        }
    }

    fn stream_data_start(&self, after_keyword: usize) -> Option<usize> {
        match (self.pdf.get(after_keyword), self.pdf.get(after_keyword + 1)) {
            (Some(b'\n'), _) => Some(after_keyword + 1),
            (Some(b'\r'), Some(b'\n')) => Some(after_keyword + 2),
            _ => None,
        }
    }

    /// The bytes between the last `obj` keyword before `keyword` and the keyword.
    fn dictionary_before(&self, keyword: usize) -> Option<&'a [u8]> {
        let start = self
            .dictionary_floor
            .max(keyword.saturating_sub(MAX_DICTIONARY_BYTES));
        let window = self.pdf.get(start..keyword)?;
        let obj = window.windows(3).rposition(|bytes| bytes == b"obj")?;
        Some(&window[obj + 3..])
    }

    fn stream_data(&mut self, dictionary: &[u8], data_start: usize) -> Option<&'a [u8]> {
        if let Some(declared) = direct_length(dictionary) {
            if declared <= MAX_METADATA_BYTES
                && declared <= self.pdf.len() - data_start
                && self.is_at_end_stream(data_start + declared)
            {
                return Some(&self.pdf[data_start..data_start + declared]);
            }
        }
        // An indirect or wrong /Length: fall back to the endstream keyword.
        let end_stream = self.next_end_stream(data_start)?;
        let mut data_end = end_stream;
        if data_end > data_start && self.pdf[data_end - 1] == b'\n' {
            data_end -= 1;
        }
        if data_end > data_start && self.pdf[data_end - 1] == b'\r' {
            data_end -= 1;
        }
        let size = data_end.saturating_sub(data_start);
        (size <= MAX_METADATA_BYTES).then(|| &self.pdf[data_start..data_start + size])
    }

    fn is_at_end_stream(&self, mut offset: usize) -> bool {
        let limit = self.pdf.len().min(offset + MAX_END_STREAM_GAP);
        while offset < limit && is_whitespace(self.pdf[offset]) {
            offset += 1;
        }
        self.pdf[offset.min(self.pdf.len())..].starts_with(b"endstream")
    }

    fn next_end_stream(&mut self, from: usize) -> Option<usize> {
        // Searches start at increasing offsets, so a cached miss or a match at
        // or after `from` still answers this one.
        let cached = matches!(self.end_stream, Some((searched_from, next)) if searched_from <= from && next.is_none_or(|at| at >= from));
        if !cached {
            self.end_stream = Some((from, find(self.pdf, b"endstream", from)));
        }
        self.end_stream.and_then(|(_, next)| next)
    }
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|bytes| bytes == needle)
        .map(|index| index + from)
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0C | 0)
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_whitespace(text: &[u8], mut index: usize) -> usize {
    while index < text.len() && is_whitespace(text[index]) {
        index += 1;
    }
    index
}

/// The offset after `name` where it is not followed by a word character.
fn find_name(text: &[u8], name: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = find(text, name, from) {
        let end = at + name.len();
        if text.get(end).is_none_or(|&next| !is_word(next)) {
            return Some(end);
        }
        from = at + 1;
    }
    None
}

/// `/Type /Metadata`, with any whitespace between.
fn is_metadata_type(dictionary: &[u8]) -> bool {
    let mut from = 0;
    while let Some(at) = find(dictionary, b"/Type", from) {
        let value = skip_whitespace(dictionary, at + b"/Type".len());
        let end = value + b"/Metadata".len();
        if dictionary[value..].starts_with(b"/Metadata")
            && dictionary.get(end).is_none_or(|&next| !is_word(next))
        {
            return true;
        }
        from = at + 1;
    }
    false
}

/// `/Filter /FlateDecode` or `/Filter [/FlateDecode]`.
fn is_flate_only(dictionary: &[u8]) -> bool {
    let Some(after) = find(dictionary, b"/Filter", 0)
        .map(|at| skip_whitespace(dictionary, at + b"/Filter".len()))
    else {
        return false;
    };
    let rest = &dictionary[after..];
    if rest.starts_with(b"/FlateDecode") {
        return true;
    }
    if !rest.starts_with(b"[") {
        return false;
    }
    let name = skip_whitespace(rest, 1);
    if !rest[name..].starts_with(b"/FlateDecode") {
        return false;
    }
    let close = skip_whitespace(rest, name + b"/FlateDecode".len());
    rest.get(close) == Some(&b']')
}

/// A direct `/Length n`, not an indirect `/Length n g R`.
fn direct_length(dictionary: &[u8]) -> Option<usize> {
    let at = find(dictionary, b"/Length", 0)?;
    let start = skip_whitespace(dictionary, at + b"/Length".len());
    if start == at + b"/Length".len() {
        return None;
    }
    let digits = dictionary[start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let after = start + digits;
    let generation = skip_whitespace(dictionary, after);
    let generation_digits = dictionary[generation..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if generation > after && generation_digits > 0 {
        let reference = skip_whitespace(dictionary, generation + generation_digits);
        if reference > generation + generation_digits && dictionary.get(reference) == Some(&b'R') {
            return None;
        }
    }
    std::str::from_utf8(&dictionary[start..after])
        .ok()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XMP_OPEN: &str =
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:v="https://verifiabl.io/ns/">"#;
    const XMP_CLOSE: &str = "</x:xmpmeta>";
    const BOM: char = '\u{FEFF}';

    /// A PDF fragment holding one uncompressed metadata stream per XMP body.
    fn pdf_with(bodies: &[String]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        for (index, body) in bodies.iter().enumerate() {
            let xmp = format!("{XMP_OPEN}{body}{XMP_CLOSE}");
            let header = format!(
                "{} 0 obj\n<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n",
                index + 1,
                xmp.len()
            );
            pdf.extend_from_slice(header.as_bytes());
            pdf.extend_from_slice(xmp.as_bytes());
            pdf.extend_from_slice(b"\nendstream\nendobj\n");
        }
        pdf
    }

    fn nested(depth: usize, text: &str) -> String {
        format!(
            "{}{text}{}",
            "<v:payload>".repeat(depth),
            "</v:payload>".repeat(depth)
        )
    }

    #[test]
    fn finds_element_and_attribute_payloads_in_file_order() {
        let pdf = pdf_with(&[
            "<v:payload> 2|ref|one </v:payload>".to_owned(),
            r#"<rdf:Description xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" v:payload="2|ref|two"/>"#.to_owned(),
        ]);
        assert_eq!(
            payloads(&pdf),
            Ok(vec!["2|ref|one".to_owned(), "2|ref|two".to_owned()])
        );
    }

    #[test]
    fn ignores_other_namespaces_empty_values_and_dtds() {
        let pdf = pdf_with(&[
            r#"<o:payload xmlns:o="https://example.com/ns/">2|ref|other</o:payload>"#.to_owned(),
            "<v:payload>   </v:payload>".to_owned(),
        ]);
        assert_eq!(payloads(&pdf), Ok(Vec::new()));

        let dtd = b"1 0 obj\n<< /Type /Metadata /Length 0 >>\nstream\n<!DOCTYPE x [<!ENTITY e \"2|ref|dtd\">]><v:payload xmlns:v=\"https://verifiabl.io/ns/\">&e;</v:payload>\nendstream";
        assert_eq!(payloads(dtd), Ok(Vec::new()));
    }

    /// A PDF fragment holding one uncompressed metadata stream of these bytes.
    fn pdf_with_packet(packet: &[u8]) -> Vec<u8> {
        let mut pdf = format!(
            "%PDF-1.7\n1 0 obj\n<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n",
            packet.len()
        )
        .into_bytes();
        pdf.extend_from_slice(packet);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
        pdf
    }

    fn utf16(text: &str, unit: fn(u16) -> [u8; 2]) -> Vec<u8> {
        text.encode_utf16().flat_map(unit).collect()
    }

    #[test]
    fn reads_utf16_packets_in_either_byte_order() {
        let packet = format!(
            r#"<?xpacket begin="{BOM}" id="W5M0MpCehiHzreSzNTczkc9d"?>{XMP_OPEN}<v:payload>2|ref|é</v:payload>{XMP_CLOSE}<?xpacket end="w"?>"#
        );
        let found = Ok(vec!["2|ref|é".to_owned()]);
        for unit in [u16::to_le_bytes, u16::to_be_bytes] {
            assert_eq!(payloads(&pdf_with_packet(&utf16(&packet, unit))), found);

            let marked = format!("{BOM}{packet}");
            assert_eq!(payloads(&pdf_with_packet(&utf16(&marked, unit))), found);
        }
    }

    #[test]
    fn skips_utf16_packets_that_are_damaged_or_hold_a_dtd() {
        let packet = format!("{XMP_OPEN}<v:payload>2|ref|ct</v:payload>{XMP_CLOSE}");
        let mut odd = utf16(&packet, u16::to_le_bytes);
        odd.pop();
        assert_eq!(payloads(&pdf_with_packet(&odd)), Ok(Vec::new()));

        // An unpaired high surrogate where the payload's first character was.
        let mut unpaired = utf16(&packet, u16::to_le_bytes);
        let payload = 2 * packet.find("2|ref").expect("the packet holds the payload");
        unpaired[payload..payload + 2].copy_from_slice(&0xD800u16.to_le_bytes());
        assert_eq!(payloads(&pdf_with_packet(&unpaired)), Ok(Vec::new()));

        let dtd = format!(
            r#"<!DOCTYPE x [<!ENTITY e "2|ref|dtd">]><v:payload xmlns:v="{PAYLOAD_NAMESPACE}">&e;</v:payload>"#
        );
        assert_eq!(
            payloads(&pdf_with_packet(&utf16(&dtd, u16::to_le_bytes))),
            Ok(Vec::new())
        );
    }

    #[test]
    fn reads_a_stream_that_is_not_a_whole_pdf() {
        assert_eq!(payloads(b""), Ok(Vec::new()));
        assert_eq!(payloads(b"stream"), Ok(Vec::new()));
        assert_eq!(
            payloads(b"obj<< /Type /Metadata /Length 9 >>stream\n"),
            Ok(Vec::new())
        );
    }

    #[test]
    fn refuses_more_payload_values_than_the_budget() {
        // Each level's value repeats the text below it, so without the budget a
        // 1 MiB packet would be read tens of thousands of times over.
        let pdf = pdf_with(&[nested(MAX_PAYLOADS + 1, "x")]);
        assert_eq!(payloads(&pdf), Err(TooMuchMetadata));

        let within = pdf_with(&[nested(MAX_PAYLOADS, "x")]);
        assert_eq!(payloads(&within).map(|found| found.len()), Ok(MAX_PAYLOADS));
    }

    #[test]
    fn counts_payload_elements_without_text_against_the_budget() {
        // Nothing to keep, but each level is a walk of the levels inside it.
        let empty = pdf_with(&[nested(MAX_PAYLOADS + 1, "")]);
        assert_eq!(payloads(&empty), Err(TooMuchMetadata));

        let blank = pdf_with(&[nested(MAX_PAYLOADS + 1, " ")]);
        assert_eq!(payloads(&blank), Err(TooMuchMetadata));

        let within = pdf_with(&[nested(MAX_PAYLOADS, "")]);
        assert_eq!(payloads(&within), Ok(Vec::new()));
    }

    #[test]
    fn refuses_more_payload_text_than_a_result_holds() {
        let half = "A".repeat(MAX_PAYLOAD_BYTES / 2 + 1);
        let pdf = pdf_with(&[
            format!("<v:payload>{half}</v:payload>"),
            format!("<v:payload>{half}</v:payload>"),
        ]);
        assert_eq!(payloads(&pdf), Err(TooMuchMetadata));

        // A few levels over a long text: 300 kB of packet, 90 MB of values.
        let amplified = pdf_with(&[nested(300, &"C".repeat(300_000))]);
        assert_eq!(payloads(&amplified), Err(TooMuchMetadata));
    }

    #[test]
    fn skips_values_too_long_to_be_a_payload() {
        let longest = "A".repeat(crate::MAX_PAYLOAD_LENGTH);
        let too_long = "B".repeat(crate::MAX_PAYLOAD_LENGTH + 1);
        let pdf = pdf_with(&[
            format!("<v:payload>{too_long}</v:payload>"),
            format!("<v:payload>{longest}</v:payload>"),
        ]);
        assert_eq!(payloads(&pdf), Ok(vec![longest.clone()]));

        // Measured in UTF-16 code units: each of these characters takes two.
        let wide = "\u{1F600}".repeat(crate::MAX_PAYLOAD_LENGTH / 2 + 1);
        let pdf = pdf_with(&[format!("<v:payload>{wide}</v:payload>")]);
        assert_eq!(payloads(&pdf), Ok(Vec::new()));
    }

    #[test]
    fn refuses_more_streams_than_the_budget() {
        let bodies = vec!["<v:payload>2|ref|ct</v:payload>".to_owned(); MAX_METADATA_STREAMS + 1];
        assert_eq!(payloads(&pdf_with(&bodies)), Err(TooMuchMetadata));
    }
}
