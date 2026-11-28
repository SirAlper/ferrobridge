# Changelog

All notable changes to ferrobridge are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/). All crates share one version (ADR-0012).

## [Unreleased]

### Added

- **Flags from the build system** (ADR-0013): `parse`, `generate` and `suggest` take `--flags-file <file>` (one argument per line, repeatable) and `--compile-commands <db> --for <source>` (an entry of a `compile_commands.json`, read through libclang). A filter keeps what libclang needs (defines, include paths, forced includes, target and language flags), summarizes dropped warning, optimization and code generation flags in one `ℹ` line per kind, and lists unknown flags in a `⚠`. Arguments after `--` are never filtered.
- **`generate --depfile <path>`:** a Make-syntax depfile listing every file the headers include, directly or not.
- **`generate --cxx-glue`:** also writes cxx's C++ side (`bridge.rs.cc`, `bridge.rs.h`, `rust/cxx.h`) through `cxx-gen` 0.7.198, for build systems that compile the bridge's C++ themselves.
- **`ferrobridge-build`:** `Build::compile_commands` and `Build::flags_file` (`generate` feature). Their flags reach both libclang and the C++ compiler (target flags only libclang); Cargo reruns on each header that was read instead of scanning include directories.

### Changed

- `--std` has no fixed default any more: an explicit `--std` wins, then a `-std=` from the flag sources, then `c++17` (unchanged when no flag source is given).

### Fixed

- A method or function returning `const T` by value (e.g. PX4's `AlphaFilter<T>::update`) produced a bridge that did not compile (cxx spells the function's type without the `const`); it is now called through the shim.
- Explicit specializations of a class template (`template <> struct is_floating_point<float>`) became several classes with the same Rust name; they are now reported as not bound.

## [0.1.0] - 2026-11-27

The first release. ferrobridge reads C++ headers with libclang, takes ownership information from `ferrobridge.toml` and generates a cxx bridge, a C++ shim and a safe Rust API, for calling C++ from Rust without hand-written bridges or `unsafe`.

### Added

- **The `ferrobridge` command** (`cargo install ferrobridge-cli`): `doctor` (which libclang is loaded), `parse` (the IR as JSON), `generate` (`bridge.rs`, `shim.h`, `shim.cc`), `suggest` (a commented-out `ferrobridge.toml` draft).
- **Supported C++:** free functions, classes (constructors, methods including virtual ones, static methods), enums, namespaces, overloads (ADR-0005), `const`/`constexpr` constants, `static inline` functions, member operators (`std::ops`), class templates instantiated with `[templates]` (ADR-0009), several headers in one bridge (ADR-0009).
- **Standard types:** `std::string` (`&str`/`String`), `std::unique_ptr<T>`, `std::vector<T>` (`&[T]`/`Vec<T>`), pointer + length → `&[T]`/`&mut [T]` (`{ slice = "len" }`).
- **Safe API** (ADR-0007): owning wrappers that run the C++ destructor, `&self`/`&mut self` methods, real Rust enums, `Option<T>` for nullable owned returns, one error type `ferrobridge::Error`; the raw cxx layer stays under `ffi` as an escape hatch.
- **Value types** (ADR-0010): trivially copyable classes with public fields become `#[repr(C)]` Rust structs; the layout is checked in C++ with `static_assert`.
- **Ownership annotations** (ADR-0006): `returns = "owned" | "borrowed"`, `params.<name> = "borrowed"`; pointers with unclear ownership are never guessed, they are reported with a suggested annotation.
- **Exceptions** become `Err`; `[library] exceptions = false` for libraries built without exceptions (ADR-0011).
- **`ferrobridge-build`:** compiles committed bridges from `build.rs` (ADR-0004), or generates them during the build with the `generate` feature (ADR-0008).
- **`ferrobridge` runtime crate:** `Error`, `Result`, `include_bridge!` and the conversion helpers used by generated code.
- **Demo:** ArduPilot `AP_Math` in `demo/ap_math/` (GPL-3.0-or-later, not published): about 82% of the API bound, including `Vector3f`, `Matrix3f` and `Quaternion`, with 59 annotation lines in `ferrobridge.toml`.

### Requirements

- Rust 1.85 or later, libclang 16 or later, a C++17 compiler. Tested on Linux (libclang 16 and 18) and macOS (Homebrew LLVM 23, Apple libclang). On macOS the libc++ headers of a recent Xcode SDK need a recent libclang; libclang 16 cannot read them.

### Known limits

- Function templates, inheritance mapped to traits, callbacks/lambdas, direct field access on non-value classes and CMake integration are not supported yet (planned for v0.2 and later).
- Platform-dependent integer types (`long`, `size_t` in signatures that reach Rust) are not bound.
- `operator[]` is not bound (no bounds checks).
- ferrobridge cannot know the preconditions of C++ functions: a call that is undefined behavior in C++ is undefined behavior from Rust too.

[Unreleased]: https://github.com/SirAlper/ferrobridge/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SirAlper/ferrobridge/releases/tag/v0.1.0
