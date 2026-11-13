# ZXing-C++'s readers as a static wasm32-wasip1 library, with the header its
# build generates. A separate derivation, so a change to the module's own code
# does not rebuild it.
{
  cmake,
  lib,
  ninja,
  stdenvNoCC,
  wasiSdk,
  zxing-cpp,
  debug ? false,
}:

let
  cxxFlags = import ./cxx-flags.nix { inherit debug; };
in
stdenvNoCC.mkDerivation {
  pname = if debug then "zxing-cpp-wasi-debug" else "zxing-cpp-wasi";
  inherit (zxing-cpp) version;

  dontUnpack = true;
  nativeBuildInputs = [
    cmake
    ninja
  ];
  dontUseCmakeConfigure = true;
  dontUseNinjaBuild = true;
  dontUseNinjaInstall = true;
  dontUseNinjaCheck = true;

  buildPhase = ''
    runHook preBuild
    export NATIVE=$PWD/crates/verifiabl-extract/target/native
    mkdir -p $NATIVE
    cmake -S ${zxing-cpp.src} -B $NATIVE/zxing -G Ninja \
      -DCMAKE_TOOLCHAIN_FILE=${wasiSdk}/share/cmake/wasi-sdk-p1.cmake \
      -DWASI_SDK_PREFIX=${wasiSdk} \
      -DCMAKE_BUILD_TYPE=Release \
      "-DCMAKE_CXX_FLAGS=${lib.concatStringsSep " " cxxFlags}" \
      -DBUILD_SHARED_LIBS=OFF \
      -DZXING_READERS=ON \
      -DZXING_WRITERS=OFF \
      -DZXING_C_API=OFF \
      -DZXING_EXAMPLES=OFF \
      -DZXING_BLACKBOX_TESTS=OFF \
      -DZXING_UNIT_TESTS=OFF \
      -DZXING_PYTHON_MODULE=OFF
    cmake --build $NATIVE/zxing --target ZXing
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 $NATIVE/zxing/core/libZXing.a $out/lib/libZXing.a
    install -Dm644 $NATIVE/zxing/core/Version.h $out/include/Version.h
    runHook postInstall
  '';

  dontFixup = true;

  meta = {
    description = "ZXing-C++ readers for wasm32-wasip1, built with wasi-sdk";
    inherit (zxing-cpp.meta) homepage license;
    inherit (wasiSdk.meta) platforms;
  };
}
