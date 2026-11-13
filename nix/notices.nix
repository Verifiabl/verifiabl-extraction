# dist/THIRD-PARTY-NOTICES.txt: the licences of everything the module links,
# read from the vendored crates, PDFium, ZXing-C++, wasi-sdk and licenses/ by
# crates/notices.
{
  cargo,
  lib,
  rustPlatform,
  rustc,
  stdenv,
  pdfiumWasi,
  wasiSdk,
  zxing-cpp,
}:

let
  crate = ../crates/verifiabl-extract;
  noticesCrate = ../crates/notices;
  tool = rustPlatform.buildRustPackage {
    pname = "verifiabl-extract-notices-tool";
    version = "0.1.0";
    src = lib.fileset.toSource {
      root = noticesCrate;
      fileset = lib.fileset.unions [
        (noticesCrate + "/Cargo.lock")
        (noticesCrate + "/Cargo.toml")
        (noticesCrate + "/src")
      ];
    };
    cargoLock.lockFile = noticesCrate + "/Cargo.lock";
  };
in
stdenv.mkDerivation {
  pname = "verifiabl-extract-notices";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      (crate + "/.cargo")
      (crate + "/Cargo.lock")
      (crate + "/Cargo.toml")
      (crate + "/build.rs")
      (crate + "/src")
    ];
  };

  cargoDeps = rustPlatform.importCargoLock { lockFile = crate + "/Cargo.lock"; };
  cargoRoot = "crates/verifiabl-extract";

  nativeBuildInputs = [
    cargo
    rustPlatform.cargoSetupHook
    rustc
    tool
  ];

  buildPhase = ''
    runHook preBuild
    (
      cd crates/verifiabl-extract
      cargo metadata --format-version 1 --locked --offline --filter-platform wasm32-unknown-unknown
    ) > metadata.json
    verifiabl-extract-notices \
      --cargo-metadata metadata.json \
      --pdfium ${pdfiumWasi} \
      --zxing ${zxing-cpp.src} \
      --wasi-sdk ${wasiSdk} \
      --licenses ${../licenses} \
      --rustc-version "$(rustc --version)" \
      > THIRD-PARTY-NOTICES.txt
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 THIRD-PARTY-NOTICES.txt $out/THIRD-PARTY-NOTICES.txt
    runHook postInstall
  '';
}
