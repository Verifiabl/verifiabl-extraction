# Extraction module interface

[`extraction-abi.json`](extraction-abi.json) lists the exports, the status codes, the limits, the pixel formats and the WebAssembly features that a host must enable. Increase `abiVersion` when the meaning of an export changes, and update each host in the same change.

## Host rules

A host must obey these rules. The fixtures and the [stand-in modules](../stand-ins/README.md) test them.

1. Before the first call, check `vf_abi_version`. If the version is different, fail. This is a setup fault, not a file fault.
2. Refuse a file larger than `maxFileBytes` before you make an instance. Report `fileTooLarge`. Do not copy or allocate memory for the file.
3. Refuse pixels larger than `maxPixelBytes` before you make an instance. Report `tooManyPixels`. Do not copy or allocate memory for the pixels. `maxPixelBytes` is `maxPixels` pixels of 4 bytes. Thus, a larger buffer holds too many pixels in each format.
4. Make a new instance for each read. Discard it after the read.
5. Stop the instance at the time budget. Report `timedOut`.
6. Treat the output of the module as untrusted. Check each pointer and length against the memory of the instance. Check the result length against `maxResultBytes` before you read the result.
7. Report `unreadable` when the module breaks the interface:
   - a trap, an exhausted stack or an uncaught exception
   - an unknown status
   - a result that is outside the memory, too large, not JSON or not in the documented shape
8. After a trap, call `vf_out_of_memory`. If it returns 1, the trap followed a failed allocation. Report `outOfMemory`, not `unreadable`. Do this for a trap in `vf_alloc` also.

A host fails only for a fault in its own setup. It never fails because of the file, the pixels or the module output.

## Reads

For each read, a host calls `vf_alloc`, writes the file or the pixels at the address that it returns, and calls one of these exports.

`vf_read_image` decodes a PNG, JPEG or WebP image and reads its QR codes.

`vf_read_pdf` does these steps:

1. It scans the bytes of the file for XMP metadata streams, in UTF-8 or UTF-16.
2. It keeps each `payload` property of at most `maxPayloadLength` UTF-16 code units. This is the barcode limit of the Verifier API. The Verifier API cannot verify a longer value, so the module ignores it.
3. If no payload remains, it renders the pages with PDFium and reads their QR codes. It renders at most `maxPdfPages` pages at `pdfRenderDpi`.

`vf_read_pixels` reads the QR codes in pixels that the caller of the host decoded. It uses the QR reader that `vf_read_image` uses. Thus, a code reads the same from a file and from its pixels.

The pixels are rows from top to bottom, with no padding between the rows. `format` is a code in `pixelFormats`:

| Format | Bytes for each pixel |
| --- | --- |
| `gray8` | 1: grey |
| `rgba32` | 4: red, green, blue, alpha |
| `bgra32` | 4: blue, green, red, alpha |

Alpha is not premultiplied. The module blends each pixel with white, as it does for an image file.

The module does these checks before it reads the pixels:

1. A `width` or a `height` of zero gives `unreadable`.
2. More than `maxPixels` pixels give `tooManyPixels`.
3. A `format` that is not in `pixelFormats` gives `unreadable`.
4. A `length` that is not `width` × `height` × the bytes for each pixel gives `unreadable`. The module calculates this product with overflow checks.

Then the module converts the pixels to greyscale in the buffer that the host wrote. Thus, a host does no work on the pixels.

The sandbox covers only the QR read of a pixel read. The caller decodes the image outside the sandbox, with its own decoder. The sandbox does not contain a bug in that decoder. For a file from an untrusted source, use `vf_read_image` or `vf_read_pdf`.

## Result limits

The module keeps each result within `maxResultBytes`:

- A PDF gives `tooMuchMetadata` if its metadata holds more payload text than one result can hold. It also gives `tooMuchMetadata` if it has more metadata streams, payload properties or decoded metadata than a payslip needs.
- QR codes with more text than one result can hold give `resultTooLarge`.

The module skips a metadata stream larger than 1 MiB, before or after inflation. This stream gives no payload. If no other stream gives a payload, the module renders the pages.
