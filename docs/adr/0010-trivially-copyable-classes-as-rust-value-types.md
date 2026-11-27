# ADR-0010: Trivially copyable classes become Rust value types

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

Per ADR-0007, every C++ class was an opaque cxx type, used in Rust behind `UniquePtr` or `Pin`. For small math types with public fields such as `Vector3f { float x, y, z; }` this is heavy: `v.x` cannot be accessed, every vector is heap-allocated, and changing it needs `Pin<&mut>`. Safety rule 3 forbids holding only *non*-trivially movable types by value.

## Decision

A class or instantiation becomes a `#[repr(C)] #[derive(Debug, Clone, Copy)] pub struct` in Rust if **all** of the following hold:
- not polymorphic, no base class
- no user-defined copy, move or destructor (except `= default`)
- all non-static fields public, not `const`, byte-aligned
- field types are numbers, `bool`, `c_char` or another value type
- its layout is known from libclang

Classes that do not meet these conditions stay opaque.

- It is introduced to cxx with `unsafe impl ::cxx::ExternType` (`Kind = Trivial`). The `unsafe` exists only in the generated inner layer, with a `// SAFETY:` justification.
- **The C++ compiler has the final word.** `shim.cc` checks with `static_assert`: `is_trivially_copyable`, `is_standard_layout`, `sizeof`, `alignof` and every field's `offsetof`. The Rust side also checks with `size_of`, `align_of` and `offset_of!`. cxx adds its own `IsRelocatable` check. If the analysis is wrong, the result is a compile error, not silent UB.
- Parameters and returns by value, `&T` / `&mut T` (no `Pin`); constructors return a value.
- `PartialEq` and `Default` are not derived; they may not carry the meaning of C++'s `==` and default constructor. `==` is bound from the C++ operator.
- Member operators become `std::ops` traits when they do not return an error (`Add`, `Mul<f32>`, `Neg`, `AddAssign`, `PartialEq`...). Named methods (`op_add`) always exist as well. Comparisons stay named methods.
- **`operator[]` is never bound.** In C++ it is usually used without bounds checks; an out-of-bounds index from safe Rust would be UB.

## Alternatives considered

### Alternative 1: opaque type + getter/setter shims for the fields
- **Pros**: No `unsafe impl` needed.
- **Cons**: Every field access is an FFI call, every object on the heap. Too heavy and not idiomatic for types like `Vector3f`.
- **Why not chosen**: Value types are at the heart of the project's "idiomatic" goal. The `static_assert`s make the compiler verify safety.

### Alternative 2: cxx's shared structs (a `struct` inside the bridge)
- **Why not chosen**: cxx generates the C++ definition of a shared struct itself. It would clash with the library's existing class.

## Consequences

### Positive
- `v.x`, `a + b`, `v.normalize()` (`&mut self`). No heap allocation or `Pin`.
- A class that is not a value type by design (private or `const` field, virtual function) automatically stays opaque.

### Negative
- A class with all fields public can be constructed from its fields in Rust; an invariant established by the C++ constructor is bypassed. Accepted, because a C++ class with all fields public allows the same.
- Because of two limitations of `prettyplease`, the generated code is post-processed as text after formatting: aliases and `SAFETY` comments.

### Risks
- A field type may have the same representation in C++ and Rust but different sets of valid values (e.g. `bool`). In C++ a `bool` can only be 0/1; Rust assumes the same.
