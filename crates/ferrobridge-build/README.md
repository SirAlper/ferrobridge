# ferrobridge-build

Compiles [ferrobridge](https://github.com/SirAlper/ferrobridge) bridges from `build.rs`: the generated C++ shims, the cxx bridges and your C++ sources go into one static library, with the same include paths, defines and C++ standard (C++17 by default).

There are two modes:

- **Committed files (default).** You run `ferrobridge generate` once, commit `bridge.rs`, `shim.h` and `shim.cc`, and `build.rs` compiles them. No libclang is needed to build.
- **Build-time generation (`generate` feature).** `build.rs` reads the headers with libclang and generates the bridge into `OUT_DIR`. Use it when the generated files must not enter your repository (for example because of the C++ library's license). Needs libclang 16 or later on every build.

```rust
// build.rs (committed files)
fn main() {
    let mut build = ferrobridge_build::Build::new();
    build
        .committed("src/ffi")         // bridge.rs, shim.h, shim.cc
        .include_dir("include")       // the C++ library's headers
        .include_dir("src/ffi")       // the bridge's `include!("shim.h")`
        .file("cpp/vehicle.cc");      // the library's implementation
    if let Err(error) = build.compile("vehicle") {
        panic!("{error}");
    }
}
```

```rust
// build.rs (ferrobridge-build = { version = "0.1", features = ["generate"] })
fn main() {
    let mut build = ferrobridge_build::Build::new();
    build
        .generate(
            ferrobridge_build::Generate::new("vehicle")
                .header("include/vehicle.h")
                .config("ferrobridge.toml"),
        )
        .include_dir("include")
        .file("cpp/vehicle.cc");
    if let Err(error) = build.compile("vehicle") {
        panic!("{error}");
    }
}

// src/lib.rs
pub mod vehicle {
    ferrobridge::include_bridge!("vehicle");
}
```

The crate using the bridge also depends on `cxx` and the [`ferrobridge`](https://crates.io/crates/ferrobridge) runtime (same minor version as this crate). Full guide: the [ferrobridge README](https://github.com/SirAlper/ferrobridge#readme).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
