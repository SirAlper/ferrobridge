# Changelog

All notable changes to ferrobridge are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/). All crates share one version (ADR-0012).

## [Unreleased]

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

- Rust 1.85 or later, libclang 16 or later, a C++17 compiler. Tested on Linux (libclang 16 and 18) and macOS (Homebrew LLVM, Apple libclang).

### Known limits

- Function templates, inheritance mapped to traits, callbacks/lambdas, direct field access on non-value classes and CMake integration are not supported yet (planned for v0.2 and later).
- Platform-dependent integer types (`long`, `size_t` in signatures that reach Rust) are not bound.
- `operator[]` is not bound (no bounds checks).
- ferrobridge cannot know the preconditions of C++ functions: a call that is undefined behavior in C++ is undefined behavior from Rust too.

[Unreleased]: https://github.com/SirAlper/ferrobridge/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SirAlper/ferrobridge/releases/tag/v0.1.0
