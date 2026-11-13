# Development

Nix pins the toolchain. Use `nix develop` for a shell with all tools. The first build compiles PDFium, which takes several minutes.

## Build

```sh
nix build .#module
nix build .#dist
```

`.#module` builds the module and its provenance, from [`nix/module.nix`](nix/module.nix). `.#dist` builds all files that `dist/` holds: the module, its provenance and its licence notices. The module build fails if the module imports something, if its exports are different from [`abi/extraction-abi.json`](abi/extraction-abi.json), or if it contains a build path.

The build is reproducible only on x86_64 Linux, which CI uses. wasi-sdk makes a slightly different ZXing-C++ on each host platform. Thus, `dist/` holds the x86_64 Linux build. On x86_64 Linux:

- `nix flake check` compares the build with `dist/`.
- `nix run .#update-dist` writes `dist/`. On a different platform, it stops and writes nothing.

`flake.lock` pins the toolchain, so it also pins the hash of the module.

[`nix/notices.nix`](nix/notices.nix) runs [`crates/notices`](crates/notices), which writes `THIRD-PARTY-NOTICES.txt` from what the module links: the vendored crates, PDFium, ZXing-C++, and [`licenses/`](licenses/README.md) for the runtimes. The notices build fails if a dependency has no licence text.

### Debug build

`nix build .#module-debug` keeps the function names and DWARF. It works on each platform and never writes `dist/`. Use it to find the source line of a trap, or what makes the module large:

```sh
nix build .#module-debug
cd crates/host
cargo run --release --bin size-report -- ../../result/verifiabl-extract.wasm
```

The report gives the section sizes, the code size of each linked library and the largest functions. Compare the share of each library, not the exact sizes: the debug code section is a few percent larger.

## Checks

CI runs these checks on each change. Run each one from its folder with `nix develop`:

| Folder | Commands |
| --- | --- |
| `.` | `nix flake check` |
| `crates/verifiabl-extract` | `cargo fmt --check`, `cargo clippy --release --locked -- -D warnings`, `cargo test --locked --target x86_64-unknown-linux-gnu` (or your host target) |
| `crates/host` | `cargo fmt --check`, `cargo clippy --release --locked --all-targets -- -D warnings`, `cargo test --release --locked` |
| `crates/notices` | `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` |
| `fuzz` | `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --release --locked` |

`cargo test` in `crates/host` is the conformance check of the reference host. [TESTING.md](TESTING.md) tells you what each check finds.

## Read a file with the reference host

```sh
cd crates/host
cargo run --release --bin read -- pdf payslip.pdf
cargo run --release --bin read -- pixels scan.rgba32 --width 1654 --height 2339 --format rgba32
cargo run --release --bin replay -- ../../fuzz/corpus
```

`read` prints the status and the number of texts that it found. It prints the texts only with `--show-texts`. Pixels have no header, so a pixel read needs their width, their height and their format. `replay` sends each file in a corpus through the module.

## Build tools

The [README](README.md#wrapped-libraries) lists the libraries that the module wraps.

| Tool | Use |
| --- | --- |
| Rust, target `wasm32-unknown-unknown` | The exports, the image decoders and the PDF metadata scanner |
| [PDFium port](nix/pdfium-wasi/README.md) | Builds PDFium from upstream source for `wasm32-wasip1` |
| [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) | Compiles the C++ and supplies the C and C++ runtimes |
| [Binaryen](https://github.com/WebAssembly/binaryen) `wasm-opt` | Converts the legacy exception encoding of PDFium to the standard encoding, and optimises the module |
| `libfuzzer-sys` | Runs the fuzz targets |

The C and C++ code calls WASI functions. The module defines them itself, in `crates/verifiabl-extract/src/wasi_stubs.rs`, so that it imports nothing.

## Layout

| Path | Contents |
| --- | --- |
| `abi/` | The interface that hosts implement |
| `crates/host/` | The Rust reference host, its conformance checks, and the `read`, `replay`, `size-report` and `generate-fixtures` binaries |
| `crates/notices/` | Writes `dist/THIRD-PARTY-NOTICES.txt` |
| `crates/verifiabl-extract/` | The module. `src/` holds the Rust. `native/` holds the C++ shims for ZXing-C++ and PDFium. |
| `dist/` | The built module, its provenance and its licence notices |
| `fixtures/` | Synthetic files, and the results that each host must get |
| `fuzz/` | Fuzz targets, their checks, and the inputs that caused failures |
| `licenses/` | Licence texts for runtime code that has no licence file |
| `nix/` | The module build, wasi-sdk, ZXing-C++ and the PDFium port |
| `stand-ins/` | Modules that break the interface, and the status that each host must report |
