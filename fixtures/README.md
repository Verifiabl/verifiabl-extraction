# Extraction fixtures

Each host must read these synthetic files in the same way. `expected.json` gives the expected status of each file for each type of read (`image`, `pdf` or `pixels`). The status names are in `abi/extraction-abi.json`. For the status `ok`, `expected.json` also gives the result. The payloads are invented scan links and metadata. No payload is from a real payslip.

A pixel fixture holds only pixels. It has no header. Thus, `pixels` in `expected.json` gives the width, the height and the format of each pixel read. The file extension is the format of the pixels in the file.

`vector-qr.pdf` draws its QR code as filled rectangles in the page content, as an SVG badge does. It has no image to decode.

`cargo run --release --bin generate-fixtures` in `crates/host` writes eight fixtures. Its deflated streams can differ by byte from the committed files, but each file reads with the same status. Three are small files that expand in the module:

- `nested-metadata.pdf` nests 300 `payload` elements around 300 kB of text. Compression makes the file smaller than 1 kB. The value of each element repeats the text in it, so the total text is 90 MB.
- `oversized-metadata.pdf` holds two different 600 kB payloads. This is more than one result can hold.
- `long-path.pdf` draws one path of 40 million segments. PDFium cannot hold this path in the module's memory.

Two fixtures are PDFs with unusual metadata:

- `utf16-metadata.pdf` holds the payload of `metadata.pdf` in an XMP packet in UTF-16.
- `long-metadata-qr.pdf` is the page of `vector-qr.pdf`, with one metadata value that is one character longer than `maxPayloadLength`. This value cannot be a payload, so the module reads the QR code on the page.

Three fixtures are pixels. Each one draws the code of `qr.png` with 3 pixels for each module:

- `qr.gray8` holds greyscale pixels. `expected.json` also reads it with an incorrect height and with an incorrect format. The length of the file does not agree with these sizes, so each of these reads gives `unreadable`.
- `qr-transparent.rgba32` draws black modules on a transparent background. The module blends alpha with white, so the code reads.
- `qr-blue.bgra32` draws blue modules on a grey background. As `bgra32`, the code reads. As `rgba32`, the blue becomes red, which is as bright as the grey. Thus, the module finds no code. This shows that a host passes the format that the caller gives.
