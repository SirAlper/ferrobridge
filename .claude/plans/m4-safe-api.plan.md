# Plan: Milestone 4 — Safe API

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 4 — Safe API ("The user can create and use a C++ object without writing any `unsafe`; exceptions arrive as `Result`")
**Complexity**: Large
**Status**: Complete (2026-11-27, approved together with 5 decisions)

## Summary
Today's generated code is safe but **not idiomatic**. The user deals with `Vehicle_new_i32(4)`, `vehicle.pin_mut().drive(..)`, `let_cxx_string!`, `CxxVector` and `cxx::Exception`. In this milestone `bridge.rs` also generates a **safe Rust layer** on top of the cxx layer (`ffi`). This layer has `Vehicle::new(4)?`, `vehicle.drive(12.5)?`, `&str` / `String`, `&[T]` / `Vec<T>`, real Rust `enum`s and a single error type, `ferrobridge::Error`. The "Safe API" column of the architecture's type table comes to life in this milestone.

Usage example (the goal):
```rust
use mylib::{Vehicle, greet, Mode};

let mut vehicle = Vehicle::new_i32(4)?;     // C++ constructor → Result<Vehicle, ferrobridge::Error>
vehicle.drive(12.5)?;                      // &mut self, no Pin
let km = vehicle.odometer();               // noexcept → a plain value
let text: String = greet("Ada")?;          // &str in, String out
let next: Mode = mylib::next(Mode::Auto)?; // a real Rust enum, exhaustive `match`
```

## Design (decisions accepted with approval)
1. **A two-level safe API.**
   - Safe methods are generated as an inherent `impl ffi::Widget { .. }` on cxx's opaque type (`&self` / `self: Pin<&mut Self>`). So the same API works on an owned object, on `&ffi::Widget` and on the `Pin<&mut ffi::Widget>` coming from M3's borrowed returns.
   - On top comes a thin **owning wrapper**: `pub struct Widget { inner: UniquePtr<ffi::Widget> }`. It reaches const methods through `Deref<Target = ffi::Widget>`. It forwards mutating methods with `&mut self`, hiding `Pin` from the user. Constructors become `Widget::new*` and static methods `Widget::max_wheels()`.
   - The cost: so that cxx's own method names do not clash, they become `raw_<name>` in the bridge (`#[cxx_name]` keeps the C++ name). The `ffi` layer stays `pub` as an escape hatch.
   - The wrapper's invariant is "the `UniquePtr` inside is not null": if null arrives, the wrapper is never created (see 4).
2. **The runtime crate `ferrobridge`** (`crates/ferrobridge`, new). It contains `Error`, `Result<T>` and small helpers used by the generated code (`#[doc(hidden)] pub mod __private`). Its only dependency is `cxx`, `#![forbid(unsafe_code)]`. The user's crate adds it as a dependency. Alternatively a separate `Error` could be generated per header; but then errors from two headers would be different types and would not combine with `?`.
3. **The safe layer is in the same `bridge.rs` file**, next to `pub mod ffi`. The number of output files and inclusion with `#[path]` do not change (ADR-0004).
4. **Type mapping (safe layer):**

| cxx layer | Safe layer | Note |
|---|---|---|
| `Result<T>` (`cxx::Exception`) | `Result<T, ferrobridge::Error>` | The error also carries which C++ item it came from (`own::Widget::adopt: ...`) |
| `&CxxString` parameter | `&str` | No shim: `let_cxx_string!` in the generated code |
| `UniquePtr<CxxString>` return | `String` | Invalid UTF-8 → `Error::Utf8` |
| `&CxxString` return (method) | `Result<&str>` | Lifetime tied to the receiver |
| `&CxxVector<T>` parameter (T: number) | `&[T]` | Copied on every call (documented) |
| `&CxxVector<CxxString>` parameter | `&[&str]` | |
| `UniquePtr<CxxVector<T>>` return (T: number / `CxxString`) | `Vec<T>` / `Vec<String>` | |
| Constructor / class returned by value (`UniquePtr<T>`, cannot be null) | `T` (the wrapper) | `Error::NullPointer` if null arrives (defensive) |
| `owned` pointer / `std::unique_ptr<T>` return (may be null) | `Option<T>` | |
| `UniquePtr<T>` parameter (ownership passes to C++) | `T` (by value) | |
| shared enum | a real `pub enum` | If the value from C++ is undefined, `Error::UnknownEnumValue`; so a function returning an enum returns `Result` even when it is `noexcept` |
| `&T`, `Pin<&mut T>`, `&mut f64`, `Pin<&mut CxxString>`, `Pin<&mut CxxVector<T>>`, `CxxVector` of classes | **as is** | No copy-and-write-back for mutable strings/vectors: if C++ keeps the reference, the copy silently goes stale |

5. **Thread safety is not assumed.** cxx's opaque types are `!Send` / `!Sync`. The wrappers stay that way. Whether a C++ class is thread-safe cannot be known without an annotation (v0.2: `send = true` in `ferrobridge.toml`).

## Scope
**In:** the 5 decisions above; a safe layer for every fixture; e2e tests using the safe layer; ADR-0007.

**Out (with reasons):**
- `Clone` (copy constructor): the IR does not record copy constructors; v0.2.
- Inheritance → traits, operators → `std::ops`: out of PRD scope (v0.2).
- Turning vectors of classes into `Vec<T>`: the elements are opaque and cannot be held by value (safety rule 3).
- Nullable parameters (`Option<&T>`), pointer + length → slices: carried over from M3, v0.2.
- `ferrobridge-build` (the `build.rs` helper): M5.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Bridge generation | `crates/ferrobridge-codegen/src/bridge.rs:16` | `quote!` → `syn::File` → `prettyplease`; namespace blocks |
| Method declarations | `crates/ferrobridge-codegen/src/bridge.rs:168` | `function_decl`: receiver, parameters, `Result` wrapping, the `'a` lifetime |
| Enums | `crates/ferrobridge-codegen/src/bridge.rs:107` | `shared_enum`: values are checked against C++ at compile time |
| Type spelling | `crates/ferrobridge-codegen/src/spelling.rs:16` | `rust_type` / `cpp_type` one to one; the new `safe_type` for the safe layer follows the same pattern |
| Naming | `crates/ferrobridge-analysis/src/naming.rs:156`, `:194` | ADR-0005; safe names are derived from these (`Vehicle_new_i32` → `Vehicle::new_i32`) |
| Errors | `crates/ferrobridge-parser/src/lib.rs:66` | A `thiserror` enum |
| Tests | `crates/ferrobridge-codegen/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/ownership.rs` | Snapshots + e2e with real C++; a lock in counter tests |
| Logging | — | None; the CLI output does not change |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge/{Cargo.toml,src/lib.rs}` | CREATE | `Error`, `Result`, the `__private` helpers |
| `Cargo.toml` (root) | UPDATE | The new crate, the dependency graph comment |
| `crates/ferrobridge-cli/Cargo.toml` | UPDATE | Bin target `doc = false` (so the doc name does not clash with the `ferrobridge` library) |
| `crates/ferrobridge-codegen/src/safe.rs` | CREATE | Safe layer generation: enums, wrappers, `impl ffi::T`, free functions, conversions |
| `crates/ferrobridge-codegen/src/{bridge,spelling,lib}.rs` | UPDATE | Raw methods become `raw_*`; adding the safe layer to the file |
| `crates/ferrobridge-codegen/tests/snapshots/*` | UPDATE | All `bridge.rs` snapshots |
| `crates/ferrobridge-e2e/{Cargo.toml,src/generated/**}` | UPDATE | The `ferrobridge` dependency, regeneration |
| `crates/ferrobridge-e2e/tests/*.rs` | UPDATE / CREATE | Tests move to the safe API; each fixture keeps one "escape hatch" test using the raw layer; `idiomatic.rs`: `pin_mut`, `let_cxx_string`, `cxx::`, `unsafe` do not appear in the test sources |
| `docs/adr/0007-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Decisions 1–3 |
| `README.md`, PRD, `CLAUDE.md`, project notes | UPDATE | Usage examples with the safe API; M4 status |

## Tasks
Each task is done with TDD: a failing test first, then code, then verification. First a short **cxx experiment** outside the repo (as in M2 and M3). To try:
- An inherent `impl` on an opaque type and a method renamed with `#[cxx_name]`
- The real Rust form of a shared enum (does cxx generate it as a `struct` + constants?)
- `CxxVector::new()` + filling it from a slice
- `Deref` on a wrapper with a `UniquePtr` field
- The generated code being clean under the workspace lints (`clippy -D warnings`, `missing_docs`)

### Task 1: The runtime crate `ferrobridge`
- **Action**: `Error` (`Cpp { item, message }`, `Utf8 { item }`, `NullPointer { item }`, `UnknownEnumValue { ty, value }`), `Result<T>`, `Display` ("own::Widget::adopt: engine failure"), `std::error::Error`. The `__private` module: `cpp_error(item) -> impl FnOnce(cxx::Exception) -> Error`, `string_from_cxx`, `vec_from_cxx`, `cxx_vector_from_slice`, `strings_from_cxx`.
- **Mirror**: `parser/src/lib.rs:66`
- **Validate**: `cargo test -p ferrobridge`

### Task 2: Renaming the raw layer
- **Action**: methods in the bridge become `raw_<name>` + `#[cxx_name = "<C++ name>"]`. Shims and free functions already do not clash inside `ffi`; they do not change.
- **Mirror**: `bridge.rs:168`, `cxx_name_attr`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot review; the e2e tests compile with the new names

### Task 3: Safe type mapping (`safe.rs`)
- **Action**: `BridgeType` / `Returns` → the safe Rust type + the conversion expression in both directions (see decision 4). Table tests: the generated signature and conversion for every row.
- **Mirror**: `spelling.rs:16`
- **Validate**: `cargo test -p ferrobridge-codegen safe::`

### Task 4: Generating the safe layer
- **Action**:
  - Real `pub enum`s, `From<Enum> for ffi::Enum` and `TryFrom<ffi::Enum> for Enum`.
  - A wrapper `struct` for every class: `Deref`, `pin_mut()`, `into_inner()`, `from_unique_ptr() -> Option<Self>`, constructors and static methods.
  - Safe methods inside `impl ffi::Class`; `&mut self` forwarders for mutating methods.
  - Safe free functions at module level.
  - A doc comment on every item naming its source (`/// C++: \`own::Widget::adopt\``).
  - Name clashes: the wrapper `struct` is defined with named fields (not a tuple struct), so it does not clash with a function in the value namespace.
- **Mirror**: `bridge.rs:16`, `:79`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot review; the generated file parses with `syn`

### Task 5: End-to-end tests
- **Action**:
  - The existing e2e tests move to the safe API (classes, enums, strings, ownership, vectors).
  - Each fixture keeps one "escape hatch" test using the raw layer.
  - New tests: a C++ function returning invalid UTF-8 → `Error::Utf8`; a function returning an undefined enum value → `Error::UnknownEnumValue`; the C++ item's name in the error message.
  - `idiomatic.rs`: `pin_mut`, `let_cxx_string`, `CxxString`, `cxx::`, `unsafe` do not appear in the safe API tests.
- **Mirror**: `crates/ferrobridge-e2e/tests/ownership.rs`
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 6: Review, ADR and documents
- **Action**:
  - `/ecc:rust-review`. Focus: the wrapper's non-null invariant, correctness of `Deref` and `Pin`, lifetimes, the cost of hidden copies.
  - ADR-0007.
  - The README's usage section is rewritten with the safe API.
  - PRD, plan notes, CLAUDE.md, project notes.
- **Validate**: the README example compiles as a copy of one of the e2e tests

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # no bin/lib doc name clash
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| cxx does not allow an inherent `impl` on an opaque type, or renaming with `#[cxx_name]` does not work on methods | Low | A cxx experiment first. Fallback design: safe methods only on the wrapper, with `WidgetRef<'a>` / `WidgetMut<'a>` view types for borrowed objects |
| The generated safe code trips the workspace lints (`missing_docs`, `clippy::all`) | Medium | A doc comment on every item; if needed, a module-level `allow` for specific lints only, with a justification |
| A doc/output name clash between `ferrobridge` (library) and `ferrobridge` (CLI binary) | Medium | `doc = false` on the CLI bin target; `cargo doc` in validation |
| Hidden copies (`&[T]` → `CxxVector`, `String` returns) are slow for large data | Medium | Documented; the raw `ffi` layer stays open for copy-free use |
| A `noexcept` function returning an enum returning `Result` is surprising | Low | The doc comment states the reason; the alternative is a panic or silent truncation on an undefined value from C++, both unacceptable |
| Renaming raw methods to `raw_*` breaks M2/M3 users | Low | Before v0.1, no external users |
| Scope size (Large) | Medium | Copy constructors, inheritance and mutable string/vector conversions deliberately left out |

## Acceptance
- [x] A safe layer is generated for all fixtures; the `ffi` layer stays as an escape hatch
- [x] The e2e tests are written with the safe API; no `unsafe`, `pin_mut`, `let_cxx_string`, `cxx::` in the test sources
- [x] All C++ exceptions arrive as `ferrobridge::Error` and say which C++ item they came from
- [x] Invalid UTF-8 and undefined enum values are `Err`, not panics
- [x] Owning wrappers are never null; returns that may be null are `Option`
- [x] `cargo doc` and all validation commands pass
- [x] ADR-0007 written; Rust review done
- [x] Patterns mirrored, not reinvented

## Implementation notes
### cxx experiment (2026-11-27, cxx 1.0.205, outside the repo)
All of it worked:
- An inherent `impl ffi::Widget` on an opaque type.
- Renaming a method with `#[cxx_name = "value"] fn raw_value(self: &Widget)`.
- An owning wrapper with `Deref<Target = ffi::Widget>`.
- `CxxVector::<i32>::new()` + `push`.
- Putting the result of `ffi::Widget_new(..)` into the wrapper.
- `clippy -D warnings` and `missing_docs` clean in the experiment crate.

**Finding 1:** cxx generates a shared enum in Rust not as a real `enum` but as a `#[repr(transparent)] struct { repr }` + constants. An undefined value can be caught in a `match` with `other => other.repr`. Converting to a real Rust `enum` with `TryFrom` is therefore both needed and possible.

**Finding 2 (deviation):** elements cannot be added to a `CxxVector<CxxString>` from Rust (`push` exists only for trivial types). The `&CxxVector<CxxString>` → `&[&str]` row in decision 4 was removed; string vector parameters stay as raw `&CxxVector<CxxString>`. String vector **returns** remain `Vec<String>`.

### Result (2026-11-27)
All tasks complete. 134 tests in total: IR 6, parser 11, analysis 48, codegen 12, CLI 13 + parse, runtime 5 + a doc test, e2e 33 (safe API, raw layer, README example, idiomaticity check). Homebrew libclang 23.1.6 and Apple libclang 21.0.0 give the same result. `fmt`, `clippy -D warnings` and `cargo doc` clean.

### Deviations and findings
- **No `&[&str]` input** (see finding 2): string vector parameters stay as raw `&CxxVector<CxxString>`.
- **Lifetimes are never written.** Inference works for methods taking `self: Pin<&mut Self>` too; an explicit `'a` tripped clippy's `needless_lifetimes`. The explicit `'a` in the raw shim layer (M3) did not change.
- **`cargo fmt` was formatting the generated files again.** In M3 only the `ffi` module carried `#[rustfmt::skip]`. Now every top-level item of the safe layer carries it too.
- **Lints:** because C++ names are kept, every safe item carries a narrow `#[allow(non_snake_case, non_camel_case_types, clippy::should_implement_trait, clippy::too_many_arguments)]`. No file-level inner attribute was used, so as not to break users who include the file with `include!`.
- **The deref coercion limit:** passing the wrapper to a function expecting `&ffi::T` in the form `f(&g()?)` does not compile, because the type of `?` cannot be inferred from the context. The value must be bound to a variable first. Documented in the README and ADR-0007.
- **Wrappers are generated only for classes that appear in the bridge as `UniquePtr<T>`.** cxx generates the C++ code needed for `UniquePtr<T>` only for types that appear in the bridge. Classes seen only by reference have no wrapper; their safe methods are on `ffi::T`, and their static methods are `ffi::T::name()`.
- **Fixture additions:** `text::raw_bytes` (invalid UTF-8) and `io::broken_mode` (undefined enum value). Both error paths are tested with real C++.
- **E2e layout:** safe API tests in one file per fixture. The raw layer in `raw_layer.rs`. `idiomatic.rs` checks that `unsafe`, `let_cxx_string`, `CxxString`, `CxxVector`, `cxx::`, `ffi::`, `.raw_` do not appear in the other test files; verified with a mutation experiment. `readme.rs` is a compiled copy of the README example.
- **TDD order:** the table tests of `safe.rs` were added after the code was written. The behavior had been tested beforehand with the e2e build and clippy.
- **Still open:** `Clone` (copy constructor), inheritance → traits, mutable string/vector conversions, vectors of classes, a `Send`/`Sync` annotation (v0.2). The `-fno-exceptions` question has to be decided before M5.
