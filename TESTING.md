# Testing the extraction module

The module reads files that applicants control, and pixels that an integrator decoded from these files. The tests look for three types of failure:

- **Escape or host failure.** A file causes a host to crash, stop, use all its memory or throw an unexpected error.
- **Resource exhaustion.** A small file uses much more time or memory than its size justifies.
- **Incorrect read.** A host reports text that is not in the file, or two hosts read one file differently.

The tests examine how the module finds payload text. They do not examine what a verification shows.

## Checks

| Check | Finds | When it runs |
| --- | --- | --- |
| Unit tests (`cargo test` on the host platform) | Errors in the limits, budgets and escaping of our Rust (`image`, `json`, `metadata`) | Each change |
| Fixtures (`fixtures/expected.json`) | A file that hosts read differently. This includes small files that expand past a limit. | Each change, in the conformance check of each host |
| Interface (`crates/host` tests) | A module that imports something, or exports something that is not in `abi/extraction-abi.json` | Each change |
| Stand-in modules (`stand-ins/expected.json`) | A host that fails when the module breaks the interface | Each change, in the conformance check of each host |
| `nix flake check` | A committed binary that is different from its source | Each change |
| Fuzz replay (`cargo test` in `fuzz/`) | A fixture or a kept input that causes a fuzz target to fail | Each change |
| Fuzzing (`fuzz/fuzz_targets/`) | Panics, slow inputs and broken guarantees in our Rust | Each night, in a private repository |
| Module replay (`crates/host`, the `replay` binary) | A corpus file that causes the built module to break the interface or exceed the time budget. It reads each file as an image, as a PDF and as greyscale pixels. | Each night, after fuzzing |

## Fuzz targets

The checks of each target are in `fuzz/src/`. The targets in `fuzz/fuzz_targets/` give inputs from libFuzzer to the checks. The targets build only with the `libfuzzer` feature. They build for the host platform, with coverage instrumentation, on the stable compiler.

| Target | Code | Fails when the code |
| --- | --- | --- |
| `metadata` | The PDF metadata scanner | Panics, takes more than 5 seconds, returns more payload text than one result holds, or returns a value longer than a payload |
| `metadata_shape` | The same scanner, with metadata streams built from a small set of numbers: nesting depth, text length, stream count, compression and encoding | Takes more than 5 seconds, or returns more payload text than one result holds |
| `image` | The size checks, greyscale conversion and truncated-file handling around the image decoders. Also the checks and the greyscale conversion of pixels that a caller decoded. | Panics, gives pixels that do not agree with the width and height, or accepts pixels whose length does not agree with their size |
| `result` | The JSON writer | Gives JSON that a parser does not decode to the same texts |

The fuzz replay needs no libFuzzer build:

```sh
cd fuzz
cargo test --release
```

It runs each file in `fuzz/regressions/<target>/` and each file in `fixtures/` through the check of each target. An input fails if the check panics or takes more than 5 seconds.

Verifiabl fuzzes the targets each night in a private repository, so a failing input is not public before a fix ships.

The fuzzer writes a failed input to `fuzz/artifacts/<target>/`. The module replay copies a failed file to `fuzz/artifacts/replay/`. The file name is the SHA-256 of the file. A note gives the source of the file and the result from the module.

The projects of PDFium, ZXing-C++, `png`, `zune-jpeg` and `image-webp` fuzz their own code. Thus, no target here fuzzes them directly. The module replay sends the corpus through them, but without coverage guidance.

## If a check finds a failure

1. Reduce the input. For a fuzz target, run `fuzz/target/<platform>/release/<target> -minimize_crash=1 <input>`.
2. Repair the cause.
3. Keep the input, so that each subsequent change tests it:
   - If the input caused a fuzz target to fail, put it in `fuzz/regressions/<target>/`.
   - If the built module or a host did not read the file correctly, put it in `fixtures/` and add its status to `expected.json`. Each host then reads it.
   - If a host did not handle a module behaviour correctly, add a stand-in module to `stand-ins/`.

## Gaps

- No sanitizer build of the C++ exists. A memory error in the C++ that stays in the module's memory causes no failure.
- No host here runs on Windows or on .NET Framework.
- The module replay has no coverage guidance. It finds only the errors that the corpus of the fuzz targets reaches.
