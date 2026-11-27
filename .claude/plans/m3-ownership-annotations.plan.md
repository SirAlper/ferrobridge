# Plan: Milestone 3 — Ownership annotations

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 3 — Ownership annotations ("The user writes annotations for ambiguous pointers; in an ambiguous case the tool asks instead of generating code")
**Complexity**: Large
**Status**: Complete (2026-11-27, approved together with 4 decisions)

## Summary
A C++ header does not say who owns a pointer or how long it lives; ferrobridge does not guess this (safety rule 1). In this milestone the user provides that information in a **`ferrobridge.toml`** file, and ferrobridge generates safe code accordingly: a pointer that transfers ownership becomes `UniquePtr<T>`, a borrowed pointer becomes `&T` / `Pin<&mut T>`, and a primitive output parameter (`double& out`) becomes `&mut f64`. No code is generated for any unannotated pointer; instead the new **`ferrobridge suggest`** command prints a `ferrobridge.toml` draft to fill in ("it asks"). Support for **`std::vector<T>`**, which slipped from M2, also arrives here.

## The trust model (the most important idea)
Every annotation in `ferrobridge.toml` is a **promise** the user makes about the C++ library ("this function does not keep the pointer", "it allocated the returned object with `new`; deleting it is up to me"). A wrong promise can mean undefined behavior; it is similar to an `unsafe` block in Rust. Therefore:
- Promises are made only in this file; the generated code assumes nothing on its own.
- Every generated file starts with a list of **"Assumptions based on annotations"**; the points of trust are visible at a glance in code review.
- Every cheap check is still done: if a borrowed return pointer is null, the shim does not turn it into a reference but returns `Err` (an error instead of UB).

## Scope
**In:**
- The `ferrobridge.toml` format and its validation: unknown function/parameter names, invalid values, unnecessary annotations → a clear error/warning (protection against typos).
- The **parameter** annotation `"borrowed"`: the pointer/reference is used only during the call, not kept, and not null.
  - `const T*` (T: class, primitive, `std::string`) → `&T` (the shim takes the address and passes it)
  - `T*` class → `Pin<&mut T>`; `T*` primitive → `&mut T`
  - primitive `T&` (e.g. `double& out`) → `&mut f64` (no shim needed)
- **Return** annotations:
  - `"owned"`: a `T*` return, the object was allocated with `new` and ownership passes to the caller → `UniquePtr<T>` (null gives an empty `UniquePtr`, no UB)
  - `"borrowed"` (methods only): a `T*` return points inside the object and is valid while the object lives → `Result<&T>` / `Result<Pin<&mut T>>`; null → `Err`
- `std::vector<T>` (T: primitive, `std::string`, classes in this header): `const std::vector<T>&` → `&CxxVector<T>`, `std::vector<T>&` → `Pin<&mut CxxVector<T>>`, return by value → `UniquePtr<CxxVector<T>>` through a shim. No annotation needed (value semantics).
- `ferrobridge generate --config ferrobridge.toml`
- `ferrobridge suggest <header> [--config ...]`: prints a commented-out `ferrobridge.toml` draft explaining the options and their meaning for every pointer/reference not yet annotated.

**Out (with reasons):**
- Nullable parameters (`Option<&T>`): cxx has no `Option`; could be solved with two shims in M4's wrapper layer.
- Slices from pointer + length pairs (`const T* data, size_t n` → `&[T]`): `size_t` also needs a shim conversion; separate work, v0.2.
- `const char*` C strings, `void*`, function pointers, arrays → still rejected with an explanatory warning.
- References returned by free functions (`'static` lifetime): cxx support not verified; a later milestone.
- Annotation through a C++ attribute in the header (`[[clang::annotate]]`) → v0.2 (decision 2).
- Annotating overloads separately: in M3 the key is the full C++ name and applies to **all overloads with that name**; a parameter name that does not exist in an overload is an error.

## Points that need a decision (accepted with your approval)
1. **The trust model** (above): annotation = the user's promise; a list of assumptions in the generated files; cheap checks (null) are always done.
2. **ADR-0006: annotations in a separate TOML file.** Because third-party headers cannot be edited, a separate file is required; C++ attribute support goes to v0.2. This closes the PRD's open question about where annotations live.
3. **An annotation stays required for primitive `T&` too.** Consistent with the decision in the architecture's type table: `double& out` is very common, but the C++ function may keep the reference; no `&mut` is generated without the `"borrowed"` promise. `suggest` lists these cases with a single command.
4. **Destructor check (added 2026-11-27).** When a `UniquePtr<T>` is dropped, `delete` runs. For this to be safe, two conditions are needed: (a) the destructor must be public and not deleted; (b) if the class is polymorphic (has a virtual method), the destructor must be virtual. The reason for (b): in the "interface + factory" pattern, `Shape* make_shape()` actually returns a subclass defined in a `.cc`. If the destructor is not virtual, `delete` is undefined behavior. Today's IR does not record the destructor at all (`lower.rs:291` skips it). So M2 has a hidden bug too: a class with a private destructor gets a constructor returning `UniquePtr`, and the generated code does not compile. Decision: IR v3 records a `Class.destructor` field. The `"owned"` return, the constructor factory and return by value (`BoxedClass`) check these two conditions; if they do not hold, the item is rejected with an explanatory warning. This applies the trust model's principle that "cheap checks are always done".

## Example annotation file
```toml
# ferrobridge.toml

# The returned pointer was allocated with `new`; deleting it is up to the caller.
[functions."geo::make_widget"]
returns = "owned"

# The returned pointer points inside the object; valid while the object lives.
[functions."geo::Widget::buffer"]
returns = "borrowed"

# The parameters are used only during the call, not kept, and not null.
[functions."geo::Widget::attach".params]
target = "borrowed"

[functions."geo::Vehicle::read_odometer".params]
out = "borrowed"
```

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Rejection reasons | `crates/ferrobridge-analysis/src/rules.rs:11` | `REASON_*` constants; the "Milestone 3" phrases will now point to the annotation file |
| Type rules | `crates/ferrobridge-analysis/src/rules.rs:50`, `:145` | The `param_type` / `ref_type` pattern is extended; a reason text when rejecting |
| Binding | `crates/ferrobridge-analysis/src/lib.rs:268` | `bind()`: skip entirely if even one parameter is unsupported |
| Plan types | `crates/ferrobridge-analysis/src/plan.rs:168`, `:231` | The `Returns` enum and `needs_shim()` grow new cases |
| Shim generation | `crates/ferrobridge-codegen/src/shim.rs:86` | `body()`: the call expression + return wrapping |
| Rust types | `crates/ferrobridge-codegen/src/spelling.rs:16` | `rust_type` / `cpp_type` match one to one |
| std types | `crates/ferrobridge-parser/src/lower.rs:401` | `lower_record`: recognition by namespace + template argument (the `unique_ptr` example at `:416`) |
| CLI | `crates/ferrobridge-cli/src/main.rs:36` | The `Generate` variant; the new `Suggest` variant follows the same pattern |
| Tests | `crates/ferrobridge-analysis/src/tests.rs:124`, `crates/ferrobridge-cli/tests/generate.rs:36` | Table tests; the e2e sync test covers the new fixtures too |
| Logging | — | None; warnings go to stderr with `⚠` / `ℹ` |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-ir/src/types.rs`, `item.rs`, `lib.rs` | UPDATE | `Type::Vector { element }`; `Class.destructor` (`Public { is_virtual }` / `NotPublic` / `Deleted`, implicitly public if not declared); `IR_FORMAT_VERSION` → 3 |
| `crates/ferrobridge-parser/src/lower.rs` | UPDATE | Recognize `std::vector<T, std::allocator<T>>`; destructor access, `= delete` and virtuality (read **before** the access filter) |
| `crates/ferrobridge-analysis/Cargo.toml` | UPDATE | `serde`, `toml` |
| `crates/ferrobridge-analysis/src/config.rs` | CREATE | The `ferrobridge.toml` model, reading, validation, error type |
| `crates/ferrobridge-analysis/src/rules.rs` | UPDATE | Rules for annotated pointers/references, `std::vector` |
| `crates/ferrobridge-analysis/src/plan.rs` | UPDATE | `ParamPlan.pass` (`Direct` / `AddressOf`), `Returns::OwnedPointer`, `Returns::BorrowedPointer`, `BridgeType::Vector`, `BridgePlan.assumptions`, `BridgePlan.suggestions` |
| `crates/ferrobridge-analysis/src/lib.rs` | UPDATE | `check(&Module, &Annotations)`; unused annotations → error/warning |
| `crates/ferrobridge-analysis/src/suggest.rs` | CREATE | A TOML draft for places not yet annotated |
| `crates/ferrobridge-codegen/src/{spelling,shim,lib}.rs` | UPDATE | `&mut T`, `CxxVector<T>`, passing addresses, owned/borrowed pointer returns (null → exception → `Err`), the assumptions block |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `generate --config`, the `suggest` command |
| `tests/fixtures/ownership/{input.h,impl.cc,ferrobridge.toml}` | CREATE | Owned/borrowed pointers, output parameters, a **live object counter** |
| `tests/fixtures/vectors/{input.h,impl.cc}` | CREATE | `std::vector<int>`, `<double>`, `<std::string>`, `<Class>` |
| `tests/fixtures/bad_config/` | CREATE | Unknown name, unknown parameter, invalid value, unnecessary annotation |
| `crates/ferrobridge-e2e/{build.rs,src/lib.rs,tests/*.rs,README.md}` | UPDATE / CREATE | The two new fixtures |
| `crates/*/tests/snapshots/` | UPDATE | IR v3 and the new fixtures |
| `docs/adr/0006-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Decision 2 |
| `README.md`, PRD, `CLAUDE.md`, project notes | UPDATE | Usage, M3 status, closing the open question |

## Tasks
Each task with TDD: a failing test first, then code, then verification. First a short **cxx experiment** (as in M2, outside the repository): an `&mut f64` parameter, `CxxVector<i32>` / `CxxVector<CxxString>` / `CxxVector<OpaqueClass>`, a `UniquePtr<CxxVector<T>>` return.

### Task 1: IR v3: `std::vector` and destructor information
- **Action**: `Type::Vector { element: Box<QualType> }`; the parser recognizes `std::vector` with the default allocator and rejects one with a custom allocator. `Class.destructor`: distinguishes private/protected/`= delete`/virtual destructors. Virtuality inherited from a base class does not matter in M3, because inheritance is still rejected.
- **Mirror**: `lower.rs:416` (recognizing `unique_ptr`), `clang.rs:354` (`is_virtual_method`)
- **Validate**: `cargo test -p ferrobridge-ir -p ferrobridge-parser` + review of the new `vectors` and `destructors` snapshots

### Task 2: The annotation file (`config.rs`)
- **Action**: `Annotations::from_toml_str` / `from_file`; `functions.<name>.returns` ∈ {`owned`, `borrowed`}, `functions.<name>.params.<parameter>` ∈ {`borrowed`}. Unknown key, invalid value → an error with a line number.
- **Mirror**: `parser/src/lib.rs:66` (thiserror pattern)
- **Validate**: `cargo test -p ferrobridge-analysis config::`

### Task 3: Annotated rules
- **Action**: `check(&Module, &Annotations)`. Pointers and primitive `T&` now look at the annotation; without one they are rejected and a **suggestion** is recorded. `"borrowed"` pointer parameters get `pass = AddressOf` (needs a shim). An `"owned"` return → `OwnedPointer`; a `"borrowed"` return from a method → `BorrowedPointer` (always `fallible`). Every annotation used is written to `assumptions`. An annotation that matches no item → error; an annotation on a place that needs none → warning. `std::vector` rules. Destructor check (decision 4): `OwnedPointer`, `BoxedClass` and constructor factories need a public destructor; polymorphic classes need a virtual destructor.
- **Mirror**: `rules.rs:50`, `lib.rs:268`
- **Validate**: `cargo test -p ferrobridge-analysis` (a table test for every annotation × type combination; unannotated pointers are still rejected)

### Task 4: Code generation
- **Action**: Rust/C++ spellings of `&mut T` (primitive) and `CxxVector<T>`; passing addresses with `&param` in the shim; `OwnedPointer` → `return ::std::unique_ptr<T>(CALL);`; `BorrowedPointer` → a null check, `throw ::std::runtime_error(...)` on null, otherwise `return *p;`; a `std::vector` returned by value → `make_unique`. An "Assumptions based on annotations" list at the top of the generated files.
- **Mirror**: `shim.rs:86`, `spelling.rs:16`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot review

### Task 5: CLI: `--config` and `suggest`
- **Action**: `generate --config <path>`; annotation errors give exit code 1. `suggest <header> [--config]`: prints the remaining unannotated places, with their options and meanings, as TOML to stdout; if there are none it says "nothing needs an annotation".
- **Mirror**: `main.rs:36`
- **Validate**: `cargo test -p ferrobridge-cli` (`suggest`'s output is valid TOML, and when given to `generate --config` the related warnings disappear)

### Task 6: End-to-end tests
- **Action**: the `ownership` fixture keeps a live object counter (the constructor increments, the destructor decrements). Tests: an object returned as `"owned"` is really deleted when the `UniquePtr` is dropped (the counter returns to 0); a `"borrowed"` return does not delete the object; a null `"borrowed"` return is `Err`; an `&mut f64` output parameter; a borrowed pointer parameter. `vectors`: creating a vector in Rust and sending it, getting one back, changing one in place.
- **Mirror**: `crates/ferrobridge-e2e/tests/classes.rs`
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 7: Review, ADR and documents
- **Action**: `/ecc:security-review` + `/ecc:cpp-review` (focus: correctness of ownership transfer, null checks, `&mut` aliasing); ADR-0006; a `ferrobridge.toml` and `suggest` section in the README; the PRD (M3 status, closing the open question), plan notes, CLAUDE.md, project notes.
- **Validate**: the README examples work when copied and run

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p ferrobridge-cli -- suggest tests/fixtures/classes/input.h
cargo run -p ferrobridge-cli -- generate tests/fixtures/ownership/input.h --out-dir target/gen \
  --config tests/fixtures/ownership/ferrobridge.toml
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| The user writes a wrong annotation (an object declared `owned` was not allocated with `new`) → UB | Medium | The trust model: promises in one place, the list of assumptions in every file, a clear warning in the README; cheap checks (null) always |
| An unexpected cxx restriction for `CxxVector<OpaqueClass>` or `&mut f64` | Medium | A cxx experiment first; unsupported combinations are rejected in analysis with an explanation |
| Annotation keys cannot tell overloads apart | Medium | In M3 they apply to all overloads, and an unknown parameter is an error; signature keys (`"geo::f(int*)"`) in v0.2 |
| The null check of `borrowed` returns uses exceptions; it does not compile in `-fno-exceptions` projects | Medium (high for ArduPilot) | The same assumption as M2; the open PRD question is to be decided before M5 |
| `std::vector<bool>` is special in C++ (bit-packed), cxx does not support it | High | Rejected explicitly and tested |
| An interface pointer returned as `"owned"` actually points to a subclass and the destructor is not virtual → `delete` is UB | Medium | Decision 4: rejected if a polymorphic class has no virtual destructor; a factory test with a virtual destructor in e2e |
| Scope size (Large) | Medium | Nullable parameters, slices, C strings and `'static` returns deliberately left out |

## Acceptance
- [x] Still no code generated for unannotated pointers; `suggest` prints a TOML draft to fill in for them
- [x] In the `ownership` fixture, an object returned as `"owned"` is deleted in C++ when the `UniquePtr` is dropped in Rust (proven with the live object counter); a `"borrowed"` return is not deleted; a null borrowed return is `Err`
- [x] Output parameters such as `double& out` work as `&mut f64` with an annotation
- [x] No `UniquePtr` is generated for classes with a non-public or deleted destructor; `"owned"` is rejected with an explanation if a polymorphic class has no virtual destructor
- [x] `std::vector<T>` works end to end; `std::vector<bool>` is rejected with an explanation
- [x] Misspelled/unknown annotations give errors with line numbers
- [x] Generated files contain the "Assumptions based on annotations" list
- [x] No `unsafe` at all in the user-side tests; all validation commands pass
- [x] ADR-0006 written; security and C++ review done
- [x] Patterns mirrored, not reinvented

## Implementation notes
### cxx experiment (2026-11-27, cxx 1.0.205, outside the repo)
All of it worked: an `&mut f64` parameter (C++ `double&`); `&CxxVector<i32>`, `Pin<&mut CxxVector<f64>>`; returning `UniquePtr<CxxVector<CxxString>>` and `UniquePtr<CxxVector<OpaqueClass>>` (`get(i)`, `len()`, `iter()`); `CxxVector::<i32>::new()` + `push` in Rust; `Result<&T>` and `Result<Pin<&mut T>>` through a shim (null → `throw` → `Err`); the destructor runs when a `UniquePtr<T>(new T)` is dropped (the live object counter returns to 0).

**Finding:** because the borrowed-return shim is a free function, Rust's `self` lifetime rule does not apply. With another reference parameter there is a `missing lifetime specifier` error. The shim signature has to be written with an explicit lifetime: `fn Widget_pick<'a>(fb_self: &'a Widget, other: &Widget) -> Result<&'a Widget>;`. In Task 4 every shim with a borrowed return uses `'a`.

### Result (2026-11-27)
All tasks complete. 116 tests in total (IR 6, parser 11, analysis 48, codegen 10, CLI 13 + parse, e2e 24). Homebrew libclang 23.1.6 and Apple libclang 21.0.0 give the same result. `fmt`, `clippy -D warnings` clean; no `unsafe` in the user-side tests.

### Deviations and findings
- **The destructor check (decision 4)** is applied separately in three places. A virtual destructor is required only for `"owned"` pointer returns, because only there can the object's real type be a subclass. For constructor factories, return by value and `std::vector` elements the type is known exactly, so only a public, non-deleted destructor is required. A base class whose definition cannot be seen is treated as "polymorphic, without a virtual destructor" (the most restrictive assumption). Deleted destructors of fields are not taken into account; they lead to a compile error, not UB.
- **`void*` no longer says "needs an annotation".** Because an annotation does not make it bindable, the reason is stated directly. The same holds for `char*` (C strings) and `T**`. `suggest` prints only places that an annotation can resolve. If a function has another problem an annotation cannot fix (e.g. `long`), there is no suggestion for that function.
- **The values in the `suggest` draft are comments.** The plan's criterion that "warnings disappear when `suggest`'s output is given to `generate --config`" is verified in a test by enabling the lines. Printing the values filled in would mean guessing ownership (ADR-0006, alternative 3).
- **Overloads:** the plan's rule "a parameter name that does not exist in an overload is an error" was implemented as "a parameter name that exists in **no** overload is an error". Otherwise overloads with different parameter names could not be annotated at all. The security review added an extra measure: if an annotation applies to more than one overload, a warning listing their signatures is printed (one of them may keep the pointer).
- **The invalid annotation fixture (`tests/fixtures/bad_config/`) was not created.** These cases are tested with table tests in analysis and with temporary files written inside tests in the CLI.
- **The `'a` lifetime:** all method shims with a borrowed return use an explicit `'a`, even without another reference parameter. This keeps the rule single and simple.
- **`cargo fmt` clashed with the generator.** rustfmt formatted the committed `bridge.rs` differently from prettyplease and broke the sync test; not seen in M2 because the lines were short. The generated `ffi` module now carries `#[rustfmt::skip]`. This prevents the same problem in user projects.
- **cargo-insta:** `cargo-insta` is not installed. Snapshots were written as `.snap.new` with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new`, reviewed by hand, and then accepted with `INSTA_UPDATE=always`.
- **Still open:** nullable parameters (`Option`), pointer+length → slices, `'static` reference returns from free functions, signature (overload-specific) annotation keys, in-header attributes (v0.2). The `-fno-exceptions` question has to be decided before M5: the null check of borrowed returns uses exceptions too.
