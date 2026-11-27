# ADR-0007: A safe API layer on top of the cxx layer

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

The cxx bridge generated in M2 and M3 was safe but not idiomatic. Users had to deal with `Vehicle_new_i32(4)`, `vehicle.pin_mut().drive(..)`, `let_cxx_string!`, `CxxVector` and `cxx::Exception`. The PRD's hypothesis is a "safe **and idiomatic**" Rust API, and the architecture's type mapping table has a "Safe API" column. C++ objects appear in Rust in three forms: owned (`UniquePtr<T>`), borrowed (`&T`) and mutably borrowed (`Pin<&mut T>`, including M3's borrowed returns). The safe API must work for all three. Per safety rule 3, non-trivial C++ objects cannot be held by value in Rust.

## Decision

Next to cxx's `ffi` module, `bridge.rs` also contains a safe layer:
1. Safe methods are written as an inherent `impl ffi::T` on cxx's opaque type (`&self` / `self: Pin<&mut Self>`). cxx's own methods become `raw_<name>` so they do not clash; the C++ name is kept with `#[cxx_name]`.
2. For every class that appears in the bridge as `UniquePtr<T>`, an owning wrapper is generated: `pub struct T { inner: UniquePtr<ffi::T> }`, never null inside, `Deref<Target = ffi::T>`, `&mut self` forwarders for mutating methods, `T::new*` constructors and static methods.
3. All errors are a single `ferrobridge::Error` type from the new runtime crate `ferrobridge`; an error carries the name of the C++ item it came from.
4. Conversions: `&str` / `String`, numeric slices / `Vec<T>`, `Vec<String>`, real Rust `enum`s (`TryFrom`), `Option<T>` for owned returns that may be null. Mutable string/vector references and vectors of classes stay as they are.

## Alternatives considered

### Alternative 1: safe methods only on the wrapper + `TRef<'a>` / `TMut<'a>` view types for borrowed objects
- **Pros**: The `ffi` layer is never touched.
- **Cons**: Three types per class and every method three times; M3's borrowed returns must be wrapped in view types; users learn three names for one object.
- **Why not chosen**: Writing an inherent `impl` on an opaque type works in cxx (verified by experiment) and covers all three forms with a single method definition.

### Alternative 2: no wrapper, users use `UniquePtr<ffi::T>` and `pin_mut()`
- **Pros**: The least generated code.
- **Cons**: `vehicle.pin_mut().drive(..)` is not idiomatic; constructors stay free functions.
- **Why not chosen**: Removing exactly this friction is the purpose of M4.

### Alternative 3: generating a separate error type per header (no runtime crate)
- **Pros**: No extra dependency in the user's crate.
- **Cons**: Errors from two headers have different types and do not combine with `?`; shared conversion code is repeated in every file.
- **Why not chosen**: A small crate without `unsafe`, depending only on `cxx`, is cheaper.

### Alternative 4: copy-and-write-back for mutable `std::string&` / `std::vector<T>&` (`&mut String`)
- **Pros**: A more idiomatic signature.
- **Cons**: If C++ keeps the reference, the copy silently goes stale; the meaning changes.
- **Why not chosen**: Safety rule 1 (don't guess). So the raw `Pin<&mut CxxString>` stays.

## Consequences

### Positive
- `let mut v = Vehicle::new_i32(4)?; v.drive(12.5)?;`: no `Pin`, cxx type or `unsafe` needed (checked automatically in the e2e tests).
- Invalid UTF-8 and an undefined enum value coming from C++ are an `Err`, not a panic.
- The raw `ffi` layer stays as an escape hatch for copy-free use and for types the safe layer does not convert.

### Negative
- cxx methods were renamed to `raw_*` (before v0.1, no external users).
- The user's crate depends on the `ferrobridge` runtime crate.
- `&[T]` parameters and `String`/`Vec` returns are copied on every call.
- Deref coercion from the wrapper to a place expecting `&ffi::T` works while the value is bound to a variable; it does not work in the form `f(&g()?)`.
- A `noexcept` function returning an enum also returns `Result` (an undefined value is possible).

### Risks
- If a C++ method has the same name as a wrapper helper (`pin_mut`, `into_inner`, `from_unique_ptr`), the forwarder on the wrapper is generated with a `_` suffix; for a const method, the wrapper's helper is chosen over Deref. Mitigation: documented; the C++ method is reached with `(*w).name()`.
- A C++ class named `ffi` clashes with the module name. Mitigation: a known limitation in v0.1.
