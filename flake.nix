{
  description = "Verifiabl extraction module toolchain";

  # This lock pins the toolchain that builds dist/, so the module's hash
  # depends on it. A flake that consumes this one must not override its inputs.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      # The platforms wasi-sdk publishes release builds for.
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      # Instantiate each package set once, without impure user configuration.
      pkgsFor = forAllSystems (
        system:
        import nixpkgs {
          inherit system;
          config = { };
        }
      );
      packagesFor =
        pkgs:
        let
          wasi-sdk = pkgs.callPackage ./nix/wasi-sdk.nix { };
        in
        rec {
          inherit wasi-sdk;
          pdfium-wasi = pkgs.callPackage ./nix/pdfium-wasi.nix { wasiSdk = wasi-sdk; };
          zxing-wasi = pkgs.callPackage ./nix/zxing-wasi.nix { wasiSdk = wasi-sdk; };
          module = pkgs.callPackage ./nix/module.nix {
            wasiSdk = wasi-sdk;
            pdfiumWasi = pdfium-wasi;
            zxingWasi = zxing-wasi;
          };
          module-debug = pkgs.callPackage ./nix/module.nix {
            wasiSdk = wasi-sdk;
            pdfiumWasi = pdfium-wasi;
            zxingWasi = pkgs.callPackage ./nix/zxing-wasi.nix {
              wasiSdk = wasi-sdk;
              debug = true;
            };
            debug = true;
          };
          notices = pkgs.callPackage ./nix/notices.nix {
            wasiSdk = wasi-sdk;
            pdfiumWasi = pdfium-wasi;
          };
          # The files dist/ holds.
          dist = pkgs.runCommand "verifiabl-extract-dist" { } ''
            mkdir $out
            cp ${module}/* ${notices}/* $out/
          '';
          stand-ins = pkgs.callPackage ./nix/stand-ins.nix { };
        };
      # Copies a built folder's files into the checkout, from the extraction folder.
      update =
        pkgs: name: folder: built:
        pkgs.writeShellApplication {
          name = "update-${name}";
          text = ''
            if [ ! -f abi/extraction-abi.json ]; then
              echo "Run this in the extraction folder." >&2
              exit 2
            fi
            install -m644 ${built}/* ${folder}/
            echo "Wrote ${folder}/ from ${built}."
          '';
        };
      # Refuses to write dist/ on a platform whose module differs.
      refuse =
        pkgs: name:
        pkgs.writeShellApplication {
          name = "update-${name}";
          text = ''
            echo "dist/ is built on ${referenceSystem}, not ${pkgs.stdenv.hostPlatform.system}." >&2
            exit 1
          '';
        };
      # dist/ holds the x86_64-linux build: the module differs slightly on
      # every other platform.
      referenceSystem = "x86_64-linux";
      # Fails when a committed file differs from the one built from source.
      compare =
        pkgs: name: script:
        pkgs.runCommand "verifiabl-extract-${name}-check" { nativeBuildInputs = [ pkgs.jq ]; } ''
          ${script}
          touch $out
        '';
    in
    {
      packages = forAllSystems (system: packagesFor pkgsFor.${system});

      # nix run .#update-dist and .#update-stand-ins, from the extraction folder.
      apps = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          packages = packagesFor pkgs;
          app = program: {
            type = "app";
            program = nixpkgs.lib.getExe program;
          };
        in
        {
          update-dist = app (
            if system == referenceSystem then update pkgs "dist" "dist" packages.dist else refuse pkgs "dist"
          );
          update-stand-ins = app (update pkgs "stand-ins" "stand-ins" packages.stand-ins);
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          packages = packagesFor pkgs;
        in
        {
          # Every stand-in in expected.json has a source, and each committed
          # module is the one its source assembles to.
          stand-ins = compare pkgs "stand-ins" ''
            committed=${./stand-ins}
            built=${packages.stand-ins}
            if ! diff -u <(jq -r '.[].module' $committed/expected.json | sort -u) <(ls $built | sort); then
              echo "stand-ins/expected.json and the stand-ins' sources name different modules." >&2
              exit 1
            fi
            for module in $built/*.wasm; do
              if ! cmp -s "$module" "$committed/$(basename "$module")"; then
                echo "stand-ins/$(basename "$module") differs from its source; run nix run .#update-stand-ins." >&2
                exit 1
              fi
            done
          '';
        }
        // nixpkgs.lib.optionalAttrs (system == referenceSystem) {
          dist = compare pkgs "dist" ''
            for file in verifiabl-extract.wasm verifiabl-extract.json THIRD-PARTY-NOTICES.txt; do
              if ! cmp -s ${packages.dist}/$file ${./dist}/$file; then
                echo "dist/$file differs from the one built from source; run nix run .#update-dist." >&2
                exit 1
              fi
            done
          '';
        }
      );

      # Entering the shell compiles PDFium once, which takes several minutes.
      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          packages = packagesFor pkgs;
        in
        {
          # For the crates' tests and lints and the fuzz targets. nix/ builds
          # the module with its own tools.
          default = pkgs.mkShell {
            # nixpkgs' Rust carries the wasm32-unknown-unknown std.
            packages = [
              pkgs.cargo
              pkgs.cargo-audit
              pkgs.rustc
              # Editor and lint tools from the same toolchain.
              pkgs.clippy
              pkgs.rust-analyzer
              pkgs.rustfmt
            ];
            # nixpkgs' rustc ships without std's source, which rust-analyzer reads.
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            # build.rs reads these when clippy or an editor checks the crate.
            WASI_SDK_PATH = "${packages.wasi-sdk}";
            PDFIUM_WASI_DIR = "${packages.pdfium-wasi}";
          };
        }
      );

      formatter = forAllSystems (system: pkgsFor.${system}.nixfmt);
    };
}
