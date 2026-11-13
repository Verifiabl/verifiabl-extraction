#!/usr/bin/env bash
# Copies the licence of PDFium and of every third-party library the compiler
# read while building it, so the notices cover exactly what is in the library:
# compiled sources and header-only code alike. Fails when one has no licence.
# Requires: bash, coreutils and grep, as the Nix build provides.
set -euo pipefail

usage() {
  echo "usage: collect-licenses.sh <pdfium-source> <compiler-inputs> <destination>" >&2
  echo "  <compiler-inputs> lists every file the compiler read: ninja -t deps" >&2
  exit 2
}

[[ $# -eq 3 ]] || usage
pdfium="$1"
inputs="$2"
destination="$3"

# A licence file at a library's root or in the src/ folder its upstream is
# checked out to. FreeType keeps its own in docs/, and agg23's is lower case.
shopt -s nocasematch
licence_name='^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|FTL\.TXT|README\.ijg)'

copy_licences() {
  local name="$1"
  local folder file base
  local copied=0
  for folder in "" "/src" "/src/docs"; do
    for file in "$pdfium/third_party/$name$folder"/*; do
      base="$(basename "$file")"
      if [[ -f "$file" && "$base" =~ $licence_name && "$base" != *.html ]]; then
        local label="$name$folder/$base"
        install -Dm644 "$file" "$destination/${label//\//-}"
        copied=1
      fi
    done
  done
  if [[ "$copied" -eq 0 ]]; then
    echo "third_party/$name is compiled into PDFium but has no licence file" >&2
    exit 1
  fi
}

install -Dm644 "$pdfium/LICENSE" "$destination/pdfium-LICENSE"

# Paths are relative to the build folder, two levels below the source.
libraries="$(grep -o '\.\./\.\./third_party/[^/ ]*/' "$inputs" | sort -u | cut -d/ -f4)"
for library in $libraries; do
  copy_licences "$library"
done

echo "Licences collected for PDFium and:" $libraries
