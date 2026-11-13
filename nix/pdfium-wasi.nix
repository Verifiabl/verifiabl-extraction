# PDFium as a static wasm32-wasip1 library, built from upstream source with
# wasi-sdk. Upstream has no WASI target, so pdfium-wasi/ holds the port: a GN
# toolchain and config, build-file conditions, no-op ICU mutexes and stub
# thread headers. pdfium-wasi/deps.json pins PDFium and each dependency.
{
  gclient2nix,
  gn,
  lib,
  ninja,
  python3,
  stdenvNoCC,
  wasiSdk,
}:

let
  # PDFium's chromium/8085 branch, which Chromium 157 ships.
  major = 157;
  minor = 0;
  build = 8085;
  patch = 0;
  port = ./pdfium-wasi;
in
stdenvNoCC.mkDerivation {
  pname = "pdfium-wasi";
  version = "${toString major}.${toString minor}.${toString build}.${toString patch}";

  gclientDeps = gclient2nix.importGclientDeps (port + "/deps.json");
  sourceRoot = "src";
  __structuredAttrs = true;
  strictDeps = true;

  nativeBuildInputs = [
    gclient2nix.gclientUnpackHook
    gn
    ninja
    python3
  ];

  patches = [ (port + "/pdfium.patch") ];

  postPatch = ''
    patch -p1 -d build < ${port}/build.patch
    install -Dm644 ${port}/toolchain.gn build/toolchain/wasi/BUILD.gn
    install -Dm644 ${port}/config.gn build/config/wasi/BUILD.gn
    install -Dm644 -t build/config/wasi/include ${port}/include/*

    substituteInPlace .gn \
      --replace-fail 'script_executable = "//third_party/cpython3/host/bin/python3"' \
        'script_executable = "${lib.getExe python3}"'

    # gclient writes these when it checks PDFium out.
    cat > build/config/gclient_args.gni <<EOF
    build_with_chromium = false
    checkout_android = false
    checkout_libpng = true
    checkout_skia = false
    EOF
  '';

  # Clang's resource directory is named for its major version.
  preConfigure = ''
    gnFlags+=("clang_version=\"$(ls ${wasiSdk}/lib/clang)\"")
  '';

  gnFlags = [
    "target_os=\"wasi\""
    "target_cpu=\"wasm\""
    "is_debug=false"
    "symbol_level=0"
    "chrome_pgo_phase=0"
    "treat_warnings_as_errors=false"

    # One static library with every dependency in it, and no JavaScript or XFA.
    "is_component_build=false"
    "pdf_is_standalone=true"
    "pdf_is_complete_lib=true"
    "pdf_enable_v8=false"
    "pdf_enable_xfa=false"
    "pdf_use_partition_alloc=false"

    # wasi-sdk's clang, sysroot and libc++ instead of Chromium's.
    "is_clang=true"
    "clang_base_path=\"${wasiSdk}\""
    "clang_use_chrome_plugins=false"
    "use_sysroot=true"
    "sysroot=\"${wasiSdk}/share/wasi-sysroot\""
    "use_custom_libcxx=false"
    "use_custom_libcxx_for_host=false"
    "use_glib=false"
  ];

  ninjaFlags = [ "pdfium" ];

  # The build runs in out/Release.
  installPhase = ''
    runHook preInstall

    install -Dm644 obj/libpdfium.a $out/lib/libpdfium.a
    mkdir -p $out/include
    cp -R ../../public/. $out/include/
    install -Dm644 args.gn $out/args.gn
    install -Dm644 ${port}/LICENSE $out/LICENSE
    ninja -t deps > compiler-inputs.txt
    bash ${port}/collect-licenses.sh ../.. compiler-inputs.txt $out/licenses

    cat > $out/VERSION <<EOF
    MAJOR=${toString major}
    MINOR=${toString minor}
    BUILD=${toString build}
    PATCH=${toString patch}
    EOF

    runHook postInstall
  '';

  dontStrip = true;
  dontFixup = true;

  meta = {
    description = "PDFium static library for wasm32-wasip1, built from source";
    homepage = "https://pdfium.googlesource.com/pdfium/";
    license =
      with lib.licenses;
      AND [
        asl20
        bsd3
        mit
      ];
    platforms = lib.platforms.unix;
  };
}
