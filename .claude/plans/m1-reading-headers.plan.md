# Plan: Milestone 1 — Reading headers

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 1 — Reading headers ("The user gives a C++ header and the tool shows what it sees inside (functions, types) in a readable form")
**Complexity**: Medium
**Status**: Complete (2026-11-26). Deviations at the end of the plan.

## Summary
The `ferrobridge parse <header>` command reads a C++ header with libclang, turns its **free functions** (with their namespaces) and their **parameter/return types** into IR, and prints the IR as readable (indented) JSON. Classes, templates and enums are not read in this milestone, but they are **not skipped silently**: they are written to the IR as "unsupported" items with their location and reason. No code generation.

## Scope
**In:** free functions · namespaces (including nested) · primitive types (`void`, `bool`, `char`, signed/unsigned integers, `float`, `double`) · `const`, pointers (`T*`), lvalue/rvalue references (`T&`, `T&&`) · `noexcept` information · overloads with the same name (as separate items) · explanatory items for unsupported constructs · libclang error diagnostics
**Out (later milestones):** class/struct members, enums, templates, `std::string`/`std::vector` mapping, ownership analysis, code generation, reading `compile_commands.json`

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Naming | `crates/ferrobridge-cli/src/main.rs:15` | Subcommands are variants of a clap derive `enum Command`; help text is a doc comment |
| Errors | `crates/ferrobridge-parser/src/lib.rs:14` | One `thiserror` enum per crate; an `#[error(...)]` message; conversion with `map_err` (`lib.rs:28`) |
| Unsafe | `crates/ferrobridge-parser/src/lib.rs:40` | A `// SAFETY:` justification above every `unsafe` block; libclang resources are released once, in the same function or in `Drop` |
| Loading libclang | `crates/ferrobridge-parser/src/lib.rs:24` | Every public entry point calls `ensure_loaded()` first |
| CLI error output | `crates/ferrobridge-cli/src/main.rs:35` | `eprintln!("✗ {error}")` + a hint line, `ExitCode::FAILURE` |
| IR | `crates/ferrobridge-ir/src/lib.rs:16` | Public types with `serde` derives and doc comments; `IR_FORMAT_VERSION` (`lib.rs:12`) |
| Tests | `crates/ferrobridge-ir/src/lib.rs:36` | In-module `#[cfg(test)] mod tests`; `unwrap()` allowed in tests. No example yet for fixture/snapshot tests → this plan sets up the first (`insta`) |
| Logging | — | The project has no logging; none is added in this milestone |

## Files to Change
| File | Action | Why |
|---|---|---|
| `Cargo.toml` | UPDATE | `ferrobridge-ir` workspace dependency, `insta` and `serde_json` versions |
| `crates/ferrobridge-ir/src/lib.rs` | UPDATE | Module root; expose the submodules |
| `crates/ferrobridge-ir/src/item.rs` | CREATE | `Module.items`, `Item` (`Function`, `Unsupported`), `Function`, `Param`, `SourceLocation` |
| `crates/ferrobridge-ir/src/types.rs` | CREATE | `Type` (primitives, `Pointer`, `LValueRef`, `RValueRef`, `Unsupported`) and `const` information |
| `crates/ferrobridge-parser/Cargo.toml` | UPDATE | `ferrobridge-ir` dependency; `insta`, `serde_json` dev-dependencies |
| `crates/ferrobridge-parser/src/lib.rs` | UPDATE | The `parse_header(path, &ParseOptions) -> Result<Module, ParserError>` entry point; new error variants |
| `crates/ferrobridge-parser/src/clang.rs` | CREATE | A thin safe wrapper: `Index`, `TranslationUnit` (RAII), `Cursor`, `Type`, walking children, diagnostics (modeled on bindgen's `clang.rs`) |
| `crates/ferrobridge-parser/src/lower.rs` | CREATE | Cursor → IR lowering; only declarations in the main file; unsupported ones become `Item::Unsupported` |
| `crates/ferrobridge-parser/tests/fixtures.rs` | CREATE | For each fixture, `parse_header` → JSON snapshot (`insta`) |
| `tests/fixtures/free_functions/input.h` | CREATE | Simple functions, unnamed parameters, `noexcept`, overloads |
| `tests/fixtures/primitive_types/input.h` | CREATE | All primitives, `<cstdint>` fixed-width types, `const`, pointers, references |
| `tests/fixtures/namespaces/input.h` | CREATE | Nested namespaces, functions with the same name in different namespaces |
| `tests/fixtures/unsupported/input.h` | CREATE | Class, template function, enum → `Unsupported` items |
| `tests/fixtures/syntax_error/input.h` | CREATE | A broken header → an error with diagnostics |
| `crates/ferrobridge-cli/Cargo.toml` | UPDATE | `serde_json` dependency |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | The `parse <header> [--std <c++17>] [-- <clang arguments>]` subcommand |
| `crates/ferrobridge-cli/tests/parse.rs` | CREATE | End-to-end tests running the built `ferrobridge` binary (`CARGO_BIN_EXE_ferrobridge`) |
| `README.md` | UPDATE | A `parse` usage example |

## Tasks
Each task with TDD: a failing test first, then code, then verification.

### Task 1: IR types
- **Action**: `Module { format_version, items }`, `Item` (serde `tag = "kind"`), `Function { name, namespace: Vec<String>, params, return_type, noexcept, location }`, `Param { name: Option<String>, ty }`, `Type` (stores the C++ spelling + `size_bytes`; platform-dependent types such as `int`/`long` are marked separately), `Unsupported { kind, name, reason, location }`, `SourceLocation { file, line, column }` (file path relative to the header). `IR_FORMAT_VERSION` → 1.
- **Mirror**: `crates/ferrobridge-ir/src/lib.rs:16` (serde derive + doc comment)
- **Validate**: `cargo test -p ferrobridge-ir` (a JSON round-trip test for every variant)

### Task 2: A safe libclang wrapper
- **Action**: In `clang.rs`, release `Index` and `TranslationUnit` with `Drop`; `TranslationUnit::parse(path, args)`; `Cursor::{kind, spelling, location, is_in_main_file, children, result_type, arguments, exception_spec}`; `Type::{kind, spelling, size_of, pointee, is_const}`; a closure bridge for `clang_visitChildren`; collect error/fatal diagnostics as `Vec<Diagnostic>`. On macOS, if `-isysroot` is not given, it is added automatically with `xcrun --show-sdk-path` (so `<cstdint>` can be found).
- **Mirror**: `crates/ferrobridge-parser/src/lib.rs:40` (`// SAFETY:` + releasing once), `lib.rs:24` (`ensure_loaded`)
- **Validate**: `cargo test -p ferrobridge-parser clang::` (a valid header opens; a missing file → error; a broken header → list of diagnostics)

### Task 3: Cursor → IR lowering
- **Action**: `lower.rs` turns top-level and in-namespace `FunctionDecl`s of the main file into `Item::Function`. Classes, structs, enums, templates and unrecognized types become `Item::Unsupported` (with a reason); nothing is skipped silently. Declarations from other `#include`d files are not taken.
- **Mirror**: the Task 2 wrapper; errors through `ParserError` (`lib.rs:14`)
- **Validate**: `cargo test -p ferrobridge-parser --test fixtures` + `cargo insta review` (the 5 fixture snapshots are reviewed by hand and accepted)

### Task 4: The `parse_header` entry point and errors
- **Action**: `ParseOptions { std: String (default "c++17"), extra_args: Vec<String> }`. New errors: `FileNotFound(PathBuf)`, `ParseFailed { diagnostics }` (each diagnostic in the form `file:line:column: message`). Warning-level diagnostics do not count as errors.
- **Mirror**: `crates/ferrobridge-parser/src/lib.rs:14`
- **Validate**: the `syntax_error` fixture returns `ParseFailed` and the message contains a line number

### Task 5: The `ferrobridge parse` command
- **Action**: `Command::Parse { header, std, clang_args }`. On success prints indented JSON to stdout, exit code 0. On error prints each diagnostic with `✗` to stderr, exit code 1. If libclang is missing it shows the same hint as `doctor`.
- **Mirror**: `crates/ferrobridge-cli/src/main.rs:15` and `main.rs:35`
- **Validate**: `cargo test -p ferrobridge-cli` (a successful parse → valid JSON and the expected function name; a broken header → exit code 1); `cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h`

### Task 6: Documents
- **Action**: a `parse` example in the README; Milestone 1 status in the PRD and the project notes are updated.
- **Validate**: the command in the README produces output when copied and run

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo insta test --review            # review snapshot changes
cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Homebrew libclang cannot find the macOS SDK headers (`<cstdint>`) | High | `-isysroot $(xcrun --show-sdk-path)` is added automatically; users can pass their own flags with `--`; tested with a fixture |
| Snapshots change with the libclang version or platform (Apple 21 vs Homebrew 23, size of `long`) | Medium | Fixtures use fixed-width types and `int`; no absolute paths in snapshots; tests use the same libclang as `doctor` |
| In `runtime` mode `clang-sys` loads the library per thread; in parallel tests a thread without it crashes | Medium | Every public entry point calls `ensure_loaded()`; the wrapper types are not `Send` (cannot leave the thread) |
| libclang's C API does not provide some information (e.g. `noexcept` details) | Low | The escape hatch in ADR-0001; the information M1 needs (name, type, size, exception spec, location) is in the C API, verified in Task 2 |
| UB in the `clang_visitChildren` closure bridge | Medium | bindgen's proven pattern; a panic does not cross the FFI boundary (`catch_unwind`); reviewed with `/ecc:security-review` |

## Acceptance
- [ ] `ferrobridge parse <header>` gives the expected output for 5 fixtures (snapshots reviewed by hand)
- [ ] Every unsupported construct appears in the output as an `Unsupported` item with its reason and location
- [ ] A broken header gives an error message with a line number and exit code 1
- [ ] All validation commands pass
- [ ] All new `unsafe` blocks have a `// SAFETY:` justification and passed `/ecc:security-review`
- [ ] Patterns mirrored, not reinvented

## Implementation notes (2026-11-26)
- **Fixed-width type detection was done differently from the plan.** libclang gives `std::int32_t` (`using ::int32_t;` inside std) as an "unexposed" type with no declaration; the typedef chain cannot be followed there. Detection is done by name (`int32_t`, `std::uint8_t`...) and compared with the real size.
- **Integration test files** needed `#![allow(clippy::unwrap_used)]`: the `clippy.toml` test exception covers only `#[test]` functions, not helper functions.
- **Security review:** all 35 `unsafe` blocks are in `clang.rs` and justified. The only problem found was fixed: if libclang returned an error code and still gave a translation unit, it would leak. No dependency scan was done because `cargo audit` / `cargo deny` are not installed.
- **Platform check:** the snapshots give the same output with both Homebrew libclang 23.1.6 and Apple libclang 21.0.0. The signedness of `char` is platform-dependent (signed on macOS arm64); it may differ on Linux CI.
