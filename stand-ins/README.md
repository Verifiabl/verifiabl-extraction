# Stand-in modules

These modules have the exports of the extraction module, but they break the interface or fail. They act as a decoder that a hostile file controls. A host runs each stand-in in place of the real module, for an image read, for a PDF read and for a pixel read. The host must report the status in `expected.json`:

- A module that breaks the interface gives `unreadable`, as a damaged file does.
- A module that does not return gives `timedOut`.
- A module that traps after a failed allocation gives `outOfMemory`. `alloc-fails.wasm` does this in `vf_alloc`, as the module does when its memory cannot hold the input. Pixels can be hundreds of megabytes, and no fixture is that large.

No stand-in can cause the host to crash, use all its memory or wait without limit.

Each `.wat` file describes the behaviour of its module. [`nix/stand-ins.nix`](../nix/stand-ins.nix) assembles the modules with Binaryen, and `nix run .#update-stand-ins` copies them here. `nix flake check` fails if a committed `.wasm` file is different from its source.
