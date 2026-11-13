# Verifiabl extraction module

## Rules

- The module must import nothing. Do not add WASI or host imports; stub what a C or C++ dependency needs inside the module.
- Change `abi/extraction-abi.json` with any export change, and bump `abiVersion` when an export's meaning changes. Update every host in the same change.
- Read [DEVELOPMENT.md](DEVELOPMENT.md) before you build or test. Build the module in `nix/module.nix`. Write `dist/` with `nix run .#update-dist` on x86_64 Linux. Do not edit `dist/` by hand.
- A released version names one module. When `dist/verifiabl-extract.wasm` changes after a release, increase the version in `version.json` and add its section to `CHANGELOG.md` before an SDK releases the new module.
- The build writes `dist/THIRD-PARTY-NOTICES.txt` from what it links. Publish it with every copy of the module. When the build says a dependency has no licence text, or the toolchain changed, add the text to `licenses/` from its source.
- Keep payslip values out of logs, errors and fixtures. Use synthetic fixtures. A fixture that names a person uses Ana Leota. One that names an organisation uses an `Example` name of its role, such as `Example Payroll Pty Ltd` or `Example Lending Pty Ltd`. Never use real people or companies, or stock names such as Jane Doe or Acme.
- Hosts treat the module's output as untrusted: check lengths and offsets against the instance's memory, and report a module that breaks the interface as `unreadable`. Every host passes `stand-ins/expected.json`.
- Catch C++ exceptions inside the module, in `native/`. One that leaves an export is not a trap, and hosts report it differently.
- Keep any input that made the module or a host misbehave, as [TESTING.md](TESTING.md) describes.
