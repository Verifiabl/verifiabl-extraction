# The stand-in modules, assembled from stand-ins/*.wat with the features the
# extraction module itself needs, and no others.
{
  binaryen,
  lib,
  runCommand,
}:

let
  source = lib.fileset.toSource {
    root = ../stand-ins;
    fileset = lib.fileset.fileFilter (file: file.hasExt "wat") ../stand-ins;
  };
in
runCommand "verifiabl-extract-stand-ins" { nativeBuildInputs = [ binaryen ]; } ''
  mkdir $out
  for wat in ${source}/*.wat; do
    wasm-as --enable-exception-handling --enable-bulk-memory "$wat" -o "$out/$(basename "$wat" .wat).wasm"
  done
''
