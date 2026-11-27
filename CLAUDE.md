# ferrobridge

A binding generator for using C++ libraries from Rust **safely and idiomatically**.
It reads headers with libclang → intermediate model (IR) → ownership inference + `ferrobridge.toml` annotations → generates a `cxx` bridge, a C++ shim and a safe Rust API.

Open source: `MIT OR Apache-2.0` (see `docs/adr/0003-open-source-dual-license.md`).

Status: **Milestone 6 (cross-header types, templates, value types) is complete**: one bridge is generated from several headers; `[templates]` instantiates class templates (members come from the pattern, with the parameters substituted); trivially copyable classes with public fields become `#[repr(C)]` Rust structs (`ExternType` + C++ `static_assert`s); member operators become `std::ops`; `[library] exceptions = false` returns values directly. `demo/ap_math/` binds ~82% of `AP_Math`, including `Vector3f`/`Matrix3f`/`Quaternion`, and `cargo run --example tour` works. Next: Milestone 7, the v0.1 release (`/ecc:plan .claude/prds/ferrobridge.prd.md`).

## Documents
- Requirements (PRD): `.claude/prds/ferrobridge.prd.md`. The planning command `/ecc:plan .claude/prds/ferrobridge.prd.md` reads this file.
- Architecture decisions: `docs/adr/` (index: `docs/adr/README.md`, template: `docs/adr/template.md`). Accepted ADRs: 0001 (libclang via `clang-sys`), 0002 (build on cxx), 0003 (open-source dual license), 0004 (generated code is committed), 0005 (overload/collision naming), 0006 (ownership annotations in a separate TOML file), 0007 (safe API layer over cxx), 0008 (build-time generation and the GPL demo), 0009 (multi-header bridges and template instantiation), 0010 (value types), 0011 (libraries without exceptions).
- Milestone plans: `.claude/plans/` (each plan ends with implementation notes and deviations).
- Personal notes and local paths stay out of the repo; the project owner keeps them in `CLAUDE.local.md`, which is not tracked by Git.

## Working rules
- Every milestone: plan with `/ecc:plan` → approval → TDD → review → commit.
- Write an ADR (`docs/adr/`) for every decision that goes beyond the MVP scope.
- Do **not copy** GPL-licensed code (e.g. ArduPilot) into the repo; use it from outside (ADR-0003).
- Everything in the repo (documents, comments, messages) is written in English.

## Safety rules (non-negotiable)
1. **Ask, don't guess:** No code is generated for a pointer whose ownership is unclear. An explanatory warning is printed together with a suggested `ferrobridge.toml` entry.
2. **Most restrictive default:** When in doubt, use `Pin<&mut T>` and `&T` instead of `&mut`.
3. **Non-trivially movable types** are never held by value in Rust: use `UniquePtr` or `Pin`.
4. **Exceptions do not cross the FFI boundary:** every shim is wrapped in `try/catch`.
5. **No `unsafe` leaks to the user:** `unsafe` exists only in the generated inner layer, with a justification comment.

## Commands
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"   # on macOS, Apple's libclang is loaded otherwise
cargo run -p ferrobridge-cli -- doctor              # shows which libclang is loaded
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                              # e2e needs a C++17 compiler
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml   # AP_Math demo
```

When the generator or the fixtures change, `crates/ferrobridge-e2e/src/generated/` must be regenerated (command: `crates/ferrobridge-e2e/README.md`; fixtures with a `ferrobridge.toml` use `--config`); otherwise the `generated_files_match_the_committed_e2e_files` test fails. Every top-level item in the generated `bridge.rs` carries `#[rustfmt::skip]`, so `cargo fmt` leaves it alone. All `.h` files in a fixture directory (not subdirectories) are that fixture's bridge headers. In e2e, safe API tests use no cxx types (checked by `tests/idiomatic.rs`); raw layer tests live in `tests/raw_layer.rs`. **Review snapshot changes by hand** with `cargo insta review` before accepting them.

`demo/ap_math/` is not part of the root workspace and is **GPL-3.0-or-later** (ADR-0008): ArduPilot code and files generated from it never enter the repo (bridges are generated into `OUT_DIR`), and demo code is not moved into the main crates. All `AP_Math` headers form a single bridge, with all annotations in one `demo/ap_math/ferrobridge.toml` (keys for template instantiations use the Rust name: `Quaternion::to_euler`). Read the C++ source before writing an annotation: the promise must be based on the source.

## Architecture (short)
`parser` (libclang → IR; the header is read through a translation unit generated in memory) → `analysis` (`ferrobridge.toml` + what can be bound safely + Rust names → `BridgePlan`) → `codegen` (`bridge.rs` = safe layer + raw `ffi`, `shim.h`, `shim.cc`) → `cli` / `build` (`build.rs`). Generated code uses the `ferrobridge` crate at runtime (`Error`, conversions). All `unsafe` lives in `crates/ferrobridge-parser/src/clang.rs`.

## Code rules (enforced by workspace lints)
- `unwrap()` / `expect()` are forbidden outside tests (`clippy.toml` allows them in tests).
- Every `unsafe` block needs a `// SAFETY:` justification above it. New libclang calls go only into `clang.rs`.
- Integration test files need `#![allow(clippy::unwrap_used)]` (the `clippy.toml` exception does not cover helper functions).
- Public items are expected to have doc comments (`missing_docs`).
- `ferrobridge-ir` may not depend on any other ferrobridge crate. Internal dependencies are added only once they are used; the target graph is written as a comment in the root `Cargo.toml`.
