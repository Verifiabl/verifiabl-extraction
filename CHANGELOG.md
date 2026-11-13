# Changelog

All notable changes to the module are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version
is in [`version.json`](version.json). The ABI version in
`dist/verifiabl-extract.json` changes on its own scale, when the meaning of an
export changes.

## [0.1.0]

The first release. It uses ABI version 3.

### Added

- `vf_read_image` reads the QR codes in a PNG, JPEG or WebP image.
- `vf_read_pdf` reads the payloads in a PDF's XMP metadata, and renders its
  pages with PDFium to read their QR codes only when the metadata gives no
  payload.
- `vf_read_pixels` reads the QR codes in pixels that the caller decoded, with
  the QR reader that `vf_read_image` uses.
- The module imports nothing and declares a 512 MiB memory maximum. The
  [host rules](abi/README.md#host-rules), fixtures and stand-in modules define
  how a host bounds and checks each read.
- The release assets are the module, `verifiabl-extract.json`,
  `THIRD-PARTY-NOTICES.txt` and `SHA256SUMS`, with a build-provenance
  attestation.
