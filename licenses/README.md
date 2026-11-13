# Licences of the runtime code

Some code in the module has no licence file. This folder holds the licence texts for that code. [`crates/notices`](../crates/notices) adds them to `dist/THIRD-PARTY-NOTICES.txt`. PDFium, ZXing-C++ and the Rust crates supply their own licence files.

[`components.json`](components.json) lists each component, its files here and the toolchain versions that they apply to. The build fails if the toolchain has a different version. Then, replace the texts from the new version.

| Folder | Source | Covers |
| --- | --- | --- |
| `wasi-libc/` | [WebAssembly/wasi-libc](https://github.com/WebAssembly/wasi-libc), at the commit in the wasi-sdk `VERSION` file | The C library, and the cloudlibc, musl and musl-fts code in it |
| `llvm/` | [llvm/llvm-project](https://github.com/llvm/llvm-project), at the commit in the wasi-sdk `VERSION` file | libc++, libc++abi, libunwind and compiler-rt |
| `rust/` | [rust-lang/rust](https://github.com/rust-lang/rust), at the tag of the `rustc` release | The Rust standard library |
| `zxing-cpp/` | The ZXing-C++ source | libzueci and the UTF-8 decoder in it. ZXing-C++ gives their licences only in source comments. |

The files are copies. Do not edit them. Replace them from their source.
