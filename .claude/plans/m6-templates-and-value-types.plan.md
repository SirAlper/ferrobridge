# Plan: Milestone 6: Cross-header types, templates and a showable `AP_Math` demo

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 6: Cross-header types, templates and fields
**Inputs**:
- `.claude/plans/m5-apmath-measurement.md` (the "After M5" section and the correction note)
- `.claude/plans/m5-build-pipeline-and-demo.plan.md` (implementation notes)
- `demo/ap_math/`
**Complexity**: Large
**Status**: Complete (2026-11-27)

## Summary
The goal is to make the `AP_Math` demo showable to others. For that:
- `Vector2f`, `Vector3f`, `Matrix3f` and `Quaternion` become real structs in Rust: fields, methods and the `+ - * /` operators.
- Functions in other headers using these types open up too (`control.h`, `location.h`, `SCurve.h`).
- In a library without exceptions, not every call needs to return `Result`.
- An example program and a "hand-written cxx" vs "ferrobridge" comparison are added to the demo.

**Measurable goal:** **≥ 80%** of the `AP_Math` API (PRD; 31% today, same rough method).
- Template class members are counted once per template (as in the measurement).
- No `unsafe` in the demo's user code.
- `cargo run --example tour` works.

## Decisions (accepted with your approval)
1. **Cross-header types: multi-header bridges (ADR-0009).**
   - One bridge is generated from several headers: `ferrobridge generate a.h b.h ...`, `Generate::new("name").header(..)`.
   - The parser reads them all in a single synthetic translation unit. Analysis recognizes types across all headers.
   - A type from a header not added to the bridge is still skipped, but the message says what to do: "`Rotation` is in `rotations.h`; add this header to the bridge as well".
   - A side benefit: a single `ferrobridge.toml` is enough for the whole bridge. M5's "one annotation file per header" finding is closed.
   - Alternative considered: sharing types between separate bridges (with cxx `ExternType` aliases). It burdens the user with managing dependencies between bridges; not chosen.
2. **Templates are instantiated with the concrete types the user lists (ADR-0009).**
   ```toml
   [templates]
   Vector3f = "Vector3<float>"
   Quaternion = "QuaternionT<float>"
   ```
   - libclang resolves the arguments; `using __fb_0 = ::Vector3<float>;` is added to the synthetic unit.
   - Members are read from the template's pattern (`ClassTemplate`) with `T` replaced by the concrete type. libclang does not show the members of an instantiated class (to be verified in Task 0).
   - On the C++ side the shim defines the alias `using Vector3f = ::Vector3<float>;`. The shim itself does **not** do an explicit instantiation; it would clash with the library's own `template class Vector3<float>;`.
   - Only templates with type parameters (`template <typename T>`); non-type parameters are out of scope.
   - Only types the library really instantiates should be listed. Otherwise the members' definitions cannot be found and there is a link error. The README says so.
3. **Value types (ADR-0010).** A class or instantiation becomes a `#[repr(C)]` struct in Rust if all of these hold:
   - all non-static fields are public
   - field types are numbers, `bool` or another value type
   - no virtual functions and no base class
   - no user-defined copy, move or destructor

   Example: `pub struct Vector3f { pub x: f32, pub y: f32, pub z: f32 }`.
   - It is introduced to cxx with `unsafe impl ExternType` (`Kind = Trivial`). The `unsafe` exists only in the generated inner layer and with a `// SAFETY:` justification (rule 5).
   - The C++ compiler has the final word. `shim.cc` checks with `static_assert`: `is_trivially_copyable`, `is_standard_layout`, `sizeof`, `alignof` and every field's `offsetof`. If the analysis is wrong, there is a compile error, not silent UB.
   - Classes that do not meet the conditions stay opaque as today (`UniquePtr`, `Pin`). Rule 3 is kept.
   - Value types derive `Clone, Copy, Debug` in Rust. `PartialEq` and `Default` are not derived: they may not carry the meaning of C++'s `==` and default constructor. They are bound from the C++ operator or constructor.
   - Methods take `&self` / `&mut self`, no `Pin`. A return by value is a plain value.
4. **Operators (`std::ops`).**
   - `+ - * /` (binary), unary `-`, `+= -= *= /=` and `==` / `!=` → `Add`, `Sub`, `Mul`, `Div`, `Neg`, `*Assign` and `PartialEq`.
   - Overloads of the same operator with different right-hand types become separate `impl`s: `Mul<f32>` and `Mul<Vector3f>`.
   - Those whose `std::ops` counterpart is unclear (`%` = cross product, `[]`, `^`) become named methods: `op_rem`, `op_index`... The reason is documented.
   - Called through the shim.
5. **Libraries without exceptions (ADR-0011; the PRD's open question).**
   ```toml
   [library]
   exceptions = false
   ```
   - This is a promise: "this library does not throw exceptions". Functions return values directly instead of `Result`.
   - If the promise is wrong, the program ends with `std::terminate`; no undefined behavior. Whether cxx wraps functions that do not return `Result` as `noexcept` is verified in Task 0; if not, this decision is dropped.
   - ferrobridge's own checks (slice length, null `borrowed` return, enum value) still return `Result`.
   - The promise goes into the generated files' list of "assumptions".
6. **The demo becomes a single bridge.** All `AP_Math` headers in a single `ap_math` module, with a single `ferrobridge.toml`.

## Scope
**In:** decisions 1–6. In addition:
- `examples/tour.rs` (quaternion from Euler angles → rotating a vector → matrix; CRC)
- the demo README ("in 5 minutes", comparison with hand-written cxx, a coverage table)
- re-measurement
- ADR-0009/0010/0011

**Out:**
- function templates (`wrap_180<T>`, `is_zero<T>`): v0.2. About 5% of the work according to the measurement.
- public fields of opaque classes (only value types have fields)
- inheritance
- non-type template parameters (`VectorN<T, N>`)
- CMake (v0.2)
- CI and the release (M7)

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Rejecting templates | `crates/ferrobridge-parser/src/lower.rs:128` | `ClassTemplate` → `unsupported`; this turns into reading the pattern |
| Lowering classes | `crates/ferrobridge-parser/src/lower.rs:221` (`class`), `:677` (`lower_record`) | Instantiations take the same path; a substitution environment is added to `lower_type` |
| Synthetic unit | `crates/ferrobridge-parser/src/lib.rs:134` | `synthetic_unit`; several headers + `using` lines |
| Annotation format | `crates/ferrobridge-analysis/src/config.rs:168` | `templates` and `library` sections in `RawFile`; an unknown key is an error with a line number |
| Known types | `crates/ferrobridge-analysis/src/lib.rs:744` | `known_types`; the value type decision is made here |
| Extern types | `crates/ferrobridge-codegen/src/bridge.rs:147` | `extern_type`; `type X = super::X;` + `ExternType` for value types |
| Safe layer | `crates/ferrobridge-codegen/src/safe.rs:280`, `:406` | `class_items`, `wrapper`; no wrapper for value types, the struct itself |
| Constant checks | `crates/ferrobridge-codegen/src/shim.rs` (`constant_checks`) | Layout `static_assert`s in the same form |
| `build.rs` | `crates/ferrobridge-build/src/lib.rs:279` | `Generate::new`; several headers |
| Tests | `crates/ferrobridge-parser/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/*.rs` | Fixture + snapshot + e2e with real C++; no cxx types in the safe API tests (`idiomatic.rs`) |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-parser/src/{clang,lib,lower}.rs` | UPDATE | Multiple headers, resolving instantiations, lowering from the pattern with substitution, fields and layout information, operators |
| `crates/ferrobridge-ir/src/{item,types,lib}.rs` | UPDATE | IR v5: `Class.fields`, `Class.layout`, `Class.instance_of`, `Method.operator`, the header in `SourceLocation` |
| `crates/ferrobridge-analysis/src/{config,rules,lib,plan,naming}.rs` | UPDATE | `[templates]`, `[library]`, the value type decision, operator plans, `exceptions = false` |
| `crates/ferrobridge-codegen/src/{bridge,shim,safe,spelling}.rs` | UPDATE | Value structs, `ExternType`, layout `static_assert`s, `std::ops` impls, alias `using`s |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | Several headers, `--include-as` per header |
| `crates/ferrobridge-build/src/lib.rs` | UPDATE | `Generate::header(..)` |
| `tests/fixtures/{multi_header,templates,value_types,operators,no_exceptions}/` | CREATE | A fixture + e2e for every decision |
| `crates/ferrobridge-e2e/**` | UPDATE | The new fixtures |
| `demo/ap_math/**` | UPDATE | A single bridge, `ferrobridge.toml`, `examples/tour.rs`, README, tests |
| `docs/adr/0009-*`, `0010-*`, `0011-*`, `docs/adr/README.md` | CREATE / UPDATE | Decisions 1–5 |
| PRD, `README.md`, `CLAUDE.md`, project notes, measurement report | UPDATE | |

## Tasks
Each task is done with TDD: a failing test first, then code, then verification.

### Task 0: Try the risky assumptions (outside the repo)
- **Action**:
  - (a) Does libclang really not show the members of an instantiated class? Can the `TemplateTypeParameter` cursor be reached from the pattern's `T` type with `clang_getTypeDeclaration`? Does `clang_getSpecializedCursorTemplate` lead from the instantiation to the template?
  - (b) cxx: can a `using Vector3f = ::Vector3<float>;` alias enter the bridge with `ExternType` (`Trivial`) and do the following?
    - parameters and returns by value
    - `self: &Vector3f` and `self: &mut Vector3f` methods
    - cxx's own `static_assert`s
  - (c) What does cxx do when a function that does not return `Result` throws an exception in C++? `std::terminate`, or undefined behavior?
  - (d) Calling an operator through a shim and wrapping it in Rust with `impl Add`.
- **Validate**: the results are written into the plan notes. If (c) is not `terminate`, decision 5 is dropped; if (b) does not work, value types are done with free functions in the shim.

### Task 1: Multi-header bridges
- **Action**:
  - Parser: `parse_headers(&[Path])`, a single synthetic unit, a set of target files.
  - The location in the IR carries the header name.
  - Codegen: `header_includes: Vec<String>`.
  - CLI: several headers; `--include-as` per header.
  - `Generate::header`.
  - A new, guiding message for a type defined in a header outside the bridge.
- **Mirror**: `lib.rs:134`, `lower.rs:68`
- **Validate**: the `multi_header` fixture: a class from `a.h` is used in a function in `b.h` (snapshot + e2e). Existing snapshots change only in the IR version.

### Task 2: Template instantiation (parser)
- **Action**:
  - `ParseOptions.instantiate: Vec<(rust_name, spelling)>`.
  - `using` lines are added to the synthetic unit; the resolved arguments and the template cursor are taken from libclang.
  - The pattern's members are lowered with a substitution environment: `T`, `Vector3<T>`, `const Vector3<T>&`, a reference to another instantiation (`Matrix3<T>` → `Matrix3f`).
  - A reference to a template that is not instantiated becomes `unsupported`; the message says which instantiation to add.
  - Fields and layout are read from the concrete type: `clang_Type_getSizeOf`, `getAlignOf`, `getOffsetOf`.
- **Mirror**: `lower.rs:221`, `:677`
- **Validate**: the `templates` fixture (a template with two parameters, references to itself and to another instantiation, a template that is not instantiated, a non-type parameter → `unsupported`); IR round trip.

### Task 3: Value types (analysis + code generation)
- **Action**:
  - The value type rule (decision 3); falling back to opaque classes.
  - `type X = super::X;` and `unsafe impl ExternType` in the bridge, with `// SAFETY:`.
  - `#[repr(C)] pub struct` in the safe layer.
  - Layout `static_assert`s in `shim.cc`.
  - Methods and constructors (`Vector3f::new_f32_f32_f32`).
- **Mirror**: `known_types`, `extern_type`, `constant_checks`
- **Validate**:
  - `value_types` e2e: reading and writing fields, return by value, an `&mut self` method, a nested value type (`Vector3f` inside `Matrix3f`).
  - A deliberately wrong layout gives a compile error through `static_assert` (a codegen unit test).
  - A class with a private field stays opaque.

### Task 4: Operators
- **Action**:
  - In the parser, operator methods are marked with `Method.operator`; free operator functions are read too.
  - Analysis: the `std::ops` mapping table (decision 4).
  - Codegen: shim + `impl` blocks.
- **Validate**: `operators` e2e: `a + b`, `a * 2.0`, `a * b` (dot product), `-a`, `a += b`, `a == b`, the cross product `op_rem`.

### Task 5: `[library] exceptions = false`
- **Action**:
  - `Annotations.library.exceptions`.
  - Analysis treats all functions as `noexcept`; ferrobridge's own checks stay `Result`.
  - The promise is added to the list of assumptions.
- **Validate**:
  - `no_exceptions` e2e: returns are plain values.
  - If the promise is wrong (C++ throws), the process ends. The test runs itself as a child process and checks for an abnormal exit.

### Task 6: A showable demo
- **Action**:
  - A single bridge, a single `ferrobridge.toml`. Its contents: `[templates]` (`Vector2f`, `Vector3f`, `Matrix3f`, `Quaternion`), `[library] exceptions = false` and M5's annotations.
  - The tests move to the new API: the `?` noise goes away.
  - `examples/tour.rs`: quaternion from Euler angles, rotating a vector, comparing with `Matrix3f`, CRC.
  - README: "run it in 5 minutes", a side-by-side comparison with the cxx bridge that would have to be written by hand (for the same `Vector3f`), a coverage table, known limits.
  - Re-measurement.
- **Validate**:
  - `ARDUPILOT_DIR=… cargo test` and `cargo run --example tour` (`--manifest-path demo/ap_math/Cargo.toml`).
  - No `unsafe` in the demo.
  - Coverage ≥ 80%.

### Task 7: Review, ADRs and documents
- **Action**:
  - `/ecc:security-review`. Focus: the `ExternType` justifications, layout verification, the chance of substitution producing a wrong type, `exceptions = false`.
  - `/ecc:cpp-review`.
  - ADR-0009/0010/0011.
  - PRD (the open question closes), README, `CLAUDE.md`, project notes.
- **Validate**: all checks pass on both libclangs (Homebrew 23, Apple 21).

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
ARDUPILOT_DIR=~/ardupilot cargo run --manifest-path demo/ap_math/Cargo.toml --example tour
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Lowering from the pattern with substitution produces a wrong type (dependent types, nested templates) | High | Type parameters only. Every unrecognized dependent type becomes `unsupported`; nothing is guessed. The layout of value types is verified with `static_assert`. Method signatures are checked by the C++ compiler in the shim: a wrong type is a compile error, not silent UB |
| libclang does show instantiated members, or the pattern does not give the expected information | Medium | Task 0 first. The method is chosen based on the result |
| cxx does not support methods or passing by value with `ExternType` aliases | Medium | Task 0. Fallback: value types through free functions in the shim |
| cxx does not wrap as `noexcept` for `exceptions = false` | Medium | Task 0 (c). If so, decision 5 is dropped and `Result` stays |
| Some members of an instantiation do not compile for that `T` | Medium | Only types the library instantiates are listed (README). The compile error is explicit; if needed, member skipping (`skip`) is added to `ferrobridge.toml` |
| Operator overloads clash in Rust (other than `Mul<f32>` vs `Mul<Vector3f>`) | Low | A clashing operator becomes a named method |
| Scope (Large) | High | Priority order 0→1→2→3→6 (the minimum for a showable demo). 4 and 5 improve the demo's quality; if time runs out, a separate decision is made before Task 6 |
| The 80% goal slips because of the measurement method's roughness (±10%) | Medium | The measurement script is updated to count template members from the plan; the method is written clearly in the report |

## Acceptance
- [x] A type from one header can be used in functions of other headers; there is a guiding message for a type outside the bridge
- [x] `Vector2f`, `Vector3f`, `Matrix3f`, `Quaternion` are bound with `[templates]`
- [x] Value types are Rust structs (fields, `&mut self`); their layout is verified in C++ with `static_assert`; other classes are opaque
- [x] Operators through `std::ops`
- [x] `exceptions = false` returns values directly; if the promise is broken the process ends (no UB); or it was dropped with a justification based on Task 0
- [x] The demo is a single bridge; `examples/tour.rs` works; the README contains "in 5 minutes" and the comparison; no `unsafe` in the demo
- [x] `AP_Math` coverage ≥ 80% (method in the report)
- [x] ADR-0009/0010/0011; security and C++ review; green on both libclangs
- [x] Patterns mirrored, not reinvented

## Implementation notes

### Task 0 (2026-11-27): all assumptions verified
- **(a) libclang:**
  - An instantiated class (including an explicit instantiation) has no cursor children. Members must be read from the pattern (`ClassTemplate`).
  - `clang_getSpecializedCursorTemplate` leads from the instantiation to the template.
  - Arguments are resolved (`float`). `sizeof`, `alignof` and `clang_Type_getOffsetOf` can be read from the concrete type.
  - **Difference from the plan:** the declaration of the pattern's `T` cannot be found (`NoDeclFound`); its canonical name is `type-parameter-<depth>-<index>`. Substitution is done with this name.
  - Dependent types such as `const V3<T>&` reach the template (`ClassTemplate`) with `clang_getTypeDeclaration` and its arguments with `clang_Type_getNumTemplateArguments`.
- **(b) cxx:** the alias `using Vector3f = ::V3<float>;` enters the bridge with `ExternType` (`Kind = Trivial`). Parameters and returns by value, `self: &Vector3f`, `self: &mut Vector3f` work. cxx itself also generates `static_assert(IsRelocatable<…>)`.
- **(c) cxx makes the C++ wrapper of functions that do not return `Result` `noexcept`.** When an exception is thrown, the process ends through `std::terminate` with `SIGABRT`; no undefined behavior. Decision 5 holds.
- **(d)** `operator+` can be called through a shim and wrapped in Rust with `impl Add`.

### Task 1 (2026-11-27): multi-header bridges
- **Parser:** `parse_headers(&[P])` includes all headers in a single synthetic unit, in the given order. `parse_header` is now its single-header form. The target files are a set.
- **IR v5: `Type::Record` / `Type::Enum` got an optional `declared_in` field.** The field is set only if the type is declared outside the bridge's headers, and appears in JSON only in that case. The only difference in existing snapshots is the IR version. There is one exception: in the `prelude` fixture, `Vec2` got `declared_in: "prelude.h"` because it comes from the prelude; this is correct.
  - To do this, `lower_type` now takes a `Types` context. Task 2's substitution environment is added here too.
- **Analysis:** if an unknown type is in a header outside the bridge, the message names that header and suggests adding it to the bridge.
- **Codegen:** `CodegenOptions.header_includes: Vec<String>`. Single-header output stayed exactly the same (snapshots and e2e files did not change). With several headers, the header comment says "Source headers: …".
- **CLI:** `generate` and `suggest` take several headers. `--include-as` is given once per header, in the same order; a count mismatch is an error.
- **`ferrobridge-build`: the API changed.** Old: `Generate::new(header, name).include_as(..)`. New: `Generate::new(name).header(path)` or `.header_as(path, include)`. The demo and the integration test were updated.
- **e2e:**
  - A new rule: all `.h` files in a fixture directory (not subdirectories) are bridge headers. This rule is applied in the CLI sync test and in the README command.
  - A new fixture `multi_header`: `geometry.h` + `input.h`, and an `outside/units.h` left outside the bridge.
- **Total:** 176 tests, clippy clean in both modes, 17 demo tests.

### Task 2 (2026-11-27): template instantiation (parser)
- **Flow:** `ferrobridge.toml` → `[templates]` (`Annotations.templates`, validated with line numbers) → the CLI and `build` load the annotations before parsing and pass them to `ParseOptions.instantiate`.
- **Synthetic unit:** for every instantiation, `using __ferrobridge_inst_N = <spelling>;` and a `static_assert(sizeof(...))` forcing the class to be instantiated are added. libclang errors on these lines are shown with the prefix "`[templates]` X = "…": …". `; { } " #` and line breaks are rejected in the spelling.
- **Substitution:**
  - `type-parameter-0-N` in the pattern is replaced with the concrete argument.
  - A dependent `Tmpl<T>` is tied to a matching instantiation if there is one.
  - A `Vec3` written by name only: libclang gives it as the pattern class (`ClassDecl`); it is recognized by having the same name and location as the template.
  - Other dependent types are `Unsupported`. Parameters of member templates (depth ≠ 0) are `Unsupported` too.
- **Two problems found:**
  - (1) `const Vec3<float>` and `Vec3<float>` are not equal as types → matching is done through the specialization declaration.
  - (2) Because of a forward declaration (`template <typename T> class Vec2;`), the template came without a body → the definition is always used (`clang_getCursorDefinition`); template declarations without a body are skipped.
- **Added to IR v5:**
  - `Class.fields` (regardless of access, with offset and location), `Class.layout`, `Class.user_copy_or_move`, `Class.instance_of`.
  - Bit fields have an `Unsupported` type.
  - Fields are no longer "unsupported members" in the parser. Analysis reports the public fields of opaque classes with the same reason; skipped members are listed in source order (the `classes` output did not change).
- **Instantiated templates** are no longer reported as "unsupported". The reason for those not instantiated says how to use `[templates]`.
- **A trial on ArduPilot:** `vector2.h`, `vector3.h`, `matrix3.h`, `quaternion.h` + `[templates]` (`Vector2f/Vector3f/Matrix3f/Quaternion`) → 4 classes, 120 methods. Remaining skips:
  - operators (Task 4)
  - fields and class parameters by value (Task 3)
  - `Rotation` (opens up when `rotations.h` is added to the bridge)
  - the `*<double>` instantiations
  - the `T[4]` array
- **Total:** 181 tests.

### Task 3 (2026-11-27): value types
- **Analysis:**
  - `BridgeType::Value`; `ClassPlan.value` (fields, byte offsets, size, alignment) and `ClassPlan.instance_spelling`.
  - The value type decision (`mark_value_types`): structural conditions (not polymorphic, no base, no user-defined copy/move/destructor, known layout, all fields public and byte-aligned), followed by a fixed-point elimination over field types (`Pt` inside `Line`).
  - Value types can be passed as parameters by value, returned by value and passed as `&T`/`&mut T`. Their constructors return values.
  - Value types inside `std::vector` / `std::unique_ptr` are rejected for now.
  - The C++ path of instantiations is `::<spelling>`.
- **Code generation:**
  - `type X = super::X;` in the bridge.
  - Instantiations in the shim namespace, with the alias `using X = ::geo::V3<float>;`.
  - In the safe layer, `#[repr(C)] #[derive(Debug, Clone, Copy)] pub struct` and `unsafe impl ::cxx::ExternType` (`Kind = Trivial`). `size_of`/`align_of`/`offset_of!` const checks on the Rust side.
  - `is_trivially_copyable`, `is_standard_layout`, `sizeof`/`alignof` and field `offsetof` `static_assert`s in `shim.cc`. Methods of value types take `&mut self`, no `Pin`.
- **Two `prettyplease` limitations were worked around as text after formatting:**
  - It cannot print `type X = super::X;` in an `extern` block → a marked `type X;` is generated and turned into the alias.
  - It drops comments → the `// SAFETY:` comment of the `unsafe impl` is added afterwards. The workspace lint (`undocumented_unsafe_blocks`) accepts this.
- **Behavior change:** `Point { float x, y; }` in the `classes` fixture is now a value type. It used to be a `UniquePtr` wrapper; now its fields are accessible. The existing e2e tests pass unchanged.
- **e2e `value_types` (5 tests):** fields, `&mut self`, passing by value, a nested struct, the `Vec3f` instantiation (cross product, `normalize`), a class with a private field staying opaque.
- **Total:** 190 tests.
- **Not tested:** "a deliberately wrong layout gives a compile error" was not tried in e2e. The `static_assert`s being generated is verified in the snapshot; cxx also adds its own `IsRelocatable` check.

### Task 4 (2026-11-27): operators
- **Parser:** member operators are normal methods with `Method.operator` (`"+"`, `"+="`…).
  - Not bound, `unsupported` with a reason: assignment, `new`/`delete`, `->`, `++`/`--`, `()`, `,`, unary `&`/`*`.
  - **An old bug fixed:** free `operator` functions in a header could turn into an invalid Rust identifier named `operator+` and break generation; they are now rejected with a reason.
- **Analysis:**
  - `OperatorKind` (token + parameter count), named methods (`op_add`, `op_neg`, `op_mul_assign`, `op_rem`...).
  - Overloads get suffixes with the existing rule (`op_mul_f32`, `op_mul_V2`).
  - Operator methods always go through the shim; the shim calls `fb_self.operator+(v)`.
- **Code generation:** `std::ops` impls for value-type operators that do not return errors: `Add/Sub/Mul/Div/Rem/BitXor/BitAnd/BitOr<Rhs>`, `Neg`/`Not`, `*Assign<Rhs>`, `PartialEq` (only with `Self`, returning `bool`).
  - The right-hand side can be a number, a value type or a `const&` value type.
  - The same trait and right-hand side are not generated twice.
  - Comparisons (`<`…) and `[]` stay named methods: the meaning of `PartialOrd` may not match C++.
  - Opaque classes have named methods only.
- **Behavior change:** `Vehicle::operator==` in the `classes` fixture is now the `op_eq` method.
- **e2e `operators` (4 tests):** `a + b`, `b - a`, `-a`, `a * 2.0`, `a * b` (dot product), `a % b`, `a += b`, `==`/`!=`, `op_lt`; `op_div`, which is not `noexcept`, returns `Result` (`Err` on division by zero); the opaque `Counter::op_add_assign`.
- **Total:** 195 tests.

### Task 5 (2026-11-27): `[library] exceptions = false`
- **Config:** the `[library]` section; an unknown key is an error with a line number. If `exceptions = false`, `Annotations.no_exceptions` keeps the line of the promise.
- **Analysis:** if the promise is made, all functions are treated as `noexcept`. ferrobridge's own checks, such as the slice length check and the null `borrowed` return, stay `Result`. The promise goes into the "assumptions" list: `(library) exceptions …`.
- **Code generation:** no change needed. cxx wraps functions that do not return `Result` as `noexcept` (Task 0); the shim is `noexcept` too.
- **An extra fix:** the note "no public constructor; can only be used with objects returned by other functions" is no longer printed for value types; value types can be constructed from their fields.
- **e2e `no_exceptions` (3 tests):**
  - Plain value returns.
  - An operator without `noexcept` now becomes a trait.
  - A slice function still returns `Result`.
  - A call breaking the promise ends the process in a child process (the C++ exception message appears on stderr).
- **Total:** 201 tests. The answer to the PRD's open `-fno-exceptions` question became this promise (to be written in ADR-0011).

### Task 6 (2026-11-27): a showable demo
- **A single bridge:** the demo is now a single `ap_math` bridge for all `AP_Math/*.h` (21 headers). `lib.rs` re-exports the bridge at the crate root: `ferrobridge_demo_ap_math::{Vector3f, crc_crc32}`.
- **A single `ferrobridge.toml`:** M5's four annotation files were merged. In addition:
  - `[library] exceptions = false`. Its basis was verified: waf uses `-fno-exceptions`, and `AP_Math` has no `throw`.
  - `[templates]` (`Vector2f/3f`, `Matrix3f`, `Quaternion` and their `double` versions). All of them are explicitly instantiated in the `.cpp`s.
  - The `to_euler` output parameters (by reading the source).
- **The first build went through without problems:** 12 classes (8 of them value types), 513 functions/methods, 41 `std::ops` impls, 48 layout `static_assert`s.
- **A security finding → a deviation from the plan.** The `const` form of `operator[]` was being bound as `op_index` without needing an annotation. In ArduPilot `[]` does no bounds checks (`MATH_CHECK_INDEXES` is off); `v.op_index(5)` from safe Rust would be undefined behavior. `operator[]` is now never bound; its reason suggests the fields for value types. It was added to the `operators` fixture too. The item "`[]` becomes a named method" in the plan's decision 4 was dropped for this reason.
- **Tests:** moved to the new API without `?`; 14 tests. Covered:
  - `Vector3f` operators, dot and cross products
  - quaternion and matrix rotation giving the same result
  - `rotate` with `Rotation`
  - `to_euler`
  - CRC catalog values
  - slice overflow `Err`
  - `SCurve`, constants
- **`examples/tour.rs`:** vectors, orientation, sensor mounting, controller, CRC, constants. `cargo run --example tour` works.
- **README:** "in 5 minutes", the comparison with hand-written cxx (59 annotation lines → 8,000+ lines of generated code), coverage, known limits. The limits include this: ferrobridge cannot know C++ preconditions.
- **Small fixes:**
  - `ferrobridge-build`'s summary warning listed the 21 paths on one line for several headers; now it says "`ap_math` (21 headers)".
  - The slice overflow message printed the item name twice (`crc_crc8: crc_crc8: …`); `ferrobridge::Error` adds it already.
- **Measurement:** **81.8%** with the demo's configuration (337/412; template members counted once), 82% with fields; 78% with `float` only. The goal (≥ 80%) was met. Details: `m5-apmath-measurement.md` → "After M6".
- **Known limit:** overload names are long (`from_euler_f32_f32_f32`; ADR-0005). Written in the README.

### Task 7 (2026-11-27): review, ADRs, documents
- **Review.** Focus: the `ExternType` justifications, layout verification, substitution, operators, `exceptions = false`. Two findings fixed:
  1. **(Security)** A class with a `const` field (`struct P { const float x; }`) counted as a value type because it is trivially copyable. Writing `p.x = …` from Rust would change a `const` object in C++ → classes with `const` fields are opaque (analysis test + e2e `Frozen`).
  2. **(C++)** `offsetof(::geo::Pair<int, float>, first)`: the macro split its arguments at the comma in the spelling and gave a compile error → for instantiations the `static_assert`s use the alias in the shim (e2e `PairIF`).
- **Found without problems:**
  - A wrong substitution is a compile error in the typed C++ signatures of the shim and cxx.
  - `Copy` matches `is_trivially_copyable`; this covers a trivial destructor too.
  - `PartialEq` (not `Eq`) is compatible with float NaNs.
  - `operator[]` had already been removed in Task 6.
- **ADRs:** ADR-0009 (multi-header + template instantiation), ADR-0010 (value types, operators, `operator[]`), ADR-0011 (`exceptions = false`).
- **Documents:**
  - Root README: multiple headers, new rows in the type table, `[templates]`/`[library]` examples, the "cannot know preconditions" warning.
  - `CLAUDE.md`, PRD (M6 complete, the `-fno-exceptions` open question closed), project notes.
- **Final verification:** fmt, clippy (both modes), `cargo doc -D warnings`; 203 tests identical on Homebrew 23 and Apple 21 libclang; 14 demo tests and `cargo run --example tour`.
