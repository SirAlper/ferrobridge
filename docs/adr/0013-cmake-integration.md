# ADR-0013: CMake integration: build-time generation, C++ compiled by CMake

**Date**: 2026-11-28
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

v0.2's goal is a CMake project that adds a Rust module calling its own C++ code, without a hand-written bridge and without copying flags by hand (PRD, v0.2 hypothesis). v0.1 works only from a Cargo `build.rs` (ADR-0004, ADR-0008). The M8 measurement on PX4 `v1.18.0` (`.claude/plans/m8-px4-measurement.md`) found five constraints:

- **Flags.** PX4's flags for a header come from directory-scoped `include_directories`/`add_definitions` and `add_compile_options`. They include:
  - a forced include (`-include visibility.h`);
  - `-std=gnu++17`;
  - a define with spaces and parentheses;
  - `-Werror`, which must not reach libclang.
- **Generated headers.** Some are generated at configure time (`px4_boardconfig.h`), others at build time (262 uORB headers, target `uorb_headers`).
- **Linking.** Modules are `STATIC` libraries linked into one `px4` executable. Corrosion's crate target can be a module dependency.
- **The spike.** A throwaway CMake spike tried two shapes. Both worked:
  - (A) Cargo compiles the C++, CMake hands the flags to `build.rs`;
  - (B) CMake generates at build time and compiles the C++.
  
  They differ in which toolchain compiles the C++, in how indirect header changes are tracked, and in the cost on a large source tree.
- **Corrosion.** `corrosion_import_crate` is the established way to build a Cargo crate from CMake. Its cxx support (`corrosion_add_cxxbridge`) is experimental and binds only hand-written bridges.

## Decision

1. **A CMake module `cmake/FerroBridge.cmake` provides `ferrobridge_add_bridge()`:**
   ```cmake
   ferrobridge_add_bridge(<name>
     TARGET <cpp-target>            # whose headers are bound and whose flags are used
     HEADERS <header>...            # the bridge's headers (one bridge, ADR-0009)
     [INCLUDE_AS <path>...]         # how the shim includes them
     [CONFIG <ferrobridge.toml>]
     [PRELUDE <header>...]
     [DEPENDS <target>...]          # build steps that produce headers it reads (e.g. uorb_headers)
     CRATE <corrosion-crate>)       # the Rust crate that includes the bridge
   ```
   It creates a `STATIC` library `<name>_bridge` (shim + cxx glue) linked `PUBLIC` to `TARGET`. It links that library into the crate's Corrosion target and tells the crate where `bridge.rs` is.
2. **Generation happens at build time** in an `add_custom_command`, never at configure time:
   - its outputs depend on the headers, `ferrobridge.toml` and the `DEPENDS` targets;
   - a depfile written by ferrobridge lists every header the translation unit included, so indirect changes regenerate too.
3. **Flags come from the target, as data.** `file(GENERATE)` writes a response file from the target's:
   - `INCLUDE_DIRECTORIES`;
   - `COMPILE_DEFINITIONS`;
   - `COMPILE_OPTIONS`;
   - `CXX_STANDARD`/`CXX_EXTENSIONS`.
   
   ferrobridge filters it. It keeps `-D`, `-U`, `-I`, `-isystem`, `-iquote`, `-include`, `-std`, `-arch`, `--target` and `--sysroot`, drops the rest, and prints one warning per kind of dropped flag. No include path or define is repeated in the user's CMake.
4. **CMake compiles all bridge C++** (the shim and the cxx glue) with the project's own compiler, flags and cache.
   - ferrobridge writes the cxx glue itself (`bridge.rs.cc`, `bridge.rs.h`, `rust/cxx.h`) through the `cxx-gen` library crate.
   - Users install one tool, and the glue version follows the `ferrobridge` version.
5. **Rust side:**
   - The crate includes the bridge with a new `ferrobridge::include_external_bridge!("<name>")`, which reads the path from an environment variable set by the module.
   - A one-line `build.rs` helper in `ferrobridge-build` emits the `rerun-if-changed` lines.
   - The safe API is the same as in v0.1.
6. **Built on Corrosion's stable API only:**
   - `corrosion_import_crate`, `corrosion_set_env_vars` and the `cargo-build_<crate>` target;
   - tested with Corrosion `v0.5.2`, documented as "≥ 0.5".
   - **Minimum CMake 3.22:** depfiles with the Makefile generator need 3.20, and Corrosion's next major needs 3.22.
7. **How users get the module:** `FetchContent` of this repository at the release tag.
   - The module finds the `ferrobridge` command (`FERROBRIDGE_EXECUTABLE`, otherwise `find_program`).
   - It stops with a clear error if the command's version is not the module's version (ADR-0012).
8. **`compile_commands.json` is not needed for CMake.**
   - It becomes the input for other build systems: `ferrobridge generate --compile-commands <db> --for <source>`. The same filter is used.
   - This is v0.2 Milestone 9, together with the response file, the filter and the depfile.
9. **One Rust `staticlib` per final executable.** Each `staticlib` carries its own Rust standard library, so several Rust modules in one binary (such as PX4's `px4`) are put in one crate. The module documents this and does not try to merge crates.
10. **The PX4 demo is an out-of-tree PX4 module in `demo/px4/`.**
    - It is loaded with `EXTERNAL_MODULES_LOCATION`, against an external PX4 checkout (`PX4_DIR`), and targets SITL only.
    - It contains no PX4 code; generated files go to the build directory.
    - It is not part of the root workspace. Its license is `MIT OR Apache-2.0`, like the project: PX4 is BSD-3-Clause, and nothing in the demo derives from it.

## Alternatives considered

### Alternative 1: Cargo compiles the C++ (spike shape A)
- **Pros:**
  - Works with v0.1 as it is.
  - No extra tool.
  - Cargo's directory scan also catches indirect header changes.
- **Cons:**
  - The bridge C++ is compiled by the `cc` crate, not with the CMake project's compiler, flags and cache. In the spike this already showed as a macOS deployment-target mismatch.
  - Flags have to be translated into `ferrobridge-build` calls, and there is no forced-include API.
  - Cargo would rescan the ~5,000 files under PX4's include directories on every build.
  - CMake cannot see the C++ sources.
- **Why not chosen:** a CMake integration whose C++ does not follow the CMake project's toolchain is the build glue v0.2 wants to remove. Shape A stays available to `build.rs` users.

### Alternative 2: generation at configure time (`execute_process`)
- **Pros:** simpler; the files exist before the build.
- **Cons:**
  - Headers generated during the build (PX4's uORB topics) cannot be read.
  - Changes are picked up only on reconfigure.
  - The flags of a target are only final at generate time.
- **Why not chosen:** it fails PX4's real case.

### Alternative 3: `corrosion_add_cxxbridge` with a separate `cxxbridge` command
- **Pros:** an existing Corrosion feature; the version matching is done by Corrosion.
- **Cons:**
  - Experimental.
  - It expects the bridge inside the crate's source tree, written by hand.
  - It is a second tool whose version must match `cxx`. In the spike this worked only after installing `cxxbridge-cmd 1.0.205` to match the crate.
- **Why not chosen:** generated bridges live in the build directory. One tool (`ferrobridge`, with `cxx-gen` built in) is simpler for users.

### Alternative 4: `find_package(ferrobridge)` from an installed package, or `ferrobridge cmake-module` writing the file
- **Pros:**
  - `find_package` is the most CMake-native form.
  - An embedded module always matches the command's version.
- **Cons:**
  - `cargo install` installs only a binary, so there is nowhere to put a CMake package config.
  - A written-out file is vendored into every project and goes stale silently.
- **Why not chosen:** `FetchContent` at a tag needs no install step, and the version check catches mismatches. An embedded module can be added later without changing the function's interface.

## Consequences

### Positive
- No flag is repeated by hand. Defines with spaces and parentheses travel as data in a response file, not on a shell command line.
- The bridge C++ is compiled like the rest of the project, with the same compiler, warnings and cache. M8 checked that the shims compile under PX4's `-Werror` warning set.
- Generated headers, including build-time ones, are handled by ordinary CMake dependencies.
- The Cargo flow of v0.1 is unchanged. The new pieces (response file, filter, depfile, cxx glue) are reusable from the CLI and `build.rs`.

### Negative
- ferrobridge gains a dependency on `cxx-gen` and must keep it in step with the `cxx` version the `ferrobridge` runtime crate accepts.
- The crate's C++ glue no longer comes from `cxx-build` in this mode. There are now two supported ways to build a bridge (Cargo and CMake), and both need e2e tests in CI.
- Users of the CMake module need a Rust toolchain and libclang at build time; a C++-only build cannot bind headers.

### Risks
- **Version skew between `cxx-gen` in the CLI and the `cxx` crate in the user's lockfile.** Mitigation:
  - ferrobridge pins `cxx` in the runtime crate to the minor range the CLI was built with;
  - M10 tests a mismatched patch version;
  - the module's version check covers ferrobridge itself.
- **Flag filtering drops something libclang needs** (e.g. `-fms-extensions` or a `-f` flag that changes the language). Mitigation:
  - one warning per dropped kind;
  - an explicit `EXTRA_CLANG_ARGS` option;
  - the filter is tested against PX4's real flag list.
- **PX4's own CMake changes between releases.** Mitigation: the demo pins a PX4 tag, and the module relies only on standard target properties, not on PX4 internals.

## Amendment (2026-11-28, Milestone 9)
Milestone 9 implemented decisions 2 (depfile), 3 (flags as data), 4 (the cxx glue) and 8 (`compile_commands.json`). Decision 3 is refined as follows; the rest stands.

- **The response file holds one argument per line**, exactly as passed to the compiler: no quoting, no `@file` syntax, blank lines ignored. CMake writes it with `file(GENERATE … CONTENT "$<JOIN:…,\n>")`, so a define such as `noreturn_function=__attribute__((noreturn))` needs no escaping. GCC/clang `@file` quoting was not chosen: CMake would have to quote, and spaces and backslashes would become a source of bugs.
- **`compile_commands.json` is read through libclang** (`clang_CompilationDatabase_*`), which splits `"command"` strings the way clang does; there is no shell splitter of our own. The entry is chosen by canonical path, or by its trailing components when a relative `--for` path does not exist from the working directory. libclang adds `--driver-mode=…` after the compiler; the filter drops it.
- **What the filter keeps** (as one or two arguments, joined or separate; relative paths are made absolute against the entry's directory):
  - preprocessor: `-D`, `-U`, `-I`, `-isystem`, `-iquote`, `-idirafter`, `-iframework`, `-F`, `-include`, `-imacros`;
  - the standard and library: `-std=` (moved out of the list, see below), `-stdlib=`;
  - the target: `-arch`, `--target=`/`-target`, `--sysroot`, `-isysroot`, `--gcc-toolchain=`, `-m32`/`-m64`, `-march=`, `-mcpu=`, `-mfpu=`, `-mfloat-abi=`, `-mthumb`/`-marm`, `-mmacosx-version-min=`;
  - flags that change the language: `-f[no-]exceptions`, `-f[no-]cxx-exceptions`, `-f[no-]rtti`, `char` signedness, `-fshort-enums`, `-fshort-wchar`, `-fms-extensions`, `-fms-compatibility`, `-f[no-]char8_t`, `-fpack-struct*`, `-f[no-]blocks`, `-ffast-math`, `-ffinite-math-only`, `-fopenmp`, `-ffreestanding`, `-fno-builtin`, `-fno-operator-names`, `-f[no-]sized-deallocation`, `-f[no-]aligned-allocation`, `-f[no-]declspec`, `-pthread`.
- **Three levels for what is dropped:**
  - without a message: the compiler (and a `ccache`-style launcher before it), `-c`, `-o <file>`, the source file, `-x <lang>`, `-M…` dependency flags, `--driver-mode=`;
  - one `ℹ` line per kind: warnings (`-W…`, `-w`, `-pedantic`, `-Qunused-arguments`), optimization and debug (`-O…`, `-g…`), code generation and linker flags (other `-f…`, `-Wl,`, `-L`, `-l`). PX4 has ~45 of these on every command; a warning that is always there would not be read;
  - one `⚠` listing them: anything else (`-Xclang …`, `--param …`, `-Wp,…`, `-fmodules…`), and flags whose value is missing.
- **Arguments after `--` are never filtered**, so a dropped flag can always be passed on purpose.
- **Standard precedence:** an explicit `--std` (or `Build::std`) wins over a `-std=` from a flag source (the last one wins), which wins over the default `c++17`.
- **`ferrobridge-build`** reads the same sources (`Build::flags_file`, `Build::compile_commands`). Defines, include paths, preprocessor and language flags go to both libclang and the C++ compiler. Target flags go to libclang only, because in `build.rs` the compiler's target comes from Cargo (`cc`). Defines and undefines keep their order. For these sources, Cargo is told to rerun on the database or flags file and on each header libclang read, not on the include directories, which avoids rescanning PX4's ~5,000 files.
- **`cxx-gen` is pinned exactly** (`=0.7.198`, matching `cxx 1.0.198`), and every glue file names both versions in its first line.

## References
- `.claude/plans/m8-px4-measurement.md` (measurement, PX4 build notes, spike results)
- `.claude/plans/m9-flags-as-data.plan.md` (the amendment's implementation and its PX4 check)
- ADR-0004 (committed generated code), ADR-0008 (build-time generation), ADR-0009 (multi-header bridges), ADR-0011 (libraries without exceptions), ADR-0012 (versioning)
