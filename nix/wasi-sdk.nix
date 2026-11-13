# The official wasi-sdk release: clang, wasm-ld and WASI sysroots with and
# without C++ exceptions. It builds the C++ in the extraction module.
{
  autoPatchelfHook,
  fetchurl,
  lib,
  libxml2,
  ncurses,
  stdenv,
  zlib,
  zstd,
}:

let
  version = "34.0";
  targets = {
    "aarch64-darwin" = {
      name = "arm64-macos";
      sha256 = "9c59398106b417f8f14913380fdf0097a8cc0ff4af9eb3ce0065a859e88d49e9";
    };
    "aarch64-linux" = {
      name = "arm64-linux";
      sha256 = "f7e243dff54d60bcc576e94d6166b69f410f2500ae4a9ceef34315be10e77971";
    };
    "x86_64-linux" = {
      name = "x86_64-linux";
      sha256 = "b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4";
    };
  };
  target =
    targets.${stdenv.hostPlatform.system}
      or (throw "wasi-sdk is not packaged for ${stdenv.hostPlatform.system}");
  release = "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${lib.versions.major version}";
  # Each platform's tarball has a sysroot built on that platform, whose
  # libc++abi embeds the build path. These platform-independent ones replace
  # them, so every host links the same bytes into the module.
  sysroot = fetchurl {
    url = "${release}/wasi-sysroot-${version}.tar.gz";
    sha256 = "9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be";
  };
  compilerRt = fetchurl {
    url = "${release}/libclang_rt-${version}.tar.gz";
    sha256 = "eee3e634dcf71aa22b1333391623cf5c9965a637dc428a27b1a858c026c587f1";
  };
in
stdenv.mkDerivation {
  pname = "wasi-sdk";
  inherit version;

  src = fetchurl {
    url = "${release}/wasi-sdk-${version}-${target.name}.tar.gz";
    inherit (target) sha256;
  };

  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ autoPatchelfHook ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    libxml2
    ncurses
    stdenv.cc.cc.lib
    zlib
    zstd
  ];

  dontConfigure = true;
  dontBuild = true;
  # The sysroot holds WebAssembly archives, not host binaries.
  dontStrip = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -R . $out
    rm -r $out/share/wasi-sysroot
    mkdir $out/share/wasi-sysroot
    tar xzf ${sysroot} -C $out/share/wasi-sysroot --strip-components=1
    for resources in $out/lib/clang/*/lib; do
      rm -r "$resources"
      mkdir "$resources"
      tar xzf ${compilerRt} -C "$resources" --strip-components=1
    done
    runHook postInstall
  '';

  meta = {
    description = "WASI-enabled WebAssembly C/C++ toolchain";
    homepage = "https://github.com/WebAssembly/wasi-sdk";
    license = lib.licenses.asl20;
    platforms = builtins.attrNames targets;
  };
}
