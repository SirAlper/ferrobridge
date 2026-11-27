# Plan: Milestone 5 — Build pipeline and the `AP_Math` demo skeleton

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 5 — Build pipeline and the `AP_Math` demo (without templates). Option 2 from the measurement report: the old M5 was split in two, templates moved to M6.
**Input**: `.claude/plans/m5-apmath-measurement.md` (coverage today ~20%; ~59% of the non-template API)
**Complexity**: Large
**Status**: Complete (2026-11-27)

## Summary
For the first time we run ferrobridge on a real library, with a real `build.rs` pipeline. The `ferrobridge-build` crate becomes usable from `build.rs`. The tool limits found by the measurement are closed:
- headers that include themselves
- `static inline` functions
- constants
- pointer + length → slice

The `demo/ap_math/` crate is built against a real ArduPilot checkout. The non-template `AP_Math` API (`crc_*`, `control.h`, `location.h`, `SCurve`, `SplineCurve`, angle conversions, constants, the `Rotation` enum) is called from Rust without `unsafe`.

**Measurable goal:** **≥ 85%** of the non-template `AP_Math` API (59% today), **≥ 30%** overall (20% today). The PRD's ≥ 80% goal moves to M6 (templates and fields).

## Decisions (accepted with your approval)
1. **Splitting the milestones.**
   - M5 = build pipeline + demo skeleton (this plan).
   - M6 = template instantiation, public fields and `AP_Math` ≥ 80%. General template support was outside the MVP in the PRD; M6's plan brings it into the MVP with an ADR.
   - M7 = the v0.1 release.
   - The PRD table and the project notes are updated accordingly.
2. **The demo's license and location (ADR-0008).**
   - The demo lives in this repo under `demo/ap_math/`, as **a separate Cargo workspace**. `cargo test --workspace` does not need ArduPilot; it runs as a separate job in CI.
   - ArduPilot code does not enter the repo; the demo points to a checkout with the `ARDUPILOT_DIR` environment variable.
   - **Generated files** derived from GPL headers **are not committed**; they are generated into `OUT_DIR` at build time. So the repo carries no GPL-derived text. This is a deliberate exception to ADR-0004, justified in ADR-0008.
   - Because the demo's own Rust code links against a GPL library, the `demo/ap_math/` directory is licensed **`GPL-3.0-or-later`** (its own `LICENSE` file, `publish = false`). The main crates stay `MIT OR Apache-2.0`.
3. **Two modes of `ferrobridge-build`.**
   - **Default:** compiles committed generated files. No libclang needed; the user flow from ADR-0004.
   - **The `generate` feature:** reads the header inside `build.rs`, generates into `OUT_DIR` and compiles. Needs libclang; the demo uses it.
   - Builder API: `Bridge::new(header).include_dir(..).define(..).prelude(..).config(..).sources(..).compile(name)`.
4. **The slice annotation** (postponed to v0.2 in M3, brought forward because of the measurement):
   ```toml
   [functions."crc_crc8".params]
   p = { slice = "len" }   # p points to an array of len elements; used during the call
   ```
   - `const T* p, N len` → a single `&[T]` parameter in Rust; `T* p` → `&mut [T]`. `len` disappears from the Rust signature.
   - T can only be a numeric type. `N` can be any integer type, including `size_t`, because it is never visible to Rust.
   - The shim checks whether the length fits in `N`; if not, an exception → `Err`. No silent truncation.
   - The promise goes into the list of assumptions ("an array of `len` elements; not kept").
5. **`static inline` functions** are bound only **if their definitions are visible in the header**. Because the C++ files of cxx and the shim include the header, the function is defined in that translation unit. Internal-linkage declarations without a definition are still rejected (otherwise there would be a link error).
6. **Constants:**
   - The values of `const`/`constexpr` variables of primitive types are computed with libclang (`clang_Cursor_Evaluate`). They are generated in the safe layer as `pub const WGS84_A: f64 = 6378137.0;`.
   - Non-finite values (`NaNf`, `inf`) are rejected with an explanation: `f32::NAN` does not keep the NaN's payload; it would be a guess.
   - IR v4: `Item::Constant`.

## Scope
**In:** decisions 1–6; `--prelude` (CLI and builder); reading headers through a synthetic translation unit; the demo crate + C++ stubs + `ferrobridge.toml`; re-measurement.

**Out:** template instantiation, public fields, operators (M6); CMake (v0.2); CI for the demo (v0.1, together with the CI setup).

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| libclang access | `crates/ferrobridge-parser/src/clang.rs:110` | `parse`; new calls (unsaved files, evaluate) only here and with `// SAFETY:` |
| Main file filter | `crates/ferrobridge-parser/src/lower.rs:68` | `is_in_main_file` → "is it in the target header" |
| Internal linkage rule | `crates/ferrobridge-parser/src/lower.rs:128` | `REASON_INTERNAL`; relaxed for `static inline` with a definition |
| Variables | `crates/ferrobridge-parser/src/lower.rs:104` | `VarDecl` → `unsupported` today; `Item::Constant` if constant |
| Annotation format | `crates/ferrobridge-analysis/src/config.rs` | `ParamAnnotation`: a string or a `{ slice = "..." }` table (`serde(untagged)`) |
| Pointer rules | `crates/ferrobridge-analysis/src/rules.rs` (`param_type`) | A new `Pass::Slice { len_index }`, like `Pass::AddressOf` |
| Shim body | `crates/ferrobridge-codegen/src/shim.rs` (`body`) | Length check + `p.data()`, `static_cast<N>(p.size())` |
| Safe layer | `crates/ferrobridge-codegen/src/safe.rs` | `pub const`; slices are passed through directly |
| `build.rs` | `crates/ferrobridge-e2e/build.rs` | Hand-written `cxx_build` code → replaced by `ferrobridge_build` (dogfooding) |
| Tests | `crates/ferrobridge-parser/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/*.rs` | Snapshots + e2e with real C++ |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-parser/src/{clang,lib,lower}.rs` | UPDATE | Synthetic translation unit, `prelude`, target file filter, `static inline`, evaluating constants |
| `crates/ferrobridge-ir/src/{item,lib}.rs` | UPDATE | `Item::Constant`, IR v4 |
| `crates/ferrobridge-analysis/src/{config,rules,lib,plan}.rs` | UPDATE | The slice annotation, `Pass::Slice`, `plan.constants` |
| `crates/ferrobridge-codegen/src/{bridge,shim,safe,spelling}.rs` | UPDATE | `&[T]` / `&mut [T]` bridges and shims, constants |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `--prelude` (parse / generate / suggest) |
| `crates/ferrobridge-build/{Cargo.toml,src/lib.rs}` | UPDATE | Builder API, two modes, the `generate` feature |
| `crates/ferrobridge-e2e/build.rs` | UPDATE | Using `ferrobridge-build` |
| `tests/fixtures/{self_include,inline_and_constants,slices}/` | CREATE | Findings 1 and 3, constants, slices |
| `demo/ap_math/{Cargo.toml,LICENSE,README.md,build.rs,ferrobridge.toml,src/**,stubs/**,tests/**}` | CREATE | The demo crate (a separate workspace) |
| `docs/adr/0008-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Decisions 2–3 |
| PRD, `README.md`, `CLAUDE.md`, project notes, `.claude/plans/m5-apmath-measurement.md` | UPDATE | The milestone split, usage, re-measurement |

## Tasks
Each task is done with TDD: a failing test first, then code, then verification.

### Task 0: An experiment for the risky parts (outside the repo)
- **Action**:
  - Compile `AP_Math`'s `.cpp` files with `cc` and link them into a small Rust program; find out which symbols (`AP_HAL`, `AP_InternalError`, `hal`...) need stubs. Finding 4 of the measurement, the biggest risk of M5.
  - Bind a `static inline` function directly with cxx.
  - Write a shim taking `rust::Slice<const uint8_t>`.
- **Validate**: the experiment crate builds and runs; the list of needed stubs is written into the plan notes.

### Task 1: The synthetic translation unit and `prelude`
- **Action**:
  - The parser reads the header not directly but through a `#include "<prelude>"… #include "<header>"` file generated in memory (`CXUnsavedFile`).
  - Only items declared in the target header are taken.
  - `ParseOptions.prelude: Vec<String>`; `--prelude` in the CLI (repeatable).
- **Mirror**: `clang.rs:110`, `lower.rs:68`
- **Validate**:
  - The new `self_include` fixture (a header that includes itself indirectly) is read.
  - All existing parser snapshots pass **unchanged** (same locations and items).

### Task 2: `static inline` and constants (IR v4)
- **Action**:
  - Internal-linkage functions with a definition in the header become `Item::Function`; those without stay `unsupported`.
  - `const`/`constexpr` primitive variables → `Item::Constant { name, namespace, ty, value }`. `value`: `Int(i128)` / `Float(f64)` / `Bool`; a value that cannot be evaluated or is not finite becomes `unsupported`.
  - The `clang_Cursor_Evaluate` and `clang_EvalResult_*` calls are added to `clang.rs` with `SAFETY` comments.
- **Mirror**: `lower.rs:104`, `:128`
- **Validate**: the `inline_and_constants` fixture snapshot (int, float, double, bool, `NaN`, `constexpr`, not evaluable); IR round-trip tests

### Task 3: The slice annotation (analysis)
- **Action**:
  - `ParamAnnotation::Slice { len }`.
  - Rules: the pointer must point to a numeric type; `len` must be an integer-typed parameter of the same function; the same `len` cannot be tied to two slices.
  - `Pass::Slice { len_index }`; the length parameter leaves the Rust signature.
  - Errors with line numbers: a nonexistent `len`, a non-integer `len`, a class pointer.
  - An assumption text; in the `suggest` draft, a `slice` option as a comment for pointer + integer pairs.
- **Mirror**: `rules.rs` (`param_type`), `config.rs`
- **Validate**: `cargo test -p ferrobridge-analysis` (table tests)

### Task 4: Code generation
- **Action**:
  - `&[T]` / `&mut [T]` in the bridge.
  - Shim: `::rust::Slice<const T>`. Length check: `if (s.size() > std::numeric_limits<N>::max()) throw …;`. Call: `f(s.data(), static_cast<N>(s.size()))`.
  - Safe layer: slices are passed as they are; constants become `pub const`.
- **Mirror**: `shim.rs`, `safe.rs`
- **Validate**: codegen snapshots + e2e tests for `slices` and `inline_and_constants` (empty slice, a length over the limit → `Err`, `&mut [f32]` changed in place)

### Task 5: `ferrobridge-build`
- **Action**:
  - The `Bridge` builder.
  - `compile()`: the committed `bridge.rs` + `shim.cc` + user sources → `cxx_build`. `cargo:rerun-if-changed` lines are written.
  - The `generate` feature: generates into `OUT_DIR`, used with `include!(concat!(env!("OUT_DIR"), …))` instead of `#[path]`. The generated file must be compatible with `include!`; M4 deliberately avoided inner attributes.
  - Annotation errors and unbound items are printed as `cargo:warning=`.
  - `ferrobridge-e2e/build.rs` is moved to this crate.
- **Mirror**: `crates/ferrobridge-e2e/build.rs`
- **Validate**: `cargo test -p ferrobridge-e2e` (committed mode); for the `generate` mode, `ferrobridge-build`'s own integration test builds a temporary crate

### Task 6: The `AP_Math` demo
- **Action**:
  - `demo/ap_math/` (a separate workspace, GPL-3.0-or-later).
  - `build.rs`: a clear error saying what to do if `ARDUPILOT_DIR` is missing; if present, `ferrobridge-build` (`generate`) with `AP_Math.h`, `control.h`, `crc.h`, `location.h`, `SCurve.h`, `SplineCurve.h`, `rotations.h`, `definitions.h`.
  - SITL defines, a `hwdef.h` stub, the C++ stubs found in Task 0.
  - `ferrobridge.toml`: `control.h` output parameters `borrowed`, `crc_*` and `fill_nanf` `slice`.
  - Tests: CRC values with known test vectors (e.g. CRC-32 `"123456789"` → `0xCBF43926`), angle conversions, `SCurve` / `SplineCurve` path computations, constants, the `Rotation` enum.
  - README: the commands for a sparse ArduPilot clone and for running.
  - Re-measurement: a "After M5" section in the measurement report.
- **Validate**: `ARDUPILOT_DIR=… cargo test --manifest-path demo/ap_math/Cargo.toml`; no `unsafe` in the demo; coverage goals (≥ 85% without templates, ≥ 30% overall)

### Task 7: Review, ADR and documents
- **Action**:
  - `/ecc:security-review` (focus: the slice length check, correctness of constants, ODR with `static inline`, path escaping in the synthetic translation unit) + `/ecc:cpp-review` (stubs).
  - ADR-0008.
  - The milestone split in the PRD, the README (`build.rs` usage, slices), CLAUDE.md, project notes.
- **Validate**: the `build.rs` example in the README is the same as e2e's `build.rs`

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
ARDUPILOT_DIR=/path/to/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Linking `AP_Math`'s `.cpp`s brings in chained dependencies such as `AP_HAL` and `AP_InternalError` (in the measurement, `AP_Math.h` pulled in 22 other libraries) | High | Task 0 first. If the stubs do not stay small, the demo starts only with the `.cpp`s that have few dependencies (`crc`, `control`, `location`, `SCurve`); the rest are reported as "could not be built" in the measurement |
| The synthetic translation unit changes locations or filtering, and existing snapshots drift | Medium | Task 1's criterion: existing snapshots must stay exactly the same |
| Evaluating constants loses precision in floating point (a `float` constant computed as `double`) | Medium | Check whether a `float` constant converted to `f32` and back to `double` gives the same value; rejected if not. Exact values in snapshots |
| The slice length does not fit in `N` (`uint16_t len`, a slice of 70,000 elements) | Medium | Shim check → `Err`; an e2e test |
| ODR or visibility problems when taking the address of a `static inline` function | Low | Verified in Task 0; called through a shim if there is a problem |
| GPL-derived generated code committed by accident | Low | The demo generates into `OUT_DIR`; `demo/ap_math/.gitignore`; ADR-0008 |
| Scope (Large) | Medium | Templates, fields, operators and CI deliberately left out |

## Acceptance
- [x] `vectorN.h`, `matrixN.h`, `rotations.h` can be read; existing parser snapshots did not change (exactly the same in Task 1; in Task 2 only the IR version 3 → 4 and two reason texts changed)
- [x] `static inline` functions and finite constants are bound; a `NaN` constant is rejected with an explanation
- [x] `&[T]` / `&mut [T]` work with the `{ slice = "len" }` annotation; a length over the limit is `Err`
- [x] `ferrobridge-e2e` uses `ferrobridge-build` in its own `build.rs`
- [x] The demo builds against a real ArduPilot checkout; its tests pass, no `unsafe`, no GPL-derived file in the repo
- [x] Re-measurement: non-template API ≥ 85%, overall ≥ 30%
- [x] ADR-0008 written; security and C++ review done; milestones split in the PRD
- [x] Patterns mirrored, not reinvented

## Implementation notes

### Task 0 (2026-11-27): the risks turned out small
- **ArduPilot checkout:** `~/ardupilot`, commit `437a3301cc`. A different commit from `32e4f8e` in the measurement; the re-measurement uses this commit.
- **`AP_Math` builds.** All 18 `.cpp`s build with these settings:
  - `-std=c++17`
  - the SITL defines from the measurement and an empty `hwdef.h`
  - `__AP_LINE__=__LINE__`, `__STDC_FORMAT_MACROS=1`, which waf adds
  - `APM_BUILD_DIRECTORY=APM_BUILD_UNKNOWN` for `SCurve.cpp`
  - `-I libraries/AP_Common/missing` for `matrix_alg.cpp`. On macOS, `feenableexcept` comes from there.
- **The stubs needed for linking (finding 4) are only four pieces; a single `.cc` is enough:**
  - `AP_InternalError::error` + `AP::internalerror()`. The error is recorded; the program does not stop.
  - `AP_CustomRotations::{from_rotation, rotate}` ×2 + `AP::custom_rotations()`. Only for `ROTATION_CUSTOM_*`; if called, an internal error is recorded.
  - `AP_HAL::panic`: prints the message, then `abort`.

  The rest is libc/libm. The plan's fallback (only the `.cpp`s with few dependencies) was not needed.
- **`static inline constexpr` functions bind directly with cxx**, the overloaded `radians(float)` too (`#[cxx_name]`). Because cxx converts the function pointer to the signature, the right overload is chosen. No ODR or visibility problems.
- **A `rust::Slice<const uint8_t>` shim works.** `crc_crc32(0xFFFFFFFF, "123456789")` gives `0xCBF43926` when inverted. An empty slice works. Giving `crc_crc8` a 300-byte slice makes the `uint8_t` length check return `Err`.
- **A hint for M6:** `wrap_180` is a function template, but `AP_Math.cpp` explicitly instantiates it for `int`/`short`/`float`/`double`. cxx picks the right instance through the `float(*)(float)` conversion. Function templates can be bound through their explicit instantiations.

### Task 1 (2026-11-27): the synthetic translation unit
- **Synthetic unit:** `<header>.ferrobridge.cc` is generated in memory next to the header (`CXUnsavedFile`). It contains `#include "<absolute path>"`.
- **Target file filter:** `clang_getFile` + `clang_File_isEqual` are used. The comparison looks at the macro expansion location, as the old `clang_Location_isFromMainFile` did. The 9 existing snapshots stayed exactly the same.
- **Deviation 1: `prelude` is turned into clang's `-include` argument.** In the plan the prelude was going to be written into the synthetic unit as an `#include` line. The resolution rule of `-include` is already defined in clang (working directory first, then include paths); we do not invent our own rule. Only the header's line remains in the synthetic unit.
- **Deviation 2: a separate `prelude` fixture was created for the prelude.** Adding it to the `self_include` fixture mixed two separate problems.
- **A new problem: end-of-file errors showed the wrong location.** An unclosed parenthesis in the header is noticed at the end of the file, and the end of the file is now in the synthetic unit. So the diagnostic showed `input.h.ferrobridge.cc:1:85`.
  - Fix: such a diagnostic is moved to its first note pointing to another file ("to match this '('"), and the note's text is appended to the message.
  - If there is no note, the diagnostic stays as it is; no diagnostic is dropped.
- **Path restriction:** header paths containing `"` or a line break cannot be written into an `#include` line; they are rejected as `InvalidArgument`.
- **CLI:** `std`, `prelude` and the clang arguments are in a single `ParseArgs` struct (`#[command(flatten)]`). `--prelude` is repeatable in `parse`, `generate` and `suggest`.
- **Verification on ArduPilot:**
  - `vectorN.h` and `matrixN.h` are read without extra arguments (finding 1).
  - `rotations.h`, `SCurve.h` and `SplineCurve.h` are read with `--prelude AP_Math/AP_Math.h` (finding 2).

### Task 2 (2026-11-27): `static inline` and constants (IR v4)
- **Deviation 1: function bodies are no longer skipped (`SkipFunctionBodies` removed).**
  - libclang does not count a function whose body was skipped as a "definition". `clang_isCursorDefinition` and `clang_getCursorDefinition` both look at the body. So `static inline int f() { ... }` looked undefined.
  - Measurement: `AP_Math.h` and the 28 libraries it pulls in take 0.54 s with bodies skipped and 0.45 s without. No difference.
  - Bodies in the header are compiled anyway when the shim is compiled; their errors now show up during `generate`.
- **Since C++11 the linkage of functions in an anonymous namespace is `Internal`, not `UniqueExternal`.** So they are recognized from the namespace path and get a separate reason ("it has no name that can be written from Rust").
- **Deviation 2: `ConstantValue` was split into `Signed(i64)` / `Unsigned(u64)` for integers.** The plan had `Int(i128)`, but `serde_json`'s `Value` type (and insta snapshots) cannot carry `i128`. `Eq` is written by hand: the values are always finite.
- **An extra safety rule: a constant whose initializer contains a function call is rejected.** If a constant depends on another constant, the chain is followed.
  - Why: `static const double X = sqrt(2.0)` is not a C++ constant expression. The compiler may compute the value at run time with libm, while libclang folds it itself at compile time. The two results may differ in the last bit.
  - Because of this rule, `DEG_TO_RAD_DOUBLE` (`asin`), `RAD_TO_DEG_DOUBLE` (depends on it), `WGS84_E` (`sqrt`) and `NaNf` (`nanf`) are not bound in ArduPilot. `WGS84_A/IF/F/B` are bound.
- **A temporary entry in analysis:** for now, analysis records `Item::Constant` as an item skipped as "not bound yet". It changes with the real binding in Task 4.
- **Snapshots:** 12 snapshots became `format_version: 4`. Two reason texts changed in the `namespaces` snapshot. 144 tests pass.

### Task 3 (2026-11-27): the slice annotation (analysis)
- **Plan shape:**
  - The pointer parameter becomes `BridgeType::Slice { mutable, element }` and `Pass::Slice`.
  - The length parameter is not in `FunctionPlan.params`; it is kept as `FunctionPlan.lengths: Vec<LengthParam { position, slice, cpp_type }>`. `position` is its position in C++, `slice` the position of the slice parameter in Rust, `cpp_type` the underlying C++ type (`"unsigned short"`).
  - This way types that cannot be carried into Rust, such as `size_t`, can also be lengths. This structure was chosen over the plan's `Pass::Slice { len_index }`, because the shim needs to know the C++ order and the length's type.
- **`ParamAnnotation` is no longer `Copy` (`Slice { len: String }`).** Deserialize is written by hand. `serde(untagged)` produced an unhelpful message like "data did not match any variant".
- **Wrong slice annotations are errors with line numbers, not warnings.** Cases counted as errors:
  - a nonexistent `len`
  - a non-integer `len`
  - `len` being the parameter itself
  - a non-numeric element (class, `char`, `bool`)
  - a parameter that is not a pointer
  - two slices sharing the same `len`
  - a separate annotation on `len`
- **Element type:** fixed-width integers, `float` or `double`. `char` is rejected (its signedness is platform-dependent); the message suggests `uint8_t`.
- **`suggest`:** for a numeric pointer, below the `borrowed` line, a `# p = { slice = "n" }` option is written for every integer parameter of the same function.
- **Behavior change:** rejecting an integer parameter (e.g. `size_t`) counts as "solvable with an annotation" if the same function has a pointer that could be a slice. So a suggestion is printed for `crc(const uint8_t*, size_t)`.
  - M3's `no_suggestions_when_an_annotation_would_not_help` test used the example `int* p, long n`. Because that signature can now be bound, the example was replaced with "a pointer to an unknown class + `long`".
- **The test helper `uncomment_first_choices`:** now enables only the first option for each table and key, and does not touch the example lines in the draft's header.

### Task 4 (2026-11-27): code generation for slices and constants
- **Shim:**
  - Arguments are built in C++ order; `static_cast<N>(s.size())` is written in place of the length.
  - The length check is done in `unsigned long long`; both sides are non-negative and no value is truncated. If it does not fit, `std::length_error` is thrown.
  - `shim.h` includes `<limits>` and `rust/cxx.h` only if there is a slice. The output of the 5 existing fixtures did not change.
- **An extra rule: every function with a length parameter returns `Result`, even if it is `noexcept`.** The check in a `noexcept` shim would be `std::terminate`. In e2e, giving `count_nonzero(... ) noexcept` a slice of 65,536 elements returns `Err`.
- **Analysis of constants:**
  - Analysis produces `plan.constants`, and the type table is applied. Platform-dependent `long`/`size_t` constants are rejected.
  - Whether the value fits the type is checked. `char` constants are accepted only between 0 and 127.
  - If the same name exists in different namespaces, it gets a prefix (`metric_UNIT`).
  - A constant with the same name as a safe function is skipped (both are in the same namespace in Rust).
- **Generating constants:**
  - `pub const` in the safe layer; `float` constants with the shortest spelling for `f32` (`9.80665`), negatives as `-` + the absolute value.
  - `#[allow(non_upper_case_globals, clippy::approx_constant)]`.
- **e2e proof:** for `WGS84_F` (`1.0 / WGS84_IF`) and `TENTH` (`0.1` → `float`), the Rust `const` is identical at the `to_bits()` level to C++'s run-time value.
- **Fixture changes:**
  - `inline_and_constants`: `int counter` → `inline int counter`. Because the header enters more than one translation unit, there was a link error otherwise.
  - `triple` and `hidden` got `[[maybe_unused]]`; the C++ warnings were silenced at the source.
  - `wgs84_f_at_runtime` / `tenth_at_runtime` were added at the end.
- **The copy of `uncomment_first_choices` in the CLI test was fixed the same way as in analysis.**
- **Total:** 164 tests (+20 in Tasks 3–4), clippy and `cargo doc -D warnings` clean.

### Task 5 (2026-11-27): `ferrobridge-build`
- **Deviation: a shared `Build` instead of a single-header chain (`Bridge::new(header)...compile(name)`).**
  - Both e2e (7 fixtures) and the demo (8 headers) build several bridges with the same include paths, defines, C++ standard and sources into one library.
  - `Build::new().committed(dir).generate(Generate::new(header, "name").include_as(..).config(..).prelude(..)).include_dir(..).define(..).file(..).std(..).flag(..).compile(lib)`.
  - In `generate` mode, include directories, defines and the standard are passed to libclang as they are; `flag` only to the C++ compiler.
- **The Rust side:** the `ferrobridge::include_bridge!("name")` macro (in the runtime crate) includes `$OUT_DIR/ferrobridge/<name>/bridge.rs`. `cxx-build` explicitly supports bridge files inside `OUT_DIR`.
- **The prelude is now a codegen setting (`CodegenOptions.prelude`).**
  - Why: if the header does not compile on its own, the shim must also include the prelude before the header. This holds for `generate --prelude` in the CLI as well. In the first attempt this was done in `ferrobridge-build` by replacing text.
- **Warnings and errors:**
  - Annotation errors and libclang diagnostics stop the build; the message line by line.
  - A single summary `cargo:warning` is printed per bridge for skipped items; the list is at the top of the generated file. The plan's "one warning per item" would be hundreds of lines for `AP_Math`.
- **Files and rebuilds:**
  - Generated files are written only if their content changed; no unnecessary C++ compilation.
  - `rerun-if-changed` is printed for the header, the config, the sources and the include directories.
- **Dogfooding:** `ferrobridge-e2e/build.rs` now uses `ferrobridge-build`'s committed-file mode; 41 e2e tests pass.
- **The `generate` mode integration test (`tests/generate_mode.rs`):**
  - A temporary crate's `build.rs` reads and compiles a header. Covered: slices, `static inline`, a `constexpr` constant, the prelude, a `typedef`'d return.
  - A wrong annotation stops the build with `ferrobridge.toml:2`.
  - No network is used: `--offline`, the workspace `Cargo.lock` is copied. Shared target directory: `target/ferrobridge-build-it/`.
- **Total:** 169 tests. Clippy is clean in both modes (with and without the feature). `cargo doc -D warnings` clean.

### Task 6 (2026-11-27): the `AP_Math` demo
- **Structure:** `demo/ap_math/` is a separate workspace (`[workspace]`), `GPL-3.0-or-later`, `publish = false`.
  - `LICENSE` is a copy of the GPLv3 text from the ArduPilot checkout. That is license text, not code; the GPL also asks for a copy to be included.
  - None of the 17 files entering the repo is generated; the bridges live under `target/…/out/ferrobridge/`.
- **`build.rs`:** 8 headers × `Generate`, prelude `AP_Math/AP_Math.h`. In addition, `AP_Math`'s 18 `.cpp`s, `stubs/ap_stubs.cc`, the defines from Task 0 and `-w` (ArduPilot's own warnings; not passed to libclang). It gives a clear error if `ARDUPILOT_DIR` is missing.
- **Deviation 1: annotations in one file per header (`annotations/<bridge>.toml`) instead of a single `ferrobridge.toml`.** Analysis requires every annotated name to exist in that header (M3: to catch typos). A shared file would give a "not found in the header" error in every bridge. Header sections in `ferrobridge.toml` could be considered later.
- **Deviation 2: `deny(unsafe_code)` at crate level instead of `forbid(unsafe_code)`; a justified `#[allow(unsafe_code)]` on the bridge modules.** `include_bridge!` adds the cxx layer to the user's crate, and cxx's expansion contains `unsafe extern`; `forbid` cannot be relaxed. The tests compile with `forbid(unsafe_code)`. A note was added to the `include_bridge!` documentation.
- **Annotations were written by reading the source.** `crc_crc4` (no length, a fixed 8 words) was deliberately not annotated. The pointer of `crc_crc16_ibm`, which is not `const` but only read, becomes a more restrictive than needed `&mut [u8]`.
- **Tests:** 17 tests against real ArduPilot. CRCs were verified with standard catalog values: CRC-32 `0xCBF43926`, XMODEM `0x31C3`, MODBUS `0x4B37`, IBM-3740 `0x29B1`, MAXIM `0xA1`, SMBUS `0xF4`, DVB-S2 `0xBC`. Also an FNV-1a reference, a 256-byte slice returning `Err`, angles, `control.h` output parameters, `SCurve` (owned object, static method), bit-exact WGS84 constants and the `Rotation` enum.
  - My first expectation for `crc8_dvb_update` (`0xBC`) was wrong: the function uses the 0x07 polynomial (CRC-8/SMBUS, `0xF4`). The test was corrected based on the source; DVB-S2 was verified separately with `crc8_dvb_s2`.
- **Re-measurement:** non-template API 131/146 = **90%** (goal ≥ 85%), overall 131/424 = **31%** (goal ≥ 30%). Without annotations: 68% / 23%. Details: `m5-apmath-measurement.md` → "After M5".
- **The CLI summary line now counts constants too.**
- **Not verified:** building with a sparse clone (needs the network; noted in the README). Linux.

### Task 7 (2026-11-27): review, ADR-0008, documents
- **Security review (`/ecc:security-review`, focused).** Three findings fixed:
  1. **The constant initializer check took exponential time.** On an `A2 = A1 + A1` chain the generator hung at 60 steps; the test timed out after 60 seconds. Fix: `InitializerCheck` remembers verified variables and has a total node budget (100,000; when exceeded, the constant is rejected conservatively). Test: `chained_constants_are_checked_in_linear_time`.
  2. **Platform-dependent constants.** Initializers containing `sizeof`/`alignof` (`CXCursor_UnaryExpr`) and `offsetof` (a reference to a field without an object, `CXCursor_MemberRef`) are rejected. In addition, integer/`bool` constants are re-checked during the C++ build with `static_assert(::ns::X == value)` in `shim.cc`; if the committed file is built on another platform or with other defines, a difference becomes a compile error. Floating-point constants cannot be used in a `static_assert`; the initializer check protects them.
  3. **Line injection into the generated C++ through include paths.** If `--include-as`, `--shim-include` or `prelude` contain `"`, a line break or NUL, `CodegenError::InvalidInclude` is returned.
- **Found without problems:**
  - `CXUnsavedFile`: libclang copies the contents.
  - The set of child diagnostics is not released (as documented).
  - No path escaping in the synthetic unit.
  - `include_bridge!` / writing into `OUT_DIR`: the name is validated as an identifier.
  - The argument order and the length check are correct.
- **C++ review (`/ecc:cpp-review`, the `ecc:cpp-reviewer` agent): no critical findings.**
  - The `static_assert` mismatch for negative `char` constants and an `__int128` length type are already blocked in analysis (`char` 0–127; not a 128-bit integer).
  - Documented:
    - the non-null pointer of an empty slice (README and the promise text)
    - local state in `static` functions being a separate copy (README)
    - stub limits: not atomic, `abort` on custom rotations, symbol clashes with real ArduPilot (top of the stub file)
  - **Still open:** the length type is written as its underlying name on the generating platform (`size_t` → `unsigned long`). On LLP64 (Windows), a slice longer than 4 GiB gets a spurious `Err`. It is on the safe side; should be handled together with Windows support.
- **A test harness bug was fixed:** the two temporary crates in `generate_mode.rs` had the same package name. Because Cargo computes the root package's identity independently of the path, the faulty crate considered the other one's `build.rs` output "fresh". Each crate got its own name; it passes on two consecutive runs too.
- **ADR-0008 written;** a note pointing to it was added to ADR-0004.
