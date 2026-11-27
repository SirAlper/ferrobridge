# Plan: Milestone 2 — First bridge

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 2 — First bridge ("A simple C++ class can be called from a Rust test without a hand-written bridge")
**Complexity**: Large
**Status**: Complete (2026-11-27). Deviations at the end of the plan.

## Summary
The `ferrobridge generate <header> --out-dir <dir>` command generates a **cxx bridge module** (`bridge.rs`) and the needed **C++ shim** files (`shim.h`, `shim.cc`) from a header. Classes (as opaque types), methods, constructors, enums, free functions and `std::string` are supported. The generated code is **compiled together with a real C++ library and called from Rust tests** in a separate test crate; not a single line of bridge is written by hand. Ownership annotations (M3) and the idiomatic safe wrapper (M4) are not in this milestone; the output stays at the cxx level.

## Scope
**In:**
- Classes/structs: opaque types (`type Vehicle;`). **Public** members only.
- Methods: `const` → `self: &T`, non-const → `self: Pin<&mut T>`. Virtual methods can be called (not overridden).
- Constructors: a shim factory function → `UniquePtr<T>`. Not generated for abstract classes and `= delete` constructors.
- Static methods: a shim free function.
- Enums (scoped and unscoped): cxx's "extern enum" pattern; values are checked against C++ at compile time.
- Free functions (from M1) and namespaces (`#[namespace = "..."]`).
- Types: primitives · `const T&` (T: primitive, class, `std::string`) → `&T` · `T&` for classes → `Pin<&mut T>` · `std::string` returns → `UniquePtr<CxxString>` · classes returned by value → wrapped in `unique_ptr` in the shim · `std::unique_ptr<T>` returns → `UniquePtr<T>`.
- Overloads: the Rust name is derived from the parameter types (`clamp_i32_i32_i32`, `clamp_f64_f64_f64`); the C++ name is kept with `#[cxx_name]`.
- Exceptions: every function that is not `noexcept` returns `Result<T>` (safety rule 4). An idiomatic error type comes in M4.
- Every unsupported item/member: a comment in the generated file + an explanatory warning in `generate`'s output.

**Out (with reasons):**
- Raw pointers and primitive `T&` (e.g. `int& out`) → **M3** (needs an ownership annotation; decision from the architecture's type table)
- Taking a class by value as a parameter → needs copy/move semantics, **M3/M4**
- `std::vector` → **removed from M2**, proposed to move to M3 (element type restrictions are separate work)
- Inheritance (base class methods), operators, conversion functions, field access, templates → v0.2
- The `build.rs` helper crate (`ferrobridge-build`) → **M5**

## Points that need a decision (accepted with your approval)
1. **Minimum libclang 16.** The function needed to detect `= delete` constructors arrived in libclang 16. The `clang-sys` feature is raised from `clang_10_0` to `clang_16_0`. The PRD's open question about the "minimum LLVM version" is thereby closed as **16** (CI will test 21–23).
2. **ADR-0004: generated code is written to files and committed.** Code is generated with the `generate` command, not hidden inside `build.rs`; users see the generated bridge, review it and keep it under version control.
3. **ADR-0005: the overload naming rule.** A name defined once stays as it is; if there is more than one with the same name, *all* of them get a suffix derived from the parameter types. Existing names changing when a new overload is added to the header is recorded as a deliberate cost.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Naming | `crates/ferrobridge-cli/src/main.rs:18` | A new subcommand is added as a variant of the `Command` enum; the doc comment becomes the help text |
| Errors | `crates/ferrobridge-parser/src/lib.rs:66` | One `thiserror` enum per crate |
| CLI error output | `crates/ferrobridge-cli/src/main.rs:78` | `report()`: `✗` + detail lines, `ExitCode::FAILURE` |
| Unsafe | `crates/ferrobridge-parser/src/clang.rs` | New libclang calls only in `clang.rs`, with a `// SAFETY:` justification |
| IR | `crates/ferrobridge-ir/src/item.rs:10` | A `#[serde(tag = "kind", rename_all = "snake_case")]` enum, fields with doc comments |
| Unsupported | `crates/ferrobridge-parser/src/lower.rs:17` | `REASON_*` constants; nothing is skipped silently |
| Tests | `crates/ferrobridge-parser/tests/fixtures.rs:20` | Fixture header → `insta` snapshot; `#![allow(clippy::unwrap_used)]` in integration test files |
| End to end | `crates/ferrobridge-cli/tests/parse.rs:17` | The real binary through `CARGO_BIN_EXE_ferrobridge` |
| Logging | — | No logging; warnings go to stderr with the `report()` pattern |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-ir/src/item.rs` | UPDATE | `Item::Class`, `Item::Enum`; `Method`, `Constructor`, `EnumVariant` |
| `crates/ferrobridge-ir/src/types.rs` | UPDATE | `Type::Record { name, namespace }`, `Type::Enum`, `Type::String`, `Type::UniquePtr`; `IR_FORMAT_VERSION` → 2 |
| `crates/ferrobridge-parser/Cargo.toml` | UPDATE | `clang-sys` feature `clang_16_0` |
| `crates/ferrobridge-parser/src/clang.rs` | UPDATE | Access specifiers, const/static/virtual/pure/deleted, abstract, enum values, template arguments |
| `crates/ferrobridge-parser/src/lower.rs` | UPDATE | Class members, constructors, enums, recognizing `std::string`/`std::unique_ptr` |
| `crates/ferrobridge-parser/tests/snapshots/*.snap` | UPDATE | M1 snapshots change with IR v2 (especially `unsupported`) |
| `tests/fixtures/classes/input.h` + `impl.cc` | CREATE | Class, method, constructor, static, virtual, abstract, deleted ctor, return by value |
| `tests/fixtures/enums/input.h` | CREATE | Scoped/unscoped, explicit values, different underlying types |
| `tests/fixtures/strings/input.h` + `impl.cc` | CREATE | `const std::string&` parameter, `std::string` return |
| `crates/ferrobridge-analysis/src/lib.rs` | UPDATE | `check(&Module) -> BridgePlan`: decides what can be bound, collects warnings (the safety rules live here) |
| `crates/ferrobridge-analysis/src/naming.rs` | CREATE | Overload suffixes, clashes with Rust keywords (`type_` instead of `r#type`) |
| `crates/ferrobridge-codegen/src/lib.rs` | UPDATE | `generate(&BridgePlan, &CodegenOptions) -> GeneratedFiles` |
| `crates/ferrobridge-codegen/src/bridge.rs` | CREATE | The `#[cxx::bridge]` module (`quote` + `prettyplease`) |
| `crates/ferrobridge-codegen/src/shim.rs` | CREATE | `shim.h` / `shim.cc`: constructor, static method and return-by-value wrappers |
| `crates/ferrobridge-codegen/tests/fixtures.rs` | CREATE | Snapshots of the 3 generated files for each fixture |
| `crates/ferrobridge-e2e/` | CREATE | A `publish = false` test crate: `build.rs` (`cxx-build`) compiles the generated files and the fixtures' `impl.cc`; `tests/` calls them from Rust |
| `crates/ferrobridge-e2e/src/generated/` | CREATE | Output of `ferrobridge generate` (committed) |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | The `generate` subcommand |
| `crates/ferrobridge-cli/tests/generate.rs` | CREATE | End to end: files are written, warnings are printed, identical to the committed files in e2e |
| `Cargo.toml` | UPDATE | `ferrobridge-analysis`, `ferrobridge-codegen` workspace dependencies; versions of `cxx`, `cxx-build`, `quote`, `proc-macro2`, `prettyplease`, `syn` |
| `docs/adr/0004-*.md`, `docs/adr/0005-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Decisions 2 and 3 |
| `README.md`, PRD, `CLAUDE.md` | UPDATE | `generate` usage, M2 status, minimum libclang 16 |

## Tasks
Each task with TDD: a failing test first, then code, then verification.

### Task 1: IR v2
- **Action**: `Class { name, namespace, is_abstract, constructors, methods, location }`, `Method { name, params, return_type, is_const, is_static, is_virtual, noexcept, location }`, `Constructor { params, noexcept, location }`, `Enum { name, namespace, scoped, underlying: Type, variants: [{ name, value: i64 }] }`. New types: `Record`, `Enum`, `String`, `UniquePtr`. Unsupported class members are kept inside the class as `unsupported_members`.
- **Mirror**: `crates/ferrobridge-ir/src/item.rs:10`
- **Validate**: `cargo test -p ferrobridge-ir` (a JSON round trip for every new variant)

### Task 2: Parser: classes, enums, std types
- **Action**: the needed calls in `clang.rs`; `lower.rs` walks class members (public only), collects constructor/method/static/virtual/abstract/deleted information and reads enum values. `std::string` (= `std::basic_string<char>`) and `std::unique_ptr<T>` are recognized by the declaration's namespace and template arguments (not just the name).
- **Mirror**: `lower.rs:17` (`REASON_*`), `clang.rs` (`// SAFETY:`)
- **Validate**: `cargo test -p ferrobridge-parser` + `cargo insta review` (the M1 snapshot differences and the 3 new fixtures are reviewed by hand)

### Task 3: Analysis: bindability and naming
- **Action**: `check()` walks the IR and decides, for every item/parameter/return, whether it can be expressed safely in cxx. The rules are the architecture's type table and the safety rules (pointer → reject, primitive `T&` → reject, class `T&` → `Pin<&mut T>`, class returned by value → `UniquePtr` through a shim). If even a single parameter of a function is unsupported, the **whole** function is skipped and the reason reported (no half bindings). Overload and keyword naming.
- **Mirror**: `lower.rs:17` (reason texts), `parser/src/lib.rs:66` (error pattern)
- **Validate**: `cargo test -p ferrobridge-analysis` (table tests for every rule; overload and keyword names)

### Task 4: Code generation
- **Action**: `bridge.rs`: `extern "C++"` blocks per namespace, `include!("shim.h")`, opaque types, methods (`self: &T` / `Pin<&mut T>`), `#[cxx_name]`, `Result<T>`, extern enums. `shim.h/.cc`: `ferrobridge_new_<Class>_<n>`, static method and return-by-value wrappers; they `#include` the original header. Every file starts with `@generated by ferrobridge — do not edit by hand` and lists unsupported items as comments.
- **Mirror**: `parser/tests/fixtures.rs:20` (snapshot pattern)
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot review; a test checks that the generated `bridge.rs` can be parsed with `syn`

### Task 5: The end-to-end test crate (`ferrobridge-e2e`)
- **Action**: the generated files for the `classes`, `enums` and `strings` fixtures are written to `src/generated/<fixture>/`. `build.rs` compiles them and the fixtures' `impl.cc` with `cxx-build`. Tests from Rust: create an object, call a method, read with a `const` method, call a static method, compare an enum value, get `Err` from a method that throws, send and receive a `std::string`.
- **Mirror**: `crates/ferrobridge-cli/tests/parse.rs` (real build/run)
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 6: The `ferrobridge generate` command
- **Action**: `generate <header> --out-dir <dir> [--include-as <path>] [--std] [-- clang arguments]`. Writes the three files, prints `⚠ <name>: <reason>` for every skipped item, and a summary at the end (`5 items bound, 2 skipped`). Exit code 1 on error.
- **Mirror**: `crates/ferrobridge-cli/src/main.rs:18` and `main.rs:78`
- **Validate**: `cargo test -p ferrobridge-cli` (files are written; the output is identical to the committed files in e2e, i.e. the generator and the committed code are in sync)

### Task 7: Security review, ADRs and documents
- **Action**: the new `unsafe` blocks and the generated C++ shims are reviewed (`/ecc:security-review` + `/ecc:cpp-review`). ADR-0004 and ADR-0005 are written. A `generate` example in the README; the PRD (M2 status, minimum libclang 16), the plan file, CLAUDE.md and the project notes are updated.
- **Validate**: the command in the README produces output when copied and run

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p ferrobridge-cli -- generate tests/fixtures/classes/input.h --out-dir target/gen
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| A pattern cxx does not support is generated (e.g. `cxx_name` on overloads, extern enums, static methods) | Medium | Every pattern is first verified with a small example in the e2e crate; moved into the shim if unsupported |
| The generated C++ shim does not compile or contains UB (e.g. `make_unique` with an abstract class) | Medium | Abstract and deleted constructors are rejected in analysis; e2e does a real build; `/ecc:cpp-review` |
| Recognizing `std::string` by name is fragile (libc++ `std::__1::basic_string`, libstdc++ differs) | High | Recognized by the declaration's namespace chain + template argument (`char`); tested with macOS libc++, Linux libstdc++ recorded as a CI risk |
| Scope size (Large) | High | `std::vector` deliberately removed; no inheritance/operators/fields; can be split into milestone 2a (class+method) / 2b (enum+string) if needed |
| Overload names drift when the header changes | Medium | Recorded as a deliberate cost in ADR-0005; in M4's idiomatic layer the user can give names (annotation file) |
| `Result<T>` shims do not compile in projects built with `-fno-exceptions` | Medium (high for ArduPilot) | M2 assumes exceptions are on; stays an open PRD question, to be decided before the `AP_Math` demo in M5 |
| `cc`/the C++ compiler slows down the e2e test | Low | One crate, three fixtures; caching in CI |

## Acceptance
- [ ] The class in the `classes` fixture is created and its methods called from a Rust test without a hand-written bridge
- [ ] Enums and `std::string` work end to end; an exception thrown from a method that is not `noexcept` becomes `Err` in Rust
- [ ] Every unsupported item/member shows up as a comment in the generated file and as a warning in the CLI
- [ ] No `unsafe` at all in the user-side tests
- [ ] `generate`'s output is identical to the committed files in e2e
- [ ] All validation commands pass
- [ ] ADR-0004 and ADR-0005 written; the new `unsafe` blocks and C++ shims reviewed
- [ ] Patterns mirrored, not reinvented

## Implementation notes (2026-11-27)
- **A cxx experiment came first** (outside the repository, in a temporary crate): `cxx_name` for overloads, extern enums, `Result`, constructor/static/string shims, a separate shim namespace, renamed types. All of it worked. Findings from the experiment:
  - cxx copies parameter names into the generated C++; C++ keywords such as `this` cannot be used. The shim receiver became `fb_self`.
  - cxx's default exception handler catches only `std::exception`; `throw 42` would end the program. `rust::behavior::trycatch` was added to the shim, so now every exception becomes `Err` (safety rule 4). It is guarded with `#ifndef` in case several shims end up in the same file.
- **Task order changed:** Task 6 (the `generate` command) was done before Task 5 (the e2e crate), so that e2e's committed files are produced by the real command.
- **Homebrew libclang could not find its resource directory:** in headers including `<string>`, `<memory>`, `stdarg.h` could not be found (not seen in M1 because `<cstdint>` did not need it). The parser now passes the `clang/<version>/` directory next to the loaded libclang as `-resource-dir`. Apple libclang had no problem.
- **Platform-dependent integers are not bound:** cxx maps types exactly; `long` (32 bits on Windows), `long long` and `size_t` correspond to different `intN_t`s on different platforms. These are rejected in analysis with an explanation ("use `std::int64_t`"). `int`, `short`, `unsigned`, `char` (→ `c_char`) and `std::intN_t` are bound. Converting through the shim was left to a later milestone.
- **A safety measure beyond the plan:** `generate` does not overwrite a file without the `@generated by ferrobridge` marker unless `--force` is given (added during review).
- **Test bug (not in the code):** the C++ `length` function computes with `float`, so the expectation for √5 had to be at `f32` precision; the test was fixed.
- **cxx version:** `cargo add` picked 1.0.198 (believed to be the newest version compatible with the workspace's `rust-version = 1.85`; 1.0.205 worked in the experiment).
- **Review:** 52 `unsafe` blocks (17 new), all in `clang.rs` and justified. No source of UB was found in the generated C++. Known behaviors: if a class that cannot be copied/moved is returned by value, the shim does not compile (the error is at compile time); if memory allocation fails in a `noexcept` shim, the program ends (like running out of memory in Rust). No dependency scan was done because `cargo audit` is not installed.
- **Result:** 65 tests (IR 4, parser 14, analysis 18, codegen 7, CLI 9, e2e 13), clippy `-D warnings` and fmt clean.
