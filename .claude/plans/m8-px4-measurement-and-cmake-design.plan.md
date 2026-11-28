# Plan: Milestone 8: PX4 measurement and CMake design

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 8: PX4 measurement and CMake design
**Inputs**:
- `.claude/plans/m5-apmath-measurement.md` (the measurement method this milestone repeats on PX4)
- `demo/ap_math/build.rs` (the hand-copied flags that v0.2 removes)
- `crates/ferrobridge-build/src/lib.rs`, `crates/ferrobridge-cli/src/main.rs` (how flags reach libclang today)
**Complexity**: Medium (almost no code in the repo; the risk is in the PX4 build and in getting the design right)
**Status**: Complete (2026-11-28); ADR-0013 accepted the same day

## Summary
v0.2 has two unknowns that decide everything after it:
1. **Which PX4 library, and how far today's tool gets on it.** Without a number, the v0.2 coverage target (PRD M11) would be a guess.
2. **How the CMake integration is shaped.** There are real alternatives: generating at configure time or at build time, and compiling the C++ glue in Cargo or in CMake. They lead to different M9–M10 work.

M8 answers both, the same way M5 started with a measurement:
- **A measurement report:** `.claude/plans/m8-px4-measurement.md`, with numbers and symbol names only; no PX4 code is copied.
- **A throwaway CMake spike** outside the repo, to test the design options on a real CMake + Corrosion build.
- **ADR-0013:** the CMake integration design.
- **A PRD update:** the chosen library, the v0.2 coverage target, M9–M11 outcomes made concrete, and the open questions closed.

**No ferrobridge code changes in M8.** Gaps the measurement finds become scope for M9–M11 or an ADR; they are not fixed here.

**Measurable goal (PRD):** "A PX4 library is chosen and measured with the current tool (coverage, blockers, how its headers get their flags and generated files); the v0.2 coverage target is set; an ADR decides the CMake integration design."

## Findings while grounding the plan
- **No PX4 checkout on this machine.** The latest stable release is `v1.18.0` (from `git ls-remote --tags`).
- **Tools on this machine:** CMake **4.5.1**, Ninja 1.13.2, Python 3.15.2, Homebrew `llvm@23` and `llvm@16`.
  - There is no Docker or Podman, so there is no local Linux fallback.
  - CMake 4 removed compatibility with `cmake_minimum_required` below 3.5. Older submodules of PX4 may fail to configure because of this. This is the first thing Task 1 checks.
- **Today flags reach libclang in three ways, all set by hand:**
  - `Build::include_dir` / `define` / `std` / `flag` (`crates/ferrobridge-build/src/lib.rs:114-148`)
  - `Generate::clang_arg` (`:331`)
  - the CLI's `-- <clang args>` (`crates/ferrobridge-cli/src/main.rs:88`)
- **The `AP_Math` demo is the problem in miniature.** `demo/ap_math/build.rs:9` keeps a `DEFINES` table copied from what waf passes, plus a `stubs/hwdef.h` for a file that waf generates.
- **`ferrobridge-build` compiles the shim and the cxx glue with `cc` inside Cargo.** The "Cargo compiles the C++" design option therefore already exists. Its weakness under CMake is that the C++ is then not built with the CMake project's toolchain and flags.
- **Corrosion:**
  - Version 0.5 needs CMake ≥ 3.15; its master branch needs ≥ 3.22.
  - `corrosion_add_cxxbridge` is experimental and binds a hand-written `#[cxx::bridge]`, not headers.
- **PX4 is BSD-3-Clause**, so ADR-0008's GPL constraints do not apply. Even so, we keep PX4 code out of this repo by default, the same way as ArduPilot (decision 4).

## Decisions (need your approval)
1. **Pin PX4 `v1.18.0`**, cloned outside the repository: shallow, with only the submodules SITL needs.
   - The path goes into `CLAUDE.local.md`, not the repo.
   - Download: `github.com/PX4/PX4-Autopilot`, about a few hundred MB with the SITL submodules. I state the real size before the submodule step, and nothing else is downloaded without telling you.
   - Alternative: `main`. More current, but the numbers would move under us; not chosen.
2. **Measure three candidates, choose one:** `src/lib/matrix` (header-only templates), `src/lib/mathlib`, `src/lib/geo`.
   - Selection rule, in this order:
     - (a) the library is actually used by PX4 modules a Rust module would sit next to;
     - (b) the reachable coverage with work that fits v0.2;
     - (c) it needs generated headers or real PX4 build flags, so the demo proves the CMake value and not just another `AP_Math`.
   - A library that reaches high coverage but needs none of PX4's build (c) is a weaker demo, and the report says so.
3. **Measure on the real SITL build's flags.** `make px4_sitl_default` runs once; flags come from its `compile_commands.json`. For now they are extracted by a small script outside the repo, which is exactly what M9 automates.
   - If SITL does not build on macOS with CMake 4.5 and the problem cannot be worked around with a documented setting (e.g. `CMAKE_POLICY_VERSION_MINIMUM=3.5`), I stop and ask you. The fallback would be a temporary Linux CI job on this branch, which means pushing.
4. **The spike is throwaway**, in the scratchpad, never committed.
   - Only the report and the ADR enter the repo; M10 builds the real module with TDD.
   - The spike uses the published `ferrobridge` 0.1.0 CLI unchanged, plus Corrosion fetched with `FetchContent` at a pinned tag (a second download: `github.com/corrosion-rs/corrosion`, small).
5. **ADR-0013 is written as `proposed`** with the options and a recommendation. It becomes `accepted` only after you read it, because it fixes the v0.2 architecture.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Measurement report | `.claude/plans/m5-apmath-measurement.md` | Goal → source (pinned commit, nothing copied) → summary → method (exact clang args) → numbers → reasons table → cumulative scenarios → findings |
| Naming | `.claude/plans/m5-apmath-measurement.md`, `m5-build-pipeline-and-demo.plan.md` | `mN-<topic>.md` for a report, `mN-<topic>.plan.md` for a plan |
| ADR | `docs/adr/template.md`, `docs/adr/0008-build-time-generation-for-gpl-demo.md` | Context → decision → alternatives (pros/cons/why not) → consequences; a row in `docs/adr/README.md`; in Obsidian, a row in the ADR table of `03 Mimari` |
| Flags today | `demo/ap_math/build.rs:9` | Every define is commented with where it comes from: the same provenance is recorded in the report |
| Plan closing | `.claude/plans/m7-v0.1-release.plan.md` (end) | Implementation notes per task, deviations marked as **Deviation** / **Finding** |
| Errors / logging / tests | n/a | No code changes in M8; the existing suite must stay green |

## Files to Change
| File | Action | Why |
|---|---|---|
| `.claude/plans/m8-px4-measurement-and-cmake-design.plan.md` | CREATE | This plan (implementation notes added at the end) |
| `.claude/plans/m8-px4-measurement.md` | CREATE | The measurement report |
| `docs/adr/0013-cmake-integration.md` | CREATE | The CMake integration design |
| `docs/adr/README.md` | UPDATE | Index row for ADR-0013 |
| `.claude/prds/ferrobridge.prd.md` + Obsidian `06 PRD` | UPDATE | M8 row → in-progress, then complete; the chosen library, the coverage target, concrete M9–M11 outcomes, open questions closed |
| `CLAUDE.md` | UPDATE | Status line: M8 done, next M9 |
| Obsidian `03 Mimari`, `02 Planlama`, `04 Geliştirme` | UPDATE | ADR row, phase table, progress log (not in the repo) |
| `CLAUDE.local.md` | UPDATE | The PX4 checkout path (not in Git) |

## Tasks
### Task 1: PX4 checkout and SITL build
- **Action:**
  - Shallow clone of `v1.18.0` outside the repo, then the submodules SITL needs.
  - Run `make px4_sitl_default` (no simulator run, build only).
  - Record: the build time; `compile_commands.json` (where it is, whether it covers the candidate libraries); which headers are generated and by which CMake targets (uORB topics, `px4_boardconfig.h` and the like); the minimum CMake version PX4 declares; any CMake 4.5 problem.
- **Mirror:** M5 Task 0 (SITL defines recorded with their origin).
- **Validate:** the build ends successfully and the `compile_commands.json` entry of one `src/lib/geo` source file has been located. On failure → decision 3.

### Task 2: measure the three candidate libraries
- **Action:**
  - Run `ferrobridge parse` and `ferrobridge generate` (0.1.0, unchanged) for every public header of `matrix`, `mathlib` and `geo`, with the flags of a translation unit that includes them.
  - Count bound, skipped and reason for each item, the same way as in M5.
  - Template members are counted once per template, with a rough script outside the repo. The report states the error margin.
  - Note per library:
    - non-type template parameters (`Matrix<Type, M, N>`);
    - inheritance between templates (`Vector3<T>` → `Vector<T,3>` → `Matrix<T,3,1>`);
    - generated headers it includes;
    - what PX4 modules include it.
- **Mirror:** `m5-apmath-measurement.md` "Method", "Numbers", "Reasons", "Cumulative scenarios".
- **Validate:**
  - Every number in the report can be reproduced with the commands in its "Method" section.
  - Skipped + bound = the denominator for each library.
  - Every skipped item has a reason, and no reason is "unknown" without a note.

### Task 3: how a Rust module fits into a PX4 module
- **Action:**
  - Read `cmake/px4_add_module.cmake` and one real module.
  - Answer: can a SITL module link a Rust `staticlib` built by Corrosion (link order, `-fno-exceptions`/`-fno-rtti` flags PX4 uses, the C++ standard, how `main`/work queues are entered)?
  - Check whether PX4 already has any Rust support in-tree.
  - No code in PX4, only notes.
- **Validate:** the report has a "PX4 integration" section answering each question, with the file and line it is based on.

### Task 4: CMake spike (throwaway)
- **Action:**
  - In the scratchpad: a tiny C++ library target, a Rust crate, Corrosion at a pinned tag, and the 0.1.0 `ferrobridge` CLI.
  - Try two shapes end to end:
    - **(A) Cargo compiles the C++.** CMake hands the target's flags to the crate's `build.rs` (an environment variable or a generated file); `ferrobridge-build` generates and compiles as today.
    - **(B) CMake compiles the C++.** CMake runs `ferrobridge generate` in an `add_custom_command` at build time, with flags from generator expressions (`$<TARGET_PROPERTY:tgt,INCLUDE_DIRECTORIES>`, `COMPILE_DEFINITIONS`). CMake builds `shim.cc` and the cxx glue as a C++ target with the project's toolchain; the crate only includes `bridge.rs`.
  - For each, record:
    - the lines a user writes;
    - whether a header edit triggers regeneration;
    - whether flags are repeated anywhere;
    - how generated headers are ordered before generation;
    - the build on macOS.
  - Also check whether the flags of a CMake target can be read without `compile_commands.json`. If yes, M9's role changes: it would then serve non-CMake build systems, and the ADR says so.
- **Validate:** both shapes build and run a call into C++ from Rust, or the report says exactly where one of them broke.

### Task 5: ADR-0013 and the coverage target
- **Action:**
  - Write ADR-0013 (`proposed`). It covers:
    - the chosen shape and why;
    - configure time vs build time;
    - the relation to Corrosion: which API, which version is pinned and tested;
    - how users get the module (`find_package` of an installed config, `FetchContent`, or `ferrobridge` writing the module file);
    - the minimum CMake and Corrosion versions;
    - flag filtering for flags libclang does not accept, with a warning per dropped flag;
    - where the PX4 demo lives.
  - Choose the library with decision 2's rule, and set the v0.2 coverage target from the cumulative scenarios: what fits M9–M11 without a new ADR.
- **Validate:**
  - The ADR answers every v0.2 open question in the PRD, or says why one stays open.
  - The target is stated as "≥ N% of `<library>`" with its denominator.

### Task 6: documents
- **Action:**
  - PRD (both copies): M8 complete, M9–M11 outcomes made concrete (e.g. M9's role after Task 4), open questions ticked with dates, new risks from the measurement.
  - `CLAUDE.md` status line, `docs/adr/README.md`.
  - Obsidian `03 Mimari` (ADR table), `02 Planlama` (phase table), `04 Geliştirme` (log row).
  - Implementation notes at the end of this plan.
- **Validate:** the two PRD copies say the same thing; `cargo test --workspace` still passes (no code changed).

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo run -p ferrobridge-cli -- doctor
git status --short            # only docs/plans/ADR changed; no generated or PX4 files
cargo fmt --all --check
cargo test --workspace        # unchanged, sanity only
```
The measurement itself is validated by reproducing the numbers from the report's "Method" section, as in M5.

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| PX4 SITL does not build on macOS with CMake 4.5 | Medium | Try the documented compatibility setting first; otherwise stop and ask (decision 3) |
| The PX4 checkout and its submodules are large or slow to fetch | Medium | Shallow clone, SITL submodules only; report the size before fetching submodules |
| All three candidates are dominated by features outside v0.2 (non-type template parameters, inheritance) | Medium | The report gives the cumulative scenarios; then the target is set on what fits, or an ADR proposes the missing feature. You decide before M9 |
| The spike leaks into the repo as half-finished code | Low | Scratchpad only; `git status` check in validation |
| ADR-0013 is decided too early, before PX4's real constraints are known | Medium | The ADR is written after Tasks 1–4, as `proposed`, and accepted only by you |
| A library that needs no PX4 build flags is chosen, so the demo proves little about CMake | Medium | Selection criterion (c) in decision 2, stated in the report |

## Acceptance
- [ ] PX4 `v1.18.0` SITL builds locally (or a decided fallback); the generated headers and the flags of the candidates are recorded
- [ ] The measurement report exists with reproducible numbers for `matrix`, `mathlib`, `geo`, and a chosen library
- [ ] Both CMake shapes are tried in a throwaway spike; the results are in the report
- [ ] ADR-0013 is written (`proposed`, then `accepted` by you)
- [ ] The v0.2 coverage target is set in the PRD; the v0.2 open questions are answered or deliberately left open
- [ ] No PX4 code and no spike code in the repo; the test suite is unchanged and green
- [ ] Obsidian notes updated (`02`, `03`, `04`, `06`)

## Implementation notes (2026-11-28)

### Task 1: PX4 checkout and SITL build
- `~/PX4-Autopilot`, tag `v1.18.0` (`935addf`): 667 MB as a shallow clone without submodules.
- PX4's CMake fetched the five SITL submodules it needs by itself (mavlink, GPS drivers, libevents, heatshrink, Micro-XRCE-DDS-Client; repositories ~21 MB). The checkout is 965 MB after the build.
- Python: only the build packages from `Tools/setup/requirements.txt` (empy < 4, jinja2, pyyaml, jsonschema, kconfiglib, pyros-genmsg, …) in a venv inside the PX4 checkout. `Tools/setup/macos.sh` was not run.
- **`make px4_sitl_default` passed with CMake 4.5.1 and no compatibility setting** (1163 steps, 68 s wall time). Decision 3's fallback was not needed.
- `compile_commands.json` has 911 entries.

### Task 2: measurement
- Report: `.claude/plans/m8-px4-measurement.md`.
- **Deviation:** the denominator was counted with libclang's Python bindings (Homebrew LLVM 23) instead of a regex script; this is closer to the compiler's view than M5's.
- **Finding:** four `matrix`/`mathlib` headers need `matrix/math.hpp` first (`--prelude`), and counting `matrix` per header hit the main-file `#pragma once` limit known from M5. It is counted through `math.hpp` as one translation unit.
- The `geo` annotations were kept only after reading `geo.cpp`: every output is only written.

### Task 3: PX4 integration
- Notes are in the report ("PX4 build and integration").
- **Finding:** PX4 supports out-of-tree modules (`EXTERNAL_MODULES_LOCATION`), which decides where the demo lives.

### Task 4: CMake spike
- In the session scratchpad, not committed: Corrosion `v0.5.2` via `FetchContent`, plus `cxxbridge-cmd 1.0.205` for shape B. This second tool was not listed in the plan; it is a small crate from crates.io, and I said so before installing it.
- Both shapes built and ran on the first try; the incremental checks are in the report.
- **Not tried:** the Makefile generator, Linux, and a mismatched `cxxbridge`/`cxx` version.

### Task 5: ADR-0013 and the target
- ADR-0013 was written as `proposed` and accepted by the project owner the same day. It decides shape B, build-time generation, `FetchContent`, CMake ≥ 3.22, Corrosion ≥ 0.5, and the demo as an out-of-tree module in `demo/px4/`.
- **Target:** ≥ 60% of `geo` + `mathlib` (146 items) with `geo` fully bound. 0.1.0 reaches 63% today; adding function template instantiation would reach ~100%, but it is out of the PRD's v0.2 scope.
- **Changes M9's role:** CMake does not need `compile_commands.json`. M9 becomes "flags as data" (response file, compilation database, filter, depfile, cxx glue), all of which M10 needs.

### Task 6: documents
- Both PRD copies, `CLAUDE.md`, `docs/adr/README.md`, `CLAUDE.local.md` (checkout path).
- Obsidian: `02 Planlama` (phases 8–12), `03 Mimari` (ADR-0013 row), `04 Geliştirme` (log).
- No code changed; `git status` shows only documents.
