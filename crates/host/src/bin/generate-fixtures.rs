//! Writes the fixtures that are generated rather than drawn: small files that
//! expand inside the module, metadata in an encoding issuers' tools rarely
//! write, metadata too long to be a payload beside a QR code, and the pixels
//! of a QR code in each pixel format.
//! fixtures/README.md describes them. A deflated stream need not match the
//! committed file byte for byte; each file must read as fixtures/expected.json
//! says, which tests/generated_fixtures.rs checks.

use std::path::PathBuf;
use std::process::ExitCode;

use miniz_oxide::deflate::compress_to_vec_zlib;
use verifiabl_extract_host::cli::{self, Failure};
use verifiabl_extract_host::{extraction_root, Abi};

const USAGE: &str = "usage: generate-fixtures [--out <folder>]";
const PAYLOAD_NAMESPACE: &str = "https://verifiabl.io/ns/";
// The payload in metadata.pdf, so both fixtures read the same.
const METADATA_PAYLOAD: &str = "2|n5waC35dPCoRtMXW5_gJqw|JVSWY3DPO5XXE3DE";
// Nested payload elements, each of whose values repeats the text inside it.
const NESTED_LEVELS: usize = 300;
const NESTED_TEXT_BYTES: usize = 300_000;
// Two payloads that together exceed the 1 MiB a result may hold.
const OVERSIZED_PAYLOAD_BYTES: usize = 600_000;
// Line segments in one path: PDFium needs more than the module's memory to hold them.
const PATH_SEGMENTS: usize = 40_000_000;
const BEST_COMPRESSION: u8 = 9;
// The edge of a QR code's finder pattern, in modules.
const FINDER_MODULES: usize = 7;
// The pixel fixtures draw each module this wide, inside a border of light modules.
const MODULE_PIXELS: usize = 3;
const QUIET_MODULES: usize = 4;
// In red, green, blue, alpha order: black on a transparent background.
const BLACK: [u8; 4] = [0, 0, 0, 255];
const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];
// In blue, green, red, alpha order: blue on grey. Read as red, green, blue,
// the blue is red, which is as bright as the grey, so the code disappears.
const BLUE_FIRST_BLUE: [u8; 4] = [255, 0, 0, 255];
const GREY_AS_BRIGHT_AS_RED: [u8; 4] = [76, 76, 76, 255];

fn main() -> ExitCode {
    cli::exit(USAGE, run(std::env::args().skip(1).collect()))
}

fn run(args: Vec<String>) -> Result<(), Failure> {
    let fixtures = extraction_root().join("fixtures");
    let out = match args.as_slice() {
        [] => fixtures.clone(),
        [flag, folder] if flag == "--out" => PathBuf::from(folder),
        _ => return Err(Failure::Usage("unexpected arguments".to_string())),
    };
    let abi =
        Abi::read(&extraction_root().join("abi/extraction-abi.json")).map_err(Failure::Error)?;
    let vector_qr = std::fs::read(fixtures.join("vector-qr.pdf"))
        .map_err(|error| Failure::Error(format!("cannot read vector-qr.pdf: {error}")))?;
    let qr_png = std::fs::read(fixtures.join("qr.png"))
        .map_err(|error| Failure::Error(format!("cannot read qr.png: {error}")))?;
    let modules = qr_modules(&qr_png)?;

    let generated = [
        ("qr.gray8", draw(&modules, &[0], &[255])),
        (
            "qr-transparent.rgba32",
            draw(&modules, &BLACK, &TRANSPARENT),
        ),
        (
            "qr-blue.bgra32",
            draw(&modules, &BLUE_FIRST_BLUE, &GREY_AS_BRIGHT_AS_RED),
        ),
        ("nested-metadata.pdf", nested_metadata()),
        ("oversized-metadata.pdf", oversized_metadata()),
        ("long-path.pdf", long_path()),
        ("utf16-metadata.pdf", utf16_metadata()),
        (
            "long-metadata-qr.pdf",
            long_metadata_qr(&vector_qr, abi.limits.max_payload_length)?,
        ),
    ];
    std::fs::create_dir_all(&out)
        .map_err(|error| Failure::Error(format!("cannot create {}: {error}", out.display())))?;
    for (name, bytes) in generated {
        let path = out.join(name);
        std::fs::write(&path, &bytes)
            .map_err(|error| Failure::Error(format!("cannot write {}: {error}", path.display())))?;
        println!("Wrote {} ({} bytes).", path.display(), bytes.len());
    }
    Ok(())
}

fn stream(dictionary: &str, data: &[u8]) -> Vec<u8> {
    let mut bytes = format!("<< {dictionary} /Length {} >>\nstream\n", data.len()).into_bytes();
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(b"\nendstream");
    bytes
}

// A one-page PDF whose page content is `contents`, followed by `extra` objects.
fn pdf(contents: Vec<u8>, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R >>".to_vec(),
        contents,
    ];
    objects.extend(extra);
    let mut file = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(file.len());
        file.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        file.extend_from_slice(body);
        file.extend_from_slice(b"\nendobj\n");
    }
    let start = file.len();
    let count = objects.len() + 1;
    let entries: String = offsets
        .iter()
        .map(|offset| format!("{offset:010} 00000 n \n"))
        .collect();
    file.extend_from_slice(
        format!(
            "xref\n0 {count}\n0000000000 65535 f \n{entries}trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n"
        )
        .as_bytes(),
    );
    file
}

fn empty_page() -> Vec<u8> {
    stream("", b"")
}

fn xmp(body: &str) -> String {
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" xmlns:v=\"{PAYLOAD_NAMESPACE}\">{body}</x:xmpmeta>"
    )
}

fn deflated_metadata(body: &str) -> Vec<u8> {
    stream(
        "/Type /Metadata /Subtype /XML /Filter /FlateDecode",
        &compress_to_vec_zlib(xmp(body).as_bytes(), BEST_COMPRESSION),
    )
}

fn nested_metadata() -> Vec<u8> {
    let body = format!(
        "{}{}{}",
        "<v:payload>".repeat(NESTED_LEVELS),
        "C".repeat(NESTED_TEXT_BYTES),
        "</v:payload>".repeat(NESTED_LEVELS)
    );
    pdf(empty_page(), vec![deflated_metadata(&body)])
}

fn oversized_metadata() -> Vec<u8> {
    let payload = |letter: &str| {
        deflated_metadata(&format!(
            "<v:payload>{}</v:payload>",
            letter.repeat(OVERSIZED_PAYLOAD_BYTES)
        ))
    };
    pdf(empty_page(), vec![payload("A"), payload("B")])
}

// An XMP packet as UTF-16 without a byte order mark before it, as packets are
// written: the mark is the value of the header's begin attribute.
fn utf16_metadata() -> Vec<u8> {
    let packet = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>{}<?xpacket end=\"w\"?>",
        xmp(&format!("<v:payload>{METADATA_PAYLOAD}</v:payload>"))
    );
    let utf16: Vec<u8> = packet.encode_utf16().flat_map(u16::to_le_bytes).collect();
    pdf(
        empty_page(),
        vec![stream("/Type /Metadata /Subtype /XML", &utf16)],
    )
}

// vector-qr.pdf's page, whose only metadata value is one character too long
// to be a payload, so the page is still read.
fn long_metadata_qr(vector_qr: &[u8], max_payload_length: usize) -> Result<Vec<u8>, Failure> {
    let missing = || Failure::Error("vector-qr.pdf has no page content stream".to_string());
    let start = find(vector_qr, b"stream\n", 0).ok_or_else(missing)? + b"stream\n".len();
    let end = find(vector_qr, b"\nendstream", start).ok_or_else(missing)?;
    let value = "A".repeat(max_payload_length + 1);
    Ok(pdf(
        stream("", &vector_qr[start..end]),
        vec![deflated_metadata(&format!(
            "<v:payload>{value}</v:payload>"
        ))],
    ))
}

fn long_path() -> Vec<u8> {
    let path = format!("0 0 m\n{}S\n", "1 1 l\n".repeat(PATH_SEGMENTS));
    // Deflated twice: the first pass leaves a regular pattern the second removes.
    let once = compress_to_vec_zlib(path.as_bytes(), BEST_COMPRESSION);
    let twice = compress_to_vec_zlib(&once, BEST_COMPRESSION);
    pdf(
        stream("/Filter [/FlateDecode /FlateDecode]", &twice),
        Vec::new(),
    )
}

// The modules of the code in qr.png, row by row: true for a dark module.
// The code's first dark pixel starts a finder pattern, which gives the width
// of a module; each module is then sampled at its centre.
fn qr_modules(png: &[u8]) -> Result<Vec<Vec<bool>>, Failure> {
    let unusable = |reason: &str| Failure::Error(format!("qr.png {reason}"));
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .map_err(|error| unusable(&format!("does not decode: {error}")))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| unusable("is too large"))?;
    let mut pixels = vec![0; size];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|error| unusable(&format!("does not decode: {error}")))?;
    let (width, bytes) = (frame.width as usize, frame.color_type.samples());
    // The first channel is enough for a black and white image.
    let dark = |x: usize, y: usize| pixels[(y * width + x) * bytes] < 128;

    let first = (0..frame.height as usize * width)
        .find(|index| dark(index % width, index / width))
        .ok_or_else(|| unusable("has no dark pixel"))?;
    let (left, top) = (first % width, first / width);
    let finder = (left..width).take_while(|x| dark(*x, top)).count();
    let right = (left..width).rev().find(|x| dark(*x, top)).unwrap_or(left);
    let module = finder / FINDER_MODULES;
    if module == 0 || !(right - left + 1).is_multiple_of(module) {
        return Err(unusable("does not start with a finder pattern"));
    }
    let count = (right - left + 1) / module;
    let centre = |index: usize, start: usize| start + index * module + module / 2;
    Ok((0..count)
        .map(|row| {
            (0..count)
                .map(|column| dark(centre(column, left), centre(row, top)))
                .collect()
        })
        .collect())
}

// The code as pixels, each `dark` or `light`, row by row with no padding.
fn draw(modules: &[Vec<bool>], dark: &[u8], light: &[u8]) -> Vec<u8> {
    let side = modules.len() + 2 * QUIET_MODULES;
    let is_dark = |row: usize, column: usize| {
        let inside = |index: usize| (QUIET_MODULES..QUIET_MODULES + modules.len()).contains(&index);
        inside(row) && inside(column) && modules[row - QUIET_MODULES][column - QUIET_MODULES]
    };
    let mut pixels = Vec::with_capacity(side * side * MODULE_PIXELS * MODULE_PIXELS * dark.len());
    for y in 0..side * MODULE_PIXELS {
        for x in 0..side * MODULE_PIXELS {
            let pixel = if is_dark(y / MODULE_PIXELS, x / MODULE_PIXELS) {
                dark
            } else {
                light
            };
            pixels.extend_from_slice(pixel);
        }
    }
    pixels
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| from + position)
}
