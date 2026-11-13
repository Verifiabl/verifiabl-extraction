# PDFium for wasm32-wasip1

`../pdfium-wasi.nix` builds upstream PDFium into the static library the extraction module links. Upstream has no WASI target, so this folder holds the port and the source pins.

| File | Purpose |
| --- | --- |
| `deps.json` | PDFium at its `chromium/8085` branch and each dependency the build reads, as Gitiles archives pinned by hash, in `gclient2nix`'s format |
| `pdfium.patch` | Build-file conditions for WASI, and no-op ICU mutexes: WASI has one thread |
| `build.patch` | Adds the `wasi` target to Chromium's build configuration |
| `toolchain.gn`, `config.gn` | The GN toolchain and compiler config for wasi-sdk |
| `include/` | `<mutex>`, `<thread>` and `<condition_variable>` without threads, which wasi-sdk's libc++ omits |
| `collect-licenses.sh` | Copies the licence of each library the compiler read, header-only ones included, and fails for one without a licence file |
| `LICENSE` | The licence of the port files |

The port files come from [run-llama/pdfium-binaries](https://github.com/run-llama/pdfium-binaries) at commit `f15f55200f695a13c2ca3aab35f8aa40a596b2f7`, under the MIT licence in `LICENSE`. That project's own changes to PDFium's source (`patches/llamaparse`) are not applied: the library is upstream PDFium with only the port.

## Change the PDFium revision

1. List the dependencies of the new revision: `gclient2nix eval https://pdfium.googlesource.com/pdfium@<commit>`.
2. Replace each entry in `deps.json` with its new `rev`, and its `hash` from `nix-prefetch-url --unpack <url>/+archive/<rev>.tar.gz`. Dependencies the build does not read stay out: V8, Skia, the test corpus, depot_tools, the instrumented libraries, NASM, clang-format, the coverage and memory tools, and Chromium's libc++.
3. Update the version in `../pdfium-wasi.nix`, rebuild the module and run every host's conformance check.
