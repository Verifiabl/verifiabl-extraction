//! Metadata streams built from a few numbers: how deep the payload elements
//! nest, how long their text is, how many streams there are, whether each is
//! deflated and whether it is UTF-8 or UTF-16. Byte mutation rarely builds those shapes, and they are the ones
//! that cost far more than their size. Whatever the shape, the scan must stay
//! within its budgets and the time limit.

use verifiabl_extract::{metadata, MAX_RESULT_BYTES};

/// Bytes of input that describe one stream.
const STREAM_BYTES: usize = 6;
/// More than the scanner allows, so its stream limit is reached too.
const MAX_STREAMS: usize = 80;
const MAX_DEPTH: usize = 4096;
/// Just over what one stream or one result may hold.
const MAX_TEXT_BYTES: usize = 1_100_000;

struct Stream {
    depth: usize,
    text_bytes: usize,
    as_attribute: bool,
    deflated: bool,
    direct_length: bool,
    encoding: Encoding,
}

enum Encoding {
    Utf8,
    Utf16LittleEndian,
    Utf16BigEndian,
}

impl Stream {
    fn from(bytes: &[u8; STREAM_BYTES]) -> Stream {
        let flags = bytes[5];
        Stream {
            depth: u16::from_le_bytes([bytes[0], bytes[1]]) as usize % MAX_DEPTH,
            text_bytes: u32::from_le_bytes([bytes[2], bytes[3], bytes[4], 0]) as usize
                % MAX_TEXT_BYTES,
            as_attribute: flags & 1 != 0,
            deflated: flags & 2 != 0,
            direct_length: flags & 4 != 0,
            encoding: match (flags >> 3) % 3 {
                0 => Encoding::Utf8,
                1 => Encoding::Utf16LittleEndian,
                _ => Encoding::Utf16BigEndian,
            },
        }
    }

    fn xmp(&self) -> String {
        let text = "A".repeat(self.text_bytes);
        let body = if self.as_attribute {
            format!(r#"<v:holder v:payload="{text}"/>"#)
        } else {
            let depth = self.depth.max(1);
            format!(
                "{}{text}{}",
                "<v:payload>".repeat(depth),
                "</v:payload>".repeat(depth)
            )
        };
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:v="https://verifiabl.io/ns/">{body}</x:xmpmeta>"#
        )
    }

    fn packet(&self) -> Vec<u8> {
        let xmp = self.xmp();
        match self.encoding {
            Encoding::Utf8 => xmp.into_bytes(),
            Encoding::Utf16LittleEndian => xmp.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            Encoding::Utf16BigEndian => xmp.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        }
    }

    fn write(&self, pdf: &mut Vec<u8>) {
        let packet = self.packet();
        let (filter, data) = if self.deflated {
            (
                " /Filter /FlateDecode",
                miniz_oxide::deflate::compress_to_vec_zlib(&packet, 1),
            )
        } else {
            ("", packet)
        };
        // Without a direct length the scanner falls back to finding endstream.
        let length = if self.direct_length {
            data.len().to_string()
        } else {
            "9 0 R".to_owned()
        };
        pdf.extend_from_slice(
            format!(
                "1 0 obj\n<< /Type /Metadata /Subtype /XML{filter} /Length {length} >>\nstream\n"
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&data);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
    }
}

pub fn check(shape: &[u8]) {
    let mut pdf = b"%PDF-1.7\n".to_vec();
    for stream in shape.as_chunks::<STREAM_BYTES>().0.iter().take(MAX_STREAMS) {
        Stream::from(stream).write(&mut pdf);
    }

    if let Ok(payloads) = metadata::payloads(&pdf) {
        let bytes: usize = payloads.iter().map(String::len).sum();
        assert!(
            bytes <= MAX_RESULT_BYTES,
            "more payload text than a result holds"
        );
    }
}
