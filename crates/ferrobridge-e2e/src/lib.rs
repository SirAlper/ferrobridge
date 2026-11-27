//! Generated bridges for the end-to-end tests.
//!
//! The files under `src/generated/` are the output of `ferrobridge generate`; do not edit them by hand.
//! The command to regenerate them is in `README.md`. A test in `ferrobridge-cli` checks that the
//! committed files are identical to the generator's current output.
//!
//! The C++ side is `tests/fixtures/<name>/input.h` and `impl.cc`; `build.rs` compiles them all.

/// `tests/fixtures/classes`: classes, constructors, methods, overloads, exceptions.
#[path = "generated/classes/bridge.rs"]
pub mod classes;

/// `tests/fixtures/enums`: scoped/unscoped enums, different underlying types, negative values.
#[path = "generated/enums/bridge.rs"]
pub mod enums;

/// `tests/fixtures/strings`: `std::string` parameters and returns.
#[path = "generated/strings/bridge.rs"]
pub mod strings;

/// `tests/fixtures/ownership`: owned/borrowed pointers annotated in `ferrobridge.toml`,
/// output parameters, live object counters.
#[path = "generated/ownership/bridge.rs"]
pub mod ownership;

/// `tests/fixtures/vectors`: `std::vector` parameters and returns.
#[path = "generated/vectors/bridge.rs"]
pub mod vectors;

/// `tests/fixtures/slices`: pointer + length pairs annotated `{ slice = "len" }`.
#[path = "generated/slices/bridge.rs"]
pub mod slices;

/// `tests/fixtures/inline_and_constants`: `static inline` functions and constants.
#[path = "generated/inline_and_constants/bridge.rs"]
pub mod inline_and_constants;

/// `tests/fixtures/multi_header`: a single bridge with two headers; types from `geometry.h`
/// in functions from `input.h`.
#[path = "generated/multi_header/bridge.rs"]
pub mod multi_header;

/// `tests/fixtures/value_types`: value types that are `#[repr(C)]` structs in Rust.
#[path = "generated/value_types/bridge.rs"]
pub mod value_types;

/// `tests/fixtures/operators`: `std::ops` impls and named operator methods.
#[path = "generated/operators/bridge.rs"]
pub mod operators;

/// `tests/fixtures/no_exceptions`: `[library] exceptions = false`.
#[path = "generated/no_exceptions/bridge.rs"]
pub mod no_exceptions;
