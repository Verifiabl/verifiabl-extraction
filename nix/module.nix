# The extraction module and its provenance, as dist/ holds them. Only the
# x86_64-linux build matches dist/: wasi-sdk's clang folds std::cos with the
# host's libm and orders coroutine frames differently on each host platform.
{
  binaryen,
  cargo,
  jq,
  lib,
  lld,
  rustPlatform,
  rustc,
  stdenv,
  pdfiumWasi,
  wasiSdk,
  zxingWasi,
  zxing-cpp,
  # Keeps function names and DWARF, to see what takes up the module's size or
  # to map a trap to source. A debug build is never shipped.
  debug ? false,
}:

let
  root = ../.;
  crate = ../crates/verifiabl-extract;
  abi = lib.importJSON ../abi/extraction-abi.json;
  target = "wasm32-unknown-unknown";
  # Cargo's own path for crates.io sources. Panic messages embed the crates'
  # source paths, so the vendored crates are mapped to it, the path a cargo
  # build outside Nix records.
  cratesIoSource = "/cargo/registry/src/index.crates.io-1949cf8c6b5b557f";
  cxxFlags = import ./cxx-flags.nix { inherit debug; };
  # The exports the module resolves its own calls to, which the linker also
  # exports: the WASI calls, and the callback ZXing-C++'s shim passes codes to.
  internalExport = "^func (__imported_wasi_snapshot_preview1_[A-Za-z0-9_]+|vf_zxing_emit)$";
  # Hosts call exports by name without checking their types, so the module
  # exports the interface and nothing a host could mistake for part of it.
  interfaceExports = [
    "memory ${abi.memory.export}"
  ]
  ++ map (name: "func ${name}") (builtins.attrNames abi.exports);
  # A build path in the module would make its hash depend on the machine.
  hostPath = "/(home|Users|tmp|root|build|nix/store)/[A-Za-z0-9_.-]+/";
in
stdenv.mkDerivation {
  pname = if debug then "verifiabl-extract-debug" else "verifiabl-extract";
  version = toString abi.abiVersion;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (crate + "/.cargo")
      (crate + "/Cargo.lock")
      (crate + "/Cargo.toml")
      (crate + "/build.rs")
      (crate + "/native")
      (crate + "/src")
    ];
  };

  cargoDeps = rustPlatform.importCargoLock { lockFile = crate + "/Cargo.lock"; };
  cargoRoot = "crates/verifiabl-extract";

  nativeBuildInputs = [
    # wasm-opt converts PDFium's legacy exception handling to the standard encoding.
    binaryen
    cargo
    jq
    lld
    rustPlatform.cargoSetupHook
    rustc
  ];

  WASI_SDK_PATH = wasiSdk;
  PDFIUM_WASI_DIR = pdfiumWasi;

  buildPhase = ''
    runHook preBuild

    export NATIVE=$PWD/crates/verifiabl-extract/target/native
    mkdir -p $NATIVE

    # Where build.rs and the shims expect ZXing-C++.
    install -Dm644 ${zxingWasi}/lib/libZXing.a $NATIVE/libZXing.a
    install -Dm644 ${zxingWasi}/include/Version.h $NATIVE/zxing/core/Version.h

    # The C++ that calls ZXing-C++ and PDFium for the Rust, as libvfshim.a.
    for shim in zxing_shim pdfium_shim; do
      ${wasiSdk}/bin/clang++ \
        --target=wasm32-wasip1 \
        --sysroot=${wasiSdk}/share/wasi-sysroot \
        ${lib.concatStringsSep " " cxxFlags} \
        -std=c++20 \
        -I ${zxing-cpp.src}/core/src \
        -I $NATIVE/zxing/core \
        -I ${pdfiumWasi}/include \
        -c crates/verifiabl-extract/native/$shim.cpp \
        -o $NATIVE/$shim.o
    done
    ${wasiSdk}/bin/llvm-ar rcs $NATIVE/libvfshim.a $NATIVE/zxing_shim.o $NATIVE/pdfium_shim.o

    # cargoSetupHook vendors the crates here.
    rustFlags='["--remap-path-prefix='"$NIX_BUILD_TOP"'/cargo-vendor-dir=${cratesIoSource}","--remap-path-prefix='"$PWD"'=/extraction"]'
    (
      cd crates/verifiabl-extract
      cargo build --release --frozen --target ${target} \
        --config "target.${target}.rustflags=$rustFlags" \
        ${lib.optionalString debug "--config profile.release.strip=false --config profile.release.debug=true"}
    )

    # PDFium's setjmp/longjmp uses the legacy exception encoding, which
    # Wasmtime rejects; translate it to the standard one ZXing-C++ uses.
    # -g keeps the name section and DWARF; -O2 can leave some DWARF inexact.
    wasm-opt --all-features --translate-to-exnref -O2 ${lib.optionalString debug "-g"} \
      crates/verifiabl-extract/target/${target}/release/verifiabl_extract.wasm \
      -o verifiabl-extract.wasm

    runHook postBuild
  '';

  doCheck = true;
  checkPhase = ''
    runHook preCheck

    wasm-dis verifiabl-extract.wasm -o module.wat

    # The sandbox's guarantee: the module can reach nothing outside its own memory.
    if grep -E '^ \(import ' module.wat; then
      echo "The extraction module must import nothing." >&2
      exit 1
    fi

    grep -oE '^ \(export "[^"]+" \((func|memory|table|global|tag)' module.wat \
      | sed -E 's/^ \(export "([^"]+)" \((.*)$/\2 \1/' \
      | grep -vE '${internalExport}' \
      | sort > exports.txt
    printf '%s\n' ${lib.escapeShellArgs interfaceExports} | sort > expected-exports.txt
    if ! diff -u expected-exports.txt exports.txt; then
      echo "The extraction module's exports differ from abi/extraction-abi.json." >&2
      exit 1
    fi

    # DWARF records build paths by design.
    if ${lib.boolToString (!debug)} && grep -aoE '${hostPath}' verifiabl-extract.wasm; then
      echo "The extraction module embeds a build path, which breaks reproducibility." >&2
      exit 1
    fi

    runHook postCheck
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 verifiabl-extract.wasm $out/verifiabl-extract.wasm
    jq -n \
      --argjson abiVersion ${toString abi.abiVersion} \
      --arg sha256 "$(sha256sum verifiabl-extract.wasm | cut -d' ' -f1)" \
      --argjson bytes "$(stat -c %s verifiabl-extract.wasm)" \
      '{ abiVersion: $abiVersion, sha256: $sha256, bytes: $bytes }' \
      > $out/verifiabl-extract.json
    runHook postInstall
  '';

  dontStrip = true;
  dontFixup = true;

  meta = {
    description = "Verifiabl's sandboxed WebAssembly module that reads payslip images and PDFs";
    inherit (wasiSdk.meta) platforms;
  };
}
