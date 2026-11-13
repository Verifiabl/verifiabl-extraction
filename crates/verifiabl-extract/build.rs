// Links ZXing-C++, PDFium and the WASI C/C++ runtime into the WebAssembly
// module. VF_NATIVE_DIR, set in .cargo/config.toml, holds libZXing.a and
// libvfshim.a, built by nix/module.nix with wasi-sdk; PDFIUM_WASI_DIR holds
// libpdfium.a; WASI_SDK_PATH is wasi-sdk.
fn main() {
    // A host build has only the plain Rust modules, for their tests and fuzz targets.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        return;
    }
    let native =
        std::env::var("VF_NATIVE_DIR").expect("VF_NATIVE_DIR is set in .cargo/config.toml");
    let sdk = std::env::var("WASI_SDK_PATH").expect("set WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=VF_NATIVE_DIR");
    let pdfium = std::env::var("PDFIUM_WASI_DIR").expect("set PDFIUM_WASI_DIR");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=PDFIUM_WASI_DIR");
    println!("cargo:rustc-link-search=native={pdfium}/lib");
    // Cargo does not track linked archives, so a rebuilt one would otherwise
    // leave the module linked against the archive it replaced.
    for archive in [
        format!("{native}/libvfshim.a"),
        format!("{native}/libZXing.a"),
        format!("{pdfium}/lib/libpdfium.a"),
    ] {
        println!("cargo:rerun-if-changed={archive}");
    }

    let sysroot = format!("{sdk}/share/wasi-sysroot/lib/wasm32-wasip1");
    // The C++ runtime built with wasm exceptions, which ZXing-C++ catches internally.
    println!("cargo:rustc-link-search=native={native}");
    println!("cargo:rustc-link-search=native={sysroot}/eh");
    println!("cargo:rustc-link-search=native={sysroot}");
    // PDFium uses setjmp/longjmp (libsetjmp), mmap and signal emulation, and pthread stubs.
    for library in [
        "vfshim",
        "ZXing",
        "pdfium",
        "c++",
        "c++abi",
        "unwind",
        "setjmp",
        "wasi-emulated-mman",
        "wasi-emulated-signal",
        "wasi-emulated-getpid",
        "c",
    ] {
        println!("cargo:rustc-link-lib=static={library}");
    }
    let builtins = std::fs::read_dir(format!("{sdk}/lib/clang"))
        .expect("wasi-sdk has lib/clang")
        .filter_map(Result::ok)
        .map(|entry| {
            entry
                .path()
                .join("lib/wasm32-unknown-wasip1/libclang_rt.builtins.a")
        })
        .find(|path| path.exists())
        .expect("wasi-sdk has the wasm32 compiler-rt builtins");
    println!("cargo:rustc-link-arg={}", builtins.display());
}
