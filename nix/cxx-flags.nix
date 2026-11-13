# The flags for ZXing-C++ and the C++ shims. PDFium has its own GN build in
# pdfium-wasi.nix. Each build runs with $NATIVE set to
# crates/verifiabl-extract/target/native under $PWD, so the mapped paths are
# the same in every derivation and on every machine.
{ debug }:
[
  "-O2"
  # Real wasm exceptions: ZXing-C++ catches errors while decoding damaged codes.
  "-fwasm-exceptions"
  "-mllvm"
  "-wasm-use-legacy-eh=false"
  # libc++ checks element access and traps, so an out-of-range index in the
  # C++ cannot quietly read or write elsewhere in the module's memory.
  "-D_LIBCPP_HARDENING_MODE=_LIBCPP_HARDENING_MODE_FAST"
  "-ffile-prefix-map=$NATIVE=/native"
  "-ffile-prefix-map=$PWD=/extraction"
]
# DWARF, for a native debugger and source-mapped traps.
++ (if debug then [ "-g" ] else [ ])
