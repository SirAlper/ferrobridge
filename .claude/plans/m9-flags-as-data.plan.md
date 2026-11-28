# Plan: Milestone 9: flags as data

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 9: Flags as data
**Inputs**:
- `docs/adr/0013-cmake-integration.md` (decisions 3, 4 and 8 are implemented here)
- `.claude/plans/m8-px4-measurement.md` (PX4's real flags, findings 2–6)
**Complexity**: Large (four new capabilities; new libclang calls; one new dependency)
**Status**: Approved and implemented (2026-11-28); see "Implementation notes" at the end

## Summary
Today flags reach libclang only as arguments typed by hand: `-- <clang args>`, or `Build::include_dir/define/flag`. M10's CMake module needs four things from ferrobridge that do not exist yet.

1. **Flags from a file.** CMake writes the target's flags into a response file and ferrobridge reads it, so no flag is retyped and no shell quoting is involved.
2. **Flags from `compile_commands.json`**, for non-CMake build systems and for CLI users. One entry is picked with `--for <source>`.
3. **A filter.** It keeps what libclang needs to read the headers the way the compiler does, and drops warnings, optimization and output flags (e.g. PX4's `-Werror -Wfatal-errors`). Every drop is reported, never silent.
4. **Outputs CMake can build from:**
   - a **depfile** of every header the translation unit read, so indirect header changes regenerate;
   - the **cxx C++ glue** (`bridge.rs.cc`, `bridge.rs.h`, `rust/cxx.h`), written by ferrobridge itself through `cxx-gen`.

`ferrobridge-build` gets the same flag inputs, so `build.rs` users can reuse a compilation database instead of copying flags. The shim is then compiled with the same defines it was read with.

**Measurable goal (PRD):** "`ferrobridge generate` takes flags from a response file or a `compile_commands.json` entry, filters what libclang does not need (with a warning per kind), writes a depfile and the cxx C++ glue; the `AP_Math` demo and e2e tests keep passing."

**Checked by (end of M9, manual):** `ferrobridge generate --compile-commands ~/PX4-Autopilot/build/px4_sitl_default --for src/lib/geo/geo.cpp` binds `geo` + `mathlib` with M8's numbers (92 of 146) and **no hand-copied flags**, M8's filter script no longer needed. The written cxx glue and shim compile with PX4's own compile command.

## Findings while grounding the plan
- **The parser already owns the clang arguments.**
  - `ParseOptions { std, extra_args, prelude, instantiate }` (`crates/ferrobridge-parser/src/lib.rs:24`).
  - `clang_args` (`:336`) puts `-x c++ -std=… [-isysroot] [-resource-dir]`, then `extra_args`, then `-include` for each prelude file.
  - Filtering and reading flag sources belong next to it, in a new `flags` module.
- **libclang can read compilation databases itself.** `clang_CompilationDatabase_fromDirectory`, `…_getAllCompileCommands`, `clang_CompileCommand_getArg/getDirectory/getFilename` are in `clang-sys` 1.9.1. libclang splits a `"command"` string the way clang does, so there is no new dependency and no shell splitter of our own.
- **libclang can list a translation unit's headers.** `clang_getInclusions` is in `clang-sys`; its visitor is a plain `extern "C" fn` (`CXInclusionVisitor`). It is new `unsafe` and goes into `clang.rs`, which is the rule.
- **`parse_headers` returns only `Module`** (`lib.rs:153`). The included files are not IR: adding them to `Module` would change the IR schema and every snapshot. A separate result type keeps the IR unchanged.
- **`--std` has a clap default** (`crates/ferrobridge-cli/src/main.rs:79`, `c++17`), so the code cannot tell an explicit `--std` from the default. A `-std=gnu++17` coming from a build system needs a precedence rule (decision 4).
- **Relative paths in `compile_commands.json`** are relative to the entry's `directory`, not to where ferrobridge runs. `-I`, `-isystem` and `-include` paths must be made absolute against it.
- **`cxx-gen`:**
  - `cxx-gen 0.7.198` pairs with the `cxx 1.0.198` in our lockfile; license `MIT OR Apache-2.0`, MSRV 1.85.
  - API: `generate_header_and_cc(TokenStream, &Opt) -> GeneratedCode { header, implementation }`, and `HEADER` is the text of `rust/cxx.h`.
  - New transitive crates: `codespan-reporting` (and its `termcolor`, `unicode-width`) and `indexmap`. `proc-macro2`, `quote` and `syn 3` are already in the tree.
- **The CLI always rewrites its outputs** (`write_files`, `main.rs:307`); `ferrobridge-build` writes only if changed (`lib.rs:242`).
  - For CMake, "always write" is the right behavior: an `add_custom_command` output older than its inputs would rerun on every build.
  - It is left as it is, and M10 decides about `restat`.
- **`Build::flag` is "C++ compiler only, not libclang"** (`ferrobridge-build/src/lib.rs:148`); there is no forced-include API (M8 finding 3). Flags from a database or file must go to both sides.
- **Test patterns:**
  - CLI tests run the built binary in a temporary directory (`crates/ferrobridge-cli/tests/generate.rs:28`).
  - `ferrobridge-build` tests build a throwaway crate under `target/ferrobridge-build-it` (`crates/ferrobridge-build/tests/generate_mode.rs`).
  - Parser fixtures are `insta` snapshots.

## Decisions (need your approval)
1. **The flags file holds one argument per line**, exactly as passed. There is no quoting and no `@file` syntax; blank lines are ignored.
   - CMake writes it with `file(GENERATE … $<JOIN:…,\n>)`. A define such as `noreturn_function=__attribute__((noreturn))` needs no escaping.
   - Alternative: GCC/clang `@file` quoting. It would match compilers, but CMake would have to quote, and spaces and backslashes become a source of bugs. Not chosen.
2. **`compile_commands.json` is read through libclang** (`clang.rs`), not with `serde_json` plus a shell splitter. It gets clang's own splitting of `"command"` strings and needs no new dependency.
   - The entry is chosen by comparing canonical paths with `--for <source>`.
   - `--compile-commands` accepts the JSON file or its directory.
3. **What the filter keeps, and how it reports.** This refines ADR-0013 decision 3; the ADR is updated in Task 7.
   - **Kept** (as one or two arguments, joined or separate):
     - `-D`, `-U`, `-I`, `-isystem`, `-iquote`, `-idirafter`, `-include`, `-imacros`;
     - `-std=`, `-stdlib=`;
     - `-arch`, `--target=`/`-target`, `--sysroot`, `-isysroot`;
     - `-m32`/`-m64`;
     - flags that change the language: `-f[no-]exceptions`, `-f[no-]rtti`, `-f[un]signed-char`, `-fshort-enums`, `-fshort-wchar`, `-fms-extensions`, `-fms-compatibility`, `-f[no-]char8_t`, `-fpack-struct*`.
   - **Dropped without a message:** the compiler executable, `-c`, `-o <file>`, the source file, `-x <lang>` (ferrobridge sets `-x c++`), and `-MD/-MMD/-MF/-MT/-MQ`.
   - **Dropped and summarized in one `ℹ` line per kind:** warnings (`-W…`, `-w`, `-pedantic`), optimization and debug (`-O…`, `-g…`), and code generation or diagnostics flags not in the keep list (`-f…`). These are known not to change how a header parses.
   - **Dropped with a `⚠` that lists them:** anything else (e.g. `-Xclang …`, `--param`). The user can pass them through `--` on purpose.
   - **Explicit `--` arguments are never filtered.**
   - Why two levels: a PX4 build would otherwise print a `⚠` with ~40 warning flags on every build, and a warning that is always there is a warning nobody reads.
4. **Standard precedence:** an explicit `--std` (or `Build::std`) wins over a `-std=` from a flags source, which wins over the default `c++17`. `--std` becomes optional in clap; its help text names the default.
5. **The cxx glue is opt-in: `ferrobridge generate --cxx-glue`, in the CLI only.**
   - It writes `bridge.rs.cc`, `bridge.rs.h` and `rust/cxx.h` next to the other three files.
   - Each file starts with our `@generated` line naming the ferrobridge and `cxx-gen` versions, so the overwrite check and ADR-0012 cover them.
   - New dependency: `cxx-gen = "0.7.198"` in `ferrobridge-cli`.
   - `ferrobridge-build` keeps `cxx-build` (unchanged Cargo flow).
6. **`ferrobridge-build` gets `Build::flags_file(path)` and `Build::compile_commands(db, source)`** (`generate` feature, since reading a database needs libclang).
   - The kept flags go to **both** libclang and the C++ compiler, so the shim is compiled with the defines it was read with.
   - Directories that come from these sources are not given to Cargo as `rerun-if-changed`. Each header libclang actually read is given instead. This fixes M8's "Cargo rescans ~5,000 PX4 files" for this path.
   - `include_dir` keeps its current behavior.
7. **Out of M9:**
   - the CMake module itself (M10);
   - `restat` / write-if-changed for the CLI (M10, together with the CMake rule);
   - Windows paths in depfiles;
   - changing the `AP_Math` demo to a compilation database (waf would need its own export step; the demo stays as it is and must keep passing).

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Naming | `crates/ferrobridge-parser/src/lib.rs:24`, `crates/ferrobridge-build/src/lib.rs:114-148` | Option structs with doc'd public fields; builder methods named after the thing they add (`include_dir`, `define`, `clang_arg`) |
| Errors | `crates/ferrobridge-parser/src/lib.rs:77` (`ParserError`) | `thiserror` enum, one variant per failure, messages that name the file/argument; the CLI prints `✗ {error}` plus details (`main.rs:365`) |
| User messages | `crates/ferrobridge-cli/src/main.rs:237-263` | `⚠ <what> (<file>:<line>): <reason>` for problems, `ℹ …` for notes, `✓ …` summary on stdout; `cargo:warning=ferrobridge: …` in `build.rs` |
| libclang | `crates/ferrobridge-parser/src/clang.rs:107` (`TranslationUnit::parse`) | Every `unsafe` block in `clang.rs` with `// SAFETY:`; RAII wrapper with `Drop`; strings copied out with `take_string` |
| Tests | `crates/ferrobridge-cli/tests/generate.rs:20-32` | Run the built binary, unique temp `out_dir(test)`, assert on files and stderr text |
| Tests | `crates/ferrobridge-build/tests/generate_mode.rs` | Throwaway crate under `target/ferrobridge-build-it` built with `cargo` |
| Tests | `crates/ferrobridge-build/src/lib.rs:530` | Unit test on `parse_options` for what libclang sees |
| Writes | `crates/ferrobridge-build/src/lib.rs:242` | `write_if_changed` in the build crate; the CLI's `write_files` writes always |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-parser/src/flags.rs` | CREATE | `CompilerFlags`, `filter`, `read_flags_file`, `from_compilation_database`; unit tests |
| `crates/ferrobridge-parser/src/clang.rs` | UPDATE | `CompilationDatabase` wrapper; `TranslationUnit::inclusions()` |
| `crates/ferrobridge-parser/src/lib.rs` | UPDATE | `pub mod flags`; `Parsed { module, inclusions }` and `parse_headers_with_inclusions`; new `ParserError` variants; `parse_headers` unchanged for callers |
| `crates/ferrobridge-parser/tests/flags.rs` | CREATE | Compilation database and inclusion tests against small temp projects |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `--flags-file`, `--compile-commands`/`--for`, `--depfile`, `--cxx-glue`, optional `--std`; filter messages |
| `crates/ferrobridge-cli/src/glue.rs` | CREATE | cxx glue through `cxx-gen`, with our `@generated` line |
| `crates/ferrobridge-cli/Cargo.toml` | UPDATE | `cxx-gen = "0.7.198"`, `proc-macro2` (to tokenize `bridge.rs`) |
| `crates/ferrobridge-cli/tests/generate.rs` | UPDATE | Tests for every new option; a C++ compile of the glue + shim |
| `crates/ferrobridge-build/src/lib.rs` | UPDATE | `Build::flags_file`, `Build::compile_commands`; flags to both sides; `rerun-if-changed` per included header for these sources |
| `crates/ferrobridge-build/tests/generate_mode.rs` | UPDATE | A throwaway crate built from a compilation database |
| `Cargo.lock`, `deny.toml` (only if a new license appears) | UPDATE | New dependency |
| `docs/adr/0013-cmake-integration.md` | UPDATE | Decision 3 refined (keep list, `ℹ`/`⚠` levels), dated amendment |
| `README.md`, `crates/ferrobridge-build/README.md`, `CHANGELOG.md` | UPDATE | New options documented; `[Unreleased]` entries |
| `.claude/prds/ferrobridge.prd.md` + Obsidian `06 PRD` | UPDATE | M9 row → in-progress, then complete |
| `CLAUDE.md`, Obsidian `02`/`03`/`04` | UPDATE | Status line, phase table, log |

## Tasks
Order: TDD per task. Write the failing test first, then the code. Each task ends with fmt, clippy and the crate's tests.

### Task 1: the flag filter
- **Action:**
  - `flags::filter(args, directory) -> CompilerFlags { std: Option<String>, args: Vec<String>, dropped: Dropped }`, implementing decision 3.
  - Handles joined and separate forms (`-Idir`/`-I dir`, `--target=x`/`-target x`).
  - Relative `-I/-isystem/-iquote/-idirafter/-include/-imacros/--sysroot` paths are made absolute against `directory` (`-include visibility.h` stays as it is when the file is not found there: clang then resolves it through the include paths, as the compiler does).
  - `-std=` moves into `std` and is not repeated in `args`.
- **Mirror:** pure functions with table tests, like `crates/ferrobridge-analysis/src/rules.rs`.
- **Validate:** `cargo test -p ferrobridge-parser flags`. The tests cover:
  - PX4's real `geo.cpp` command line from M8, typed into the test: it keeps exactly the 7 defines, 15 includes, `-include visibility.h`, `-arch arm64` and std `gnu++17`, and summarizes the warnings and optimizations;
  - relative paths;
  - unknown flags;
  - two-argument flags at the end of the list (missing value → reported, not a panic).

### Task 2: flag sources
- **Action:**
  - `flags::read_flags_file(path)` implements decision 1, with an error naming the file.
  - `clang.rs` gets a `CompilationDatabase` wrapper (`fromDirectory`, `getAllCompileCommands`, per-command filename/directory/args; `Drop` releases everything).
  - `flags::from_compilation_database(path, source) -> (args, directory)` matches by canonical path, and errors list the 3 closest file names when no entry matches.
  - New `ParserError` variants: `CompilationDatabase { path, message }` and `NoCompileCommand { source, database }`.
- **Mirror:** `TranslationUnit` RAII and `// SAFETY:` comments in `clang.rs`; `ParserError` style.
- **Validate:** `crates/ferrobridge-parser/tests/flags.rs` writes a temporary `compile_commands.json` (one entry with `"arguments"`, one with a quoted `"command"` containing `-DX="a b"` and a relative `-I`) and checks:
  - the parsed arguments;
  - the absolute paths;
  - the directory-or-file argument;
  - the "no entry" error.
  
  It must also pass on libclang 16 (CI `libclang-min`).

### Task 3: inclusions and the depfile
- **Action:**
  - `TranslationUnit::inclusions()` collects the files via `clang_getInclusions`; the in-memory synthetic unit is excluded.
  - `parse_headers_with_inclusions(headers, options) -> Result<Parsed, ParserError>` returns `Parsed { module, inclusions: Vec<PathBuf> }` (sorted, unique). `parse_headers` calls it and returns `module`, so no caller changes.
  - A depfile writer (CLI) produces Make syntax: the target is `<out-dir>/bridge.rs`, and dependencies are every inclusion plus `ferrobridge.toml` if given. Spaces, `#` and `$` are escaped as GCC/Ninja expect.
- **Mirror:** `TranslationUnit::diagnostics` (`clang.rs:197`) for collecting from libclang.
- **Validate:**
  - a parser test with a header that includes a second header, which includes a third: all three are listed and the synthetic unit is not;
  - a depfile unit test with a path containing a space and `$`.

### Task 4: CLI options
- **Action:**
  - `generate`/`parse`/`suggest` accept `--flags-file <path>` (repeatable) and `--compile-commands <path> --for <source>`. Their filtered flags come before the `--` arguments, so explicit ones win.
  - `--std` becomes optional (decision 4).
  - `generate` also accepts `--depfile <path>`, written only after a successful generation.
  - Filter reports are printed to stderr as `ℹ` and `⚠` lines (decision 3).
  - Errors use the existing `✗` style.
- **Mirror:** `crates/ferrobridge-cli/src/main.rs:90` (`impl From<ParseArgs> for ParseOptions` becomes a fallible conversion that reads the sources).
- **Validate:** new tests in `crates/ferrobridge-cli/tests/generate.rs`:
  - a header that only compiles with a define and an include path given only through a flags file;
  - the same through a compilation database with `--for`;
  - `--std` precedence;
  - the `ℹ` summary for warnings and the `⚠` for an unknown flag;
  - the depfile's content.
  
  `generated_files_match_the_committed_e2e_files` stays green, because the defaults are unchanged.

### Task 5: cxx glue
- **Action:**
  - `glue.rs` tokenizes the generated `bridge.rs` text (`proc_macro2::TokenStream::from_str`) and calls `cxx_gen::generate_header_and_cc` with default `Opt`.
  - It writes `bridge.rs.cc`, `bridge.rs.h` and `rust/cxx.h` (`cxx_gen::HEADER`), each with a first line `// @generated by ferrobridge <v> (cxx-gen 0.7.198) — do not edit by hand`.
  - `foreign_files` checks these paths too when `--cxx-glue` is given.
  - The dependency is added; `cargo deny check` must pass.
- **Mirror:** `write_files` and `GENERATED_MARKER` (`main.rs:104`).
- **Validate:**
  - a CLI test generates `classes` with `--cxx-glue` and compiles `shim.cc` and `bridge.rs.cc` with the system C++ compiler (`c++ -std=c++17 -c`, include dirs: out dir and fixture), the same compiler the e2e tests already need;
  - a test that a second run without `--force` does not refuse the glue files it wrote itself.

### Task 6: `ferrobridge-build`
- **Action:**
  - `Build::flags_file(path)` and `Build::compile_commands(db, source)` (`generate` feature) store a flag source. At `compile`, the source is read and filtered: `-I`/`-D`/`-U` become include dirs and defines, `std` follows decision 4, and the other kept flags go to both `cxx_build` (`flag`) and libclang.
  - Filter reports become `cargo:warning=ferrobridge: …` (only the `⚠` level; the `ℹ` summary is printed as a normal build-script line, not a warning).
  - For these sources, `rerun-if-changed` is emitted per inclusion from `parse_headers_with_inclusions`, not per directory.
- **Mirror:** `generate::parse_options` and its unit test (`lib.rs:432`, `:530`); `generate_mode.rs`.
- **Validate:**
  - a unit test that a database's flags reach both libclang and the compiler side;
  - a `generate_mode.rs` test that builds a throwaway crate whose header compiles only with flags from a `compile_commands.json`, runs it, and checks that the build output has `rerun-if-changed` for the included header and not for its directory.

### Task 7: regression, PX4 check, documents
- **Action:**
  - Run the full suite, the `AP_Math` demo (14 tests + `tour`), clippy in both modes, `cargo doc -D warnings`, MSRV check with 1.85 and `cargo deny`.
  - **Manual PX4 check:**
    - `geo` + `mathlib` with `--compile-commands ~/PX4-Autopilot/build/px4_sitl_default --for src/lib/geo/geo.cpp --prelude matrix/math.hpp --config <M8's toml> --cxx-glue --depfile …`, compared with M8's numbers;
    - compile the shim and glue with PX4's own `geo.cpp` command;
    - check that the depfile lists `px4_boardconfig.h`.
  - Documents: README (new options, with a short "flags from your build system" section), `crates/ferrobridge-build/README.md`, CHANGELOG `[Unreleased]`, ADR-0013 amendment, both PRD copies, `CLAUDE.md`, Obsidian notes, and implementation notes in this plan.
  - Review with `/ecc:rust-review`, then `/ecc:security-review` (new `unsafe`, file and path handling).
- **Validate:** the commands below; the PX4 numbers match M8 (92/146) with no flag typed by hand.

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo +1.85 check --workspace --all-targets --all-features
cargo deny check
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
ARDUPILOT_DIR=~/ardupilot cargo run --manifest-path demo/ap_math/Cargo.toml --example tour
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| libclang 16 behaves differently in the compilation database or inclusion APIs | Medium | The parser tests run in CI's `libclang-min` job; the API is old (3.x), only behavior is at risk |
| The keep list misses a flag that changes how headers parse, so libclang sees different code than the compiler | Medium | Unknown flags are never silent (`⚠`), `--` passes anything; the PX4 command line is a test case; the list is in one place |
| `cxx-gen`'s output does not compile together with our shim or needs `Opt` settings | Low | Task 5's compile test; M8's spike already compiled the same glue from `cxxbridge` |
| Version skew between `cxx-gen` in the CLI and the user's `cxx` | Medium | The marker line names both versions; the mismatch test is in M10 (ADR-0013) |
| A depfile listing system headers makes every SDK update regenerate | Low | That is correct behavior (the compiler does the same); generation is fast (M8: under a second for `geo` + `mathlib`) |
| Making `--std` optional changes behavior for existing users | Low | The default stays `c++17` when no flags source is given; covered by the unchanged e2e sync test |
| The filter changes the meaning of existing `--` arguments | Low | `--` arguments are never filtered (decision 3), with a test for it |

## Acceptance
- [ ] Flags file and compilation database accepted by `generate`, `parse`, `suggest` and `ferrobridge-build`
- [ ] Filter implements decision 3; every dropped flag is either summarized (`ℹ`) or listed (`⚠`)
- [ ] Depfile lists every included header; the cxx glue compiles with the shim
- [ ] `ferrobridge-build` sends database flags to both libclang and the compiler, with per-header `rerun-if-changed`
- [ ] PX4 `geo` + `mathlib` reproduces M8's 92/146 from `compile_commands.json` with no hand-copied flags
- [ ] All validation commands pass; the `AP_Math` demo passes; e2e generated files unchanged
- [ ] New `unsafe` only in `clang.rs`, each with `// SAFETY:`; review done
- [ ] ADR-0013 amended; README, CHANGELOG, PRD, `CLAUDE.md`, Obsidian notes updated

## Implementation notes (2026-11-28)

### What was built
- `crates/ferrobridge-parser/src/flags.rs`: `filter`, `CompilerFlags { std, flags: Vec<Flag>, dropped }`, `Flag { kind: FlagKind, args }`, `Dropped::reports() -> Vec<Report>` (`Note`/`Warning`), `read_flags_file`, `from_compilation_database -> CompileCommand`.
- `clang.rs`: `compile_commands()` (RAII guards for the database and the command list) and `TranslationUnit::inclusions()`. `lib.rs`: `Parsed` and `parse_headers_with_inclusions`. New errors: `FlagsFile`, `CompilationDatabase`, `NoCompileCommand`.
- CLI: `--flags-file`, `--compile-commands`/`--for`, optional `--std`, `--depfile`, `--cxx-glue`; new modules `glue.rs` and `depfile.rs`.
- `ferrobridge-build`: `Build::flags_file`, `Build::compile_commands`, `Error::Flags`.
- ADR-0013 got a dated amendment (decision 3 refined).

### Deviations from the plan
1. **Structured kept flags.** The filter returns `Vec<Flag>` with a `FlagKind` instead of a plain `Vec<String>`. `ferrobridge-build` needs the kind to turn `-I`/`-D` into `include`/`define` calls and to keep target flags away from `cc`. `CompilerFlags::clang_args()` gives the flat list.
2. **Target flags go only to libclang in `ferrobridge-build`.** This covers `-arch`, `--target=`, `--sysroot`, `-isysroot`, `-m32/-m64`, `-march=`, … In `build.rs`, `cc` sets the target from Cargo, so a second `-arch` from the database could produce a multi-arch compile. The plan said "the other kept flags go to both".
3. **Keep list, slightly larger.** These were added because they change macros or header search: `-iframework`/`-F`, `--gcc-toolchain=`, `-march=`/`-mcpu=`/`-mfpu=`/`-mfloat-abi=`/`-mthumb`/`-marm`, deployment targets (`-mmacosx-version-min=`), `-f[no-]cxx-exceptions`, `-f[no-]blocks`, `-ffast-math`, `-ffinite-math-only`, `-f[no-]gnu-keywords`, `-pthread`. `-Qunused-arguments` is summarized with the warnings, so PX4 prints no `⚠`. `--driver-mode=` is dropped silently: libclang adds it after the compiler.
4. **Launchers.** A `ccache`/`sccache`/`distcc`/`icecc` first argument is skipped together with the compiler.
5. **A relative `--for` path that does not exist** from the working directory is matched by its trailing path components when exactly one entry ends with them. When several entries match, the error lists them.
6. **CLI tests in their own file**, `crates/ferrobridge-cli/tests/flags.rs`, not in `generate.rs`.
7. **The depfile also lists the `--flags-file` files.** They are inputs of the bridge, like `ferrobridge.toml`.
8. **`cxx-gen` is pinned exactly** (`=0.7.198`), so the version written in the marker line cannot drift.
9. **PX4 counts.** The real `geo.cpp` command has 8 defines (7 + `-DNDEBUG`) and 14 include directories, not 7 and 15. It also has 28 warning flags, 4 optimization/debug flags and 13 code generation flags. The unit test uses the real list.
10. **The `build.rs` test reads the build script's stdout from disk** (`build/<pkg>-<hash>/output` or `build/<pkg>/<hash>/run/stdout`, depending on the Cargo version). Its `build.rs` gets a unique comment line, because Cargo 1.100 judges freshness by content.

### Two generator bugs found by the PX4 check (fixed)
The PX4 bridge had never been compiled through cxx in M8 (only counted). Compiling `bridge.rs.cc` exposed two v0.1 bugs. Both also broke the Cargo path (`cxx-build`):
1. **Explicit class template specializations** (`template <> struct math::is_floating_point<float>`, three of them) were lowered as ordinary classes, so they became three Rust types with the same name. They are now `unsupported` (`class_template_specialization`). Test: `tests/fixtures/templates` (parser snapshot reviewed by hand).
2. **A by-value return with a top-level `const`** (`const T AlphaFilter<T>::update(…)`) was bound directly. cxx spells the member-function pointer type without the `const`, so it does not compile. Such functions now go through the shim (`FunctionPlan::const_return`). Test: `V3<T>::sum` in the `value_types` e2e fixture. Red first, then green; e2e files and codegen snapshots were regenerated and reviewed by hand.

### Manual PX4 check (PX4 v1.18.0, `~/PX4-Autopilot`)
- **Same bridge as before.** `geo` + `mathlib` (15 headers, prelude `matrix/math.hpp`, `[templates]` for `Vector2f` and the six `mathlib` type-parameter templates) was generated twice:
  - with M8's method (flags filtered by a script and copied after `--`, `--std gnu++17`);
  - with `--compile-commands build/px4_sitl_default --for src/lib/geo/geo.cpp`.

  `bridge.rs`, `shim.h` and `shim.cc` are **byte-for-byte identical**, with the same warnings. No flag was typed by hand. The output reports `ℹ` 28 warning, 4 optimization/debug and 13 code generation flags, and no `⚠`.
- **Compiles with PX4's own flags.** `shim.cc` and the `--cxx-glue` `bridge.rs.cc` both compile with PX4's own `geo.cpp` command (`-Wall -Wextra -Werror -Wshadow …`), after the two fixes above.
- **The depfile has 339 dependencies.** Among them: the configure-time `build/px4_sitl_default/px4_boardconfig.h`, the forced `src/include/visibility.h`, `matrix/math.hpp`, every bridge header and the SDK headers.
- **Not reproduced: M8's 92/146.** M8's `ferrobridge.toml` (19 `geo` annotation lines) was kept in an old session scratchpad and is gone. The identical output against M8's flag method is the evidence that the flags are equivalent. The 92/146 number will be measured again with the demo's own `ferrobridge.toml` in M11.
- The commands are in the session scratchpad (`px4/run.sh`) and are not committed, like M8's scripts.

### Review (`/ecc:rust-review`, with the security points: new `unsafe`, paths, argument handling)
- **No CRITICAL or HIGH findings.** The reviewer found these sound:
  - the new `unsafe` blocks (disposal order, null checks, the visitor and `catch_unwind`);
  - the order of the flag prefixes;
  - that no plugin-loading flag (`-Xclang -load`, `-fplugin=`) can pass the filter;
  - that no shell is involved.
- Fixed:
  - **MEDIUM:** `ferrobridge-build` watched `<entry directory>/compile_commands.json` instead of the database itself (Makefile/Bear entries live elsewhere). `CompileCommand::database` is now the canonical database file.
  - **MEDIUM:** the CLI depfile did not list `--compile-commands`. It does now, with a test.
  - **MEDIUM:** some `-f` flags that change macros or keywords were dropped with only a note. These are now kept: `-fopenmp`, `-ffreestanding`, `-fno-builtin`, `-fno-operator-names`, `-f[no-]sized-deallocation`, `-f[no-]aligned-allocation`, `-f[no-]declspec`, `-f[no-]delayed-template-parsing`. `-fmodules*` is reported with a `⚠`.
  - **LOW:** in `ferrobridge-build`, defines go to the compiler as `-D…` arguments in their original order, so a `-DX … -UX` pair keeps its meaning. `--target <v>` is also accepted as two arguments. One dead table entry and a repeated expression were removed.
- Left as they are (LOW), all visible rather than silent:
  - duplicate database entries for one source (the first one is taken);
  - rare separate forms such as `-Xarch_<a> <flag>`, `-L dir`, `-framework X` and `-gcc-toolchain <v>`, which end up in the `⚠` list;
  - Windows backslashes in depfiles (Windows is out of scope);
  - a failed depfile write leaves the generated files in place (the exit code is still non-zero).

### Validation
- fmt, clippy (both modes) and `cargo test --workspace --all-features` pass: **235 tests** (v0.1: 204).
- `cargo doc -D warnings`, `cargo +1.85 check` and `cargo deny` pass.
- `AP_Math` demo: 14 tests and `tour` pass.
- The new parser tests also pass with libclang 16 (Homebrew `llvm@16`).
