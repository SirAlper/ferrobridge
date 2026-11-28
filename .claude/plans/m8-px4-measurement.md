# Measurement for Milestone 8: ferrobridge on PX4 (2026-11-28)

**Goal:** before v0.2 builds a CMake integration, find out:
- which PX4 library the demo should use;
- how far ferrobridge 0.1.0 gets on it;
- what PX4's build does to headers and flags;
- which of two CMake integration shapes holds up on a real CMake + Corrosion build.

**Source:**
- PX4-Autopilot `v1.18.0` (commit `935addf`), shallow clone outside the repo; build `px4_sitl_default` on macOS (arm64).
- No PX4 code is copied into this repo; only numbers, symbol names and file:line references are here. PX4 is BSD-3-Clause, so this is a choice, not a license requirement.
- Tool: ferrobridge 0.1.0 (`main` at `e96b42a`), Homebrew libclang 23.1.6, CMake 4.5.1, Ninja 1.13.2.

## Summary
- **PX4 SITL builds unchanged with CMake 4.5.**
  - 1163 build steps, no compatibility setting needed.
  - The build writes `build/px4_sitl_default/compile_commands.json` (911 entries; `CMAKE_EXPORT_COMPILE_COMMANDS ON`, `CMakeLists.txt:315`).
  - The plan's main build risk did not happen.
- **ferrobridge 0.1.0 reads PX4 headers with PX4's real flags.** It only has to drop warning, optimization and output flags. Required, all handled today through `--`:
  - a forced include (`-include visibility.h`);
  - `-std=gnu++17`;
  - a define with spaces and parentheses (`noreturn_function=__attribute__((noreturn))`).
- **The generated shims compile under PX4's own flags**, including `-Wall -Wextra -Werror -Wshadow -Wdouble-promotion -Wcast-align`. This was checked for all three candidates.
- **Coverage today** (template members counted once, float instantiations):

  | Library | Public items | Bound, no config | Bound with `[templates]` + annotations | Main blockers |
  |---|---|---|---|---|
  | `geo` | 28 | 17 (61%) | **28 (100%)**, 19 annotation lines | none left |
  | `mathlib` | 118 | 11 (9%) | **64 (54%)** | 36 function templates |
  | `matrix` | 371 | 3 (1%) | ~66 (18%) | inheritance between templates, `size_t` template parameters, function templates |
  | `geo` + `mathlib` (one bridge) | 146 | 28 (19%) | **92 (63%)** | as above |

- **Recommended demo library: `geo` + `mathlib` as one bridge.** `matrix::Vector2f/3f` are instantiated as supporting types and are not counted.
  - `matrix` is out of reach for v0.2.
  - `geo` alone is too small to say much about coverage.
- **CMake spike:** both integration shapes work end to end (C++ exception → `Err`, flags from the target, regeneration on header and define changes). Shape B, where CMake compiles the C++, is recommended in ADR-0013.

## Method
- Flags: the `compile_commands.json` entry of `src/lib/geo/geo.cpp`, filtered to `-D`, `-I`, `-include <file>` and `-arch <arch>` (26 arguments).
  - All three libraries get their flags from directory-scoped `include_directories`/`add_definitions` (`cmake/px4_add_common_flags.cmake:176-200`), so one entry is representative.
  - Dropped: `-W*`, `-f*`, `-O*`, `-g`, `-o`, `-c` and the source file.
- `ferrobridge generate` with all headers of a library as one bridge (as in `AP_Math`):
  - Options: `--std gnu++17`, `--include-as` relative to `src/lib`.
  - `mathlib` and `matrix` also get `--prelude matrix/math.hpp`: `Euler.hpp`, `SparseVector.hpp`, `TrajMath.hpp` and `WelfordMeanVector.hpp` do not compile on their own, and in PX4 they always come after `math.hpp`.
- `[templates]`:
  - `mathlib`: the six class templates with only type parameters, `<float>`.
  - `matrix`: `Vector2/3/4`, `Quaternion`, `Dcm`, `Dcm2`, `Euler`, `AxisAngle` `<float>`, plus `Matrix<float, 3, 3>`, which is rejected.
- Annotations for `geo`:
  - The `ferrobridge suggest` draft, kept only after reading `src/lib/geo/geo.cpp`. Every pointer/reference output is only written, never kept: `geo.cpp:67`, `:90`, `:129-157`, `:177-213`, `:275-304`, `:345-377`.
  - 19 annotation lines.
- **Denominator:** public API items counted with libclang's Python bindings (`clang.cindex`, Homebrew LLVM 23), deduplicated by USR. Counted once each:
  - free functions and function templates;
  - public constructors, methods, operators and fields of classes and class templates (members once per template, as in M5);
  - enums;
  - namespace-scope constants.
  
  Not counted:
  - destructors, deleted functions, private/protected members;
  - static data members of type-trait helpers;
  - out-of-class member specializations.
  
  `matrix` is counted through `math.hpp` as one translation unit. Unlike M5's script this uses the compiler's view, but the definition still decides the numbers: inherited members are not counted again in the derived class. Expect about ±5%.
- The scripts (flag filter, measurement driver, counter) stayed in the session scratchpad and are not committed. The commands above reproduce the numbers.

## Numbers per library
### `geo` (`src/lib/geo/geo.h`, 28 items)
- 11 free functions, 10 `MapProjection` members, 3 fields of `crosstrack_error_s`, 4 constants.
- **Without configuration:** 17 bound. Skipped:
  - 8 raw pointer outputs (`double*`, `float*`, `crosstrack_error_s*`);
  - 2 non-const `T&` outputs (`MapProjection::project/reproject`);
  - 1 return of `matrix::Vector2f` (not instantiated).
- **With 19 annotation lines and `Vector2f` instantiated:** 28/28.
- `geo` is a real compiled library: `add_library(geo geo.cpp)` → `libgeo.a` (`src/lib/geo/CMakeLists.txt:34`). All bound functions are defined there (`nm`), so the demo links against an existing CMake target.

### `mathlib` (14 headers, 118 items)
- 36 function templates, 13 functions, 1 constant, 68 members of class templates.
- **With `[templates]`:** 64 bound. Unbound:

  | Reason | Items | v0.2? |
  |---|---|---|
  | Function templates (`constrain<T>`, `min`, `max`, `radians`, `sq`, `expo`…) | 36 | No: function template instantiation is out of v0.2 scope (PRD) |
  | Class templates with an integer parameter (`WelfordMeanVector<T, size_t N>`, `MedianFilter<T, int WINDOW>`) | 12 | No: ADR-0009 allows type parameters only |
  | Array parameters (`T samples[]` + count, `float a[3]`) | 4 | Possible: `T[]` is a pointer; with `{ slice = "n" }` it would work if treated as one |
  | `auto` return (`WelfordMean::count`) | 1 | Possible: libclang gives the deduced type |
  | Nested enum (`SecondOrderReferenceModel::DiscretizationMethod` as a parameter) | 1 | Already planned ("nested types") |

- Not in the denominator but reported:
  - 3 out-of-class member specializations ("not recognized"; their declarations are bound);
  - the `is_floating_point` helper (3 static members).

### `matrix` (21 headers, 371 items)
- 78 function templates, 3 functions, 288 members of class templates, 2 fields. The largest owners are `Matrix` (61), `SliceT` (36), `Quaternion` (25), `SquareMatrix` (24) and `Vector` (23).
- **With nine `<float>` instantiations:** about 66 members bound (18%). Blockers:
  - **Inheritance between templates:**
    - `Vector3<T>` → `Vector<T, 3>` → `Matrix<T, 3, 1>`; `Quaternion<T>` → `Vector4<T>`.
    - Most of what a user calls (`norm`, `normalized`, `dot`, element access) lives in the bases.
    - 8 "inheritance" warnings; the bases' members are not reachable.
  - **`size_t` template parameters:** `Matrix<Type, M, N>`, `Vector<Type, M>`, `SliceT<…>` cannot be instantiated (ADR-0009). `size_t` is also a platform-dependent type, which v0.1 does not bind.
  - **Classes by value:** operators take `Vector3` by value (9).
  - **Template-dependent types in signatures:** 15.
- **Conclusion:** `matrix` needs inheritance mapping and non-type template parameters. Both are outside v0.2 and each needs an ADR. `Vector2f`/`Vector3f` can still be instantiated as supporting types for `geo`/`mathlib`; then their own members are bound (constructors from floats, `cross`, `norm`…).

## Cumulative scenarios: `geo` + `mathlib` (146 items)
| Capability | Gained | Bound | Coverage |
|---|---|---|---|
| ferrobridge 0.1.0, no configuration | | 28 | 19% |
| + `[templates]` (mathlib class templates, `Vector2f/3f`) | +45 | 73 | 50% |
| + 19 annotation lines for `geo` | +19 | 92 | **63%** |
| + small fixes (`T[]` as pointer for slices, `auto` return, nested enum) | +6 | 98 | 67% |
| + class templates with integer parameters | +12 | 110 | 75% |
| + function template instantiation | +36 | 146 | ~100% |

**Annotation burden:** 19 lines for 92 bound items ≈ 21 per 100, close to `AP_Math` (18).

## PX4 build and integration
- **Flags** (`geo.cpp` entry):
  - 7 defines, among them `__PX4_POSIX`, `__PX4_DARWIN`, `ENABLE_LOCKSTEP_SCHEDULER`, `noreturn_function=__attribute__((noreturn))`;
  - 15 include directories, including two in the build directory;
  - `-include visibility.h`, `-std=gnu++17`, `-arch arm64`;
  - and the full PX4 warning set with `-Werror -Wfatal-errors` (must not reach libclang).
- **Generated headers:**
  - `px4_boardconfig.h` is generated **at configure time** by Kconfig (`cmake/kconfig.cmake:72`). `mathlib`, and through it `geo`, include it through `px4_platform_common/defines.h:43`.
  - The 262 uORB topic headers are generated **at build time** (`msg/CMakeLists.txt:368`, target `uorb_headers`). The candidates do not include them, but every PX4 module depends on them (`cmake/px4_add_module.cmake:155`). A Rust module that reads or writes topics will need them, so generation must be orderable after a build step.
- **Modules:**
  - `px4_add_module` creates a `STATIC` library (`cmake/px4_add_module.cmake:151`).
  - `DEPENDS` entries of type `STATIC_LIBRARY` or `INTERFACE_LIBRARY` are linked into it (`:233-242`). Corrosion's crate target is an `INTERFACE` library, so it qualifies.
  - SITL links all modules into one `px4` executable (`platforms/posix/CMakeLists.txt:65`).
- **Out-of-tree modules** are supported with `EXTERNAL_MODULES_LOCATION` (`CMakeLists.txt:224`). The demo can live in this repo without copying PX4 code.
- **Exceptions and RTTI:**
  - SITL has exceptions on, and `-fno-rtti` is not used for `px4_sitl` (`cmake/px4_add_common_flags.cmake:165`).
  - NuttX targets use `-fno-exceptions` (`platforms/nuttx/cmake/px4_impl_os.cmake:86`). They are out of v0.2 scope, but `[library] exceptions = false` (ADR-0011) already exists for them.
- **No Rust in PX4 v1.18:** 0 `.rs` files; no Corrosion or Cargo in its CMake files.

## CMake spike (throwaway, scratchpad)
**Setup:**
- A C++ library target `mylib`:
  - include directory and defines on the target, one of them with parentheses;
  - one header generated by `add_custom_command` at build time (standing in for uORB);
  - a class that throws.
- Corrosion `v0.5.2` via `FetchContent`, a Rust `staticlib` crate, and a C++ `main` that calls it.
- ferrobridge 0.1.0, unchanged.

**Shape A: Cargo compiles the C++.**
- CMake writes the target's `INCLUDE_DIRECTORIES` and `COMPILE_DEFINITIONS` (generator expressions) to a file with `file(GENERATE)`.
- `corrosion_set_env_vars` passes the path, and the crate's `build.rs` feeds the file to `ferrobridge-build` (`generate` feature).
- `add_dependencies(cargo-build_<crate> <generated-header target>)` orders it.

**Shape B: CMake compiles the C++.**
- `add_custom_command` runs `ferrobridge generate` at build time, with the same generator expressions on the command line (`COMMAND_EXPAND_LISTS`). It then runs `cxxbridge` (`cxxbridge-cmd 1.0.205`, matching the crate's `cxx`) for the cxx glue and `rust/cxx.h`.
- `add_library(<name> STATIC shim.cc bridge.rs.cc)` is linked to `mylib`.
- The crate does `include!(env!("FERROBRIDGE_BRIDGE_RS"))`; its `build.rs` only emits `rerun-if-changed`.

| | Shape A | Shape B |
|---|---|---|
| Builds and runs (result 506, exception → `Err`) | ✓ | ✓ |
| No-op rebuild | 0.14 s, nothing regenerated | same |
| Header edit (new function) | regenerated, new function bound | same |
| Define changed in CMake (`MYLIB_SCALE` 3 → 4) | regenerated, result 508 | same |
| Header included indirectly changes | caught: Cargo scans the include directories | **missed** unless ferrobridge writes a depfile |
| C++ compiled with | `cc` crate: CMake's toolchain and flags are not used | CMake, with the target's own toolchain and flags |
| Extra tools | none | `cxxbridge`, version-matched to `cxx` |
| Cost in PX4 | Cargo would rescan ~5,000 files under PX4's include directories on every build (`rerun-if-changed` on directories, `crates/ferrobridge-build/src/lib.rs:205`) | none |

## Findings
1. **CMake 4.5 is not a problem for PX4 v1.18.** This risk is retired for the local build.
2. **The flags libclang must not see are warnings and optimizations, not includes or defines.** Filtering by kind (keep `-D`, `-I`, `-include`, `-isystem`, `-std`, `-arch`, `--target`) is enough for PX4. Each dropped flag should be visible, not silent.
3. **`ferrobridge-build` has no forced-include API:** `-include` must be passed twice, with `Generate::clang_arg` and with `Build::flag`. This matters for shape A and for `build.rs` users who copy PX4 flags.
4. **Defines with spaces and parentheses survive** both the CLI (`--`) and generator expressions (`VERBATIM`). A response file is still safer for long flag lists and for Windows command-line limits.
5. **Transitively included headers need a depfile** for build-time generation in CMake. libclang can list a translation unit's inclusions (`clang_getInclusions`), so `ferrobridge generate --depfile` is a small addition in `clang.rs`.
6. **A second tool (`cxxbridge`) is fragile:** its version must match the `cxx` crate, as Corrosion's own cxx support does it. If ferrobridge writes the cxx glue itself (the `cxx-gen` library crate), there is one tool and one version.
7. **Toolchain mismatch is visible even in the spike.** On macOS the linker warned that `cxx.o` (built by `cc`) targets macOS 27.2 while CMake targets 26.0. In shape B only cxx's own runtime C++ is built by `cc`; the bridge C++ follows CMake.
8. **One Rust `staticlib` per final binary.** Each Rust `staticlib` contains its own copy of the Rust standard library, and SITL links every module into one `px4` executable. Two Rust modules as two `staticlib`s would collide, so the demo uses one crate, and the docs must say so.
9. **PX4 headers are not self-contained either.** Four `matrix`/`mathlib` headers need `matrix/math.hpp` first (`--prelude`, as with `AP_Math`).
10. **`mathlib` needs a configure-time generated header** (`px4_boardconfig.h`), and other modules need build-time generated ones (uORB). The demo therefore exercises "flags and generated files from the build", unlike a header-only library with plain `-I`.
