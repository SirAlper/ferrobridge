//! ArduPilot `AP_Math`, with the safe Rust API ferrobridge generates during the build.
//!
//! All `AP_Math` headers form a single bridge: `Vector3f`, `Matrix3f`, `Quaternion` are real
//! Rust structs (fields, methods, the `+ - * /` operators); CRCs take `&[u8]`; because
//! ArduPilot does not use exceptions (`ferrobridge.toml`: `[library] exceptions = false`),
//! functions return values directly.
//!
//! The unbound items and the assumptions based on annotations are listed at the top of the
//! generated file (`$OUT_DIR/ferrobridge/ap_math/bridge.rs`).
//!
//! This crate has no hand-written `unsafe` (`deny(unsafe_code)`). The `#[allow(unsafe_code)]`
//! on the bridge module covers only the generated inner layer (cxx's `ffi` and the justified
//! `ExternType` impls of the value types); the tests and examples are compiled with
//! `forbid(unsafe_code)`.

#![deny(unsafe_code)]

/// All headers of `AP_Math`.
#[allow(unsafe_code)] // The generated inner layer (cxx `ffi`, `ExternType`).
pub mod ap_math {
    ferrobridge::include_bridge!("ap_math");
}

pub use ap_math::*;
