# ferrobridge-cli

The `ferrobridge` command. It generates **safe and idiomatic** Rust bindings for C++ headers, on top of [cxx](https://cxx.rs). Ownership information that a header cannot express comes from a small annotation file (`ferrobridge.toml`); a pointer with unclear ownership is never guessed, it is reported with a suggested annotation.

```bash
cargo install ferrobridge-cli
ferrobridge doctor      # which libclang is loaded (16 or later is needed)
```

On macOS, point it at Homebrew's LLVM; otherwise Apple's libclang is loaded:

```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

| Command | What it does |
|---|---|
| `ferrobridge doctor` | Checks the environment: which libclang is loaded, and its version |
| `ferrobridge parse <header>` | Prints what ferrobridge sees in a header (the IR) as JSON |
| `ferrobridge generate <headers>... --out-dir <dir>` | Writes `bridge.rs` (safe API + raw cxx bridge), `shim.h` and `shim.cc`; every item that cannot be bound is printed with its reason |
| `ferrobridge suggest <header>` | Prints a commented-out `ferrobridge.toml` draft for every pointer or reference that needs an annotation |

```bash
ferrobridge generate include/vehicle.h --out-dir src/ffi \
  --include-as vehicle.h --shim-namespace ferrobridge::vehicle -- -I include
```

The generated files are compiled by [`ferrobridge-build`](https://crates.io/crates/ferrobridge-build) and use the [`ferrobridge`](https://crates.io/crates/ferrobridge) runtime crate (same minor version as this command). Full guide: the [ferrobridge README](https://github.com/SirAlper/ferrobridge#readme).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
