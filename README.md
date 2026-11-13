# verifiabl-extraction

verifiabl-extraction finds Verifiabl payloads in image and PDF files. It is a WebAssembly module, which gives two benefits:

- The WebAssembly runtime isolates the code that parses files. Input files are untrusted and can be hostile. A bug in the module cannot reach the memory of the host, unless the runtime also has a bug.
- Verifier SDKs in different ecosystems use the same module. Thus, each SDK reads a file in the same way, and there is one binary to test and secure.

The module finds payload text only. It does not show that a document is genuine. A verification shows that a payload matches a record that a registered issuer sealed.

## Use the module

[`dist/`](dist) holds the built module:

- `verifiabl-extract.wasm`: the module.
- `verifiabl-extract.json`: the ABI version and the SHA-256 of the module.
- `THIRD-PARTY-NOTICES.txt`: the licences of all code in the module. Publish this file with each copy of the module.

A host must enable the WebAssembly exceptions proposal and obey the [host rules](abi/README.md#host-rules). Each host must get the results in [`fixtures/expected.json`](fixtures/README.md) and [`stand-ins/expected.json`](stand-ins/README.md).

| Host | Runtime |
| --- | --- |
| Rust ([`crates/host`](crates/host)), the reference host | Wasmtime |
| .NET (`Verifiabl.Verifier.Extraction`) | Wasmtime, C API |

### Pixel reads

The module also reads QR codes in pixels that an integrator decoded with its own imaging library. It uses the same QR reader as for an image file, so a code reads the same from a file and from its pixels.

The sandbox covers only the QR read. The decoder of the integrator runs outside the sandbox. For a file from an untrusted source, use the image read or the PDF read, which decode the file in the module. [`abi/README.md`](abi/README.md#reads) gives the pixel formats and checks.

## Isolation

| Control | Effect |
| --- | --- |
| The module imports nothing | The module cannot open files, use the network, read the clock or reach host memory. |
| A new instance for each read | One file cannot see the data of a different file. |
| A 512 MiB memory maximum, declared in the module | Each host enforces the same cap. |
| A time budget for each read | A small hostile file cannot stop the host. |
| Hosts check the output of the module | A hostile file can take control of the module. A module that breaks the interface gives `unreadable`, as a damaged file does. |
| Our parsers are in Rust | Rust prevents memory errors in the code that we write. |

## Wrapped libraries

| Library | Use |
| --- | --- |
| [ZXing-C++](https://github.com/zxing-cpp/zxing-cpp) | Reads QR codes |
| [PDFium](https://pdfium.googlesource.com/pdfium/) | Renders PDF pages, if the metadata gives no payload |
| [`png`](https://crates.io/crates/png), [`zune-jpeg`](https://crates.io/crates/zune-jpeg), [`image-webp`](https://crates.io/crates/image-webp) | Decode PNG, JPEG and WebP images |
| [`miniz_oxide`](https://crates.io/crates/miniz_oxide), [`roxmltree`](https://crates.io/crates/roxmltree) | Inflate and parse XMP metadata streams in PDFs |

These are mature libraries, and their projects fuzz them. `dist/THIRD-PARTY-NOTICES.txt` lists all code in the module.

## Reproduce the module

The build is reproducible on x86_64 Linux:

```sh
nix build .#module
sha256sum result/verifiabl-extract.wasm
```

The hash must agree with `dist/verifiabl-extract.json`. [DEVELOPMENT.md](DEVELOPMENT.md) gives the build and the checks.

## Releases

Each release, tagged `v<version>`, attaches `verifiabl-extract.wasm`, `verifiabl-extract.json`, `THIRD-PARTY-NOTICES.txt` and `SHA256SUMS`. [CHANGELOG.md](CHANGELOG.md) lists the changes. A verifier SDK records the release that its copy of the module came from in `extraction-release.json`.

Verify that this repository's release workflow built the assets of a release from the tag. The attestation shows that the workflow made `SHA256SUMS`. `SHA256SUMS` then shows that the other assets are the files that the workflow built:

```sh
gh release download v0.1.0 --repo Verifiabl/verifiabl-extraction --dir release
gh attestation verify release/SHA256SUMS \
  --repo Verifiabl/verifiabl-extraction \
  --signer-workflow Verifiabl/verifiabl-extraction/.github/workflows/release.yml \
  --source-ref refs/tags/v0.1.0 &&
  (cd release && sha256sum --check SHA256SUMS)
```

To compare the module with its source, check out the tag and [reproduce the module](#reproduce-the-module).

## Security

[SECURITY.md](SECURITY.md) tells you how to report a vulnerability. [TESTING.md](TESTING.md) gives the tests and the known gaps.
