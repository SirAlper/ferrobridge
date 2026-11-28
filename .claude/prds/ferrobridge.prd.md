# ferrobridge — Product Requirements Document (PRD)

> First written: 26 November 2026. Updated for v0.2: 28 November 2026 (scope), 28 November 2026 (after the M8 measurement).

## Problem
Developers on teams that have a large C++ code base and want to write new code in Rust have to write bridge code by hand for every C++ function and class. The existing automatic tool is either in maintenance mode or works only with Bazel. Because a C++ header carries no ownership or lifetime information, the developer guesses it. A wrong guess can lead to undefined behavior (UB). If the problem is not solved, teams postpone moving to Rust or live with unsafe, hand-written intermediate layers that are hard to maintain.

**v0.2 addition:** most C++ code bases that would add Rust are built with **CMake**, not Cargo. v0.1 works only from a Cargo `build.rs`: a CMake project has to copy its include paths and defines into `build.rs` by hand, keep them in sync, and wire the Rust crate into the CMake build itself. Headers that depend on files generated during the CMake build (for example message headers) cannot be read at all before that build step has run. So the C++ teams most likely to adopt Rust step by step get the least help from the tool.

## Evidence
**At the ecosystem level (November 2026 research):**
- The Rust Foundation's Rust/C++ Interop Initiative lists 40 interop problems in its September 2026 report. It notes "relatively little progress" on producing solutions outside of external libraries.
- Google is investing in Crubit for Chromium (July 2026). Large organizations have the same problem.
- autocxx is officially in maintenance mode. Crubit describes itself as "not yet viable for general purpose use" and supports only Bazel; **CMake support is not planned**.
- Many companies write their own internal solution on top of bindgen (Rust Foundation report). This points to hand-made, temporary solutions.
- Corrosion is the established way to build Cargo crates from CMake; its own cxx integration (`corrosion_add_cxxbridge`) is still marked experimental and binds only a hand-written bridge. Nothing generates the bridge from headers inside a CMake build.

**From v0.1 (released 2026-11-27):**
- The v0.1 hypothesis held on a real library: about **82%** of ArduPilot `AP_Math` (337 of 412 public items) is callable from Rust with no hand-written bridge and no user `unsafe`, with 59 annotation lines.
- The `AP_Math` demo needed a hand-written `build.rs` that repeats ArduPilot's include paths and defines, because ArduPilot builds with waf. This is the step a CMake integration removes.

**At the user level:**
- Assumption: target developers are assumed to feel this pain and to be willing to use an automatic tool. Ways to validate: (1) scanning autocxx and cxx GitHub issues, (2) reviewing Rust discussions on the ArduPilot/PX4 forums, (3) showing the real-world demo to the community and collecting feedback. The v0.1 announcements are the first step of (3); their feedback feeds the v0.2 plan.

## Users
- **Primary (v0.1):** a Rust developer who needs to use an existing C++ library from Rust and uses **Cargo**. Example domains: robotics/UAVs, embedded systems, game engines, simulation. The need appears when a new Rust module has to reach old C++ code.
- **Primary (v0.2):** a developer on a **CMake** C++ project who wants to add a Rust module (a library or executable target) that calls the project's own C++ code, without leaving CMake as the build entry point. Example: a PX4 developer adding a Rust module that uses a PX4 library, built with PX4's normal `make`/CMake build.
- **Not targeted:**
  - Teams using Bazel (Crubit solves this)
  - People who want to call Rust code from C++ (Zngur solves this)
  - Users of pure C libraries (bindgen is enough)

## Hypothesis
**v0.1 (validated, 2026-11-27):** We believe that **automatically generating a safe Rust API from C++ headers and a small ownership annotation file** will **free Rust developers who use Cargo and have a C++ library** **from writing bridges and `unsafe` by hand**.
We would know we were right when at least 80% of the public API of ArduPilot's `AP_Math` library could be called from Rust with no hand-written bridge and without the user writing any `unsafe`. **Result: about 82%.**

**v0.2:** We believe that **a CMake function that generates the ferrobridge bridge for a CMake target, taking compiler flags from the target itself,** will let **developers on CMake C++ projects** **add a Rust module that calls their C++ code with no hand-written bridge, no `unsafe` and no build glue copied by hand**.
**We will know we are right when:** a Rust module inside a real PX4 build (SITL) calls a PX4 library through a generated bridge, the build is driven only by PX4's CMake, no include path or define is repeated by hand, and the coverage target for that library (set after the PX4 measurement, Milestone 8) is reached.

## Success Metrics
**v0.1 (results):**

| Metric | Target | Result |
|---|---|---|
| `AP_Math` public API coverage | ≥ 80% | ~82% (337/412 items, template members counted once) |
| `unsafe` in user code | 0 | 0 (demo crate scanned) |
| Constructs skipped silently | 0 | 0 (every skipped item has an explanatory warning) |
| Annotation lines needed | Measure | 59 annotation lines for 337 bound items ≈ **18 per 100 API items** |

**v0.2 (targets):**

| Metric | Target | How it is measured |
|---|---|---|
| PX4 library coverage | **≥ 60% of `geo` + `mathlib`** (146 public items), with `geo` fully bound. Set in M8: 0.1.0 reaches 63% with `[templates]` and 19 annotation lines; the rest is mostly function templates (out of scope) | Supported items / public items, template members counted once (method: `.claude/plans/m8-px4-measurement.md`) |
| Flags repeated by hand | 0 include paths or defines in the CMake integration | Read from the CMake target's properties (ADR-0013); checked in the sample project and the PX4 demo |
| CMake lines a user writes | ≤ 10 lines to add one Rust target with one bridge (excluding `ferrobridge.toml`) | Line count in the sample CMake project |
| `unsafe` in user code | 0 | Scan the sample project and the PX4 demo |
| Constructs skipped silently | 0 | Negative tests, carried over from v0.1 |
| No regression | `AP_Math` stays ≥ 82%; the v0.1 Cargo flow keeps working | Demo CI job; existing e2e tests |
| External use after v0.1 | TBD after the announcements | crates.io downloads, GitHub issues/stars, feedback |

## Scope
**Delivered in v0.1:**
- Direction: Rust → C++ (calling C++ from Rust)
- Free functions, classes and methods, enums, namespaces, overloads, constants, `static inline` functions, member operators, class templates instantiated with `[templates]`, value types, several headers in one bridge
- `std::string`, `std::unique_ptr`, `std::vector`, pointer + length as slices
- C++ exceptions come back as `Result`; `[library] exceptions = false`
- Ownership information from `ferrobridge.toml`
- Cargo build integration (`ferrobridge-build`)
- An explanatory warning for every unsupported construct
- An end-to-end demo on `AP_Math`

**v0.2 scope (CMake first):**
- **Flags from the build system, as data:** `ferrobridge generate` reads flags from a response file or from a `compile_commands.json` entry, keeps only the flags libclang needs (with a warning per kind of dropped flag), writes a depfile of the headers it read, and can write the cxx C++ glue itself (ADR-0013).
- **CMake integration (ADR-0013):** `ferrobridge_add_bridge(...)` in `cmake/FerroBridge.cmake` generates the bridge at build time with the target's own flags, compiles the shim and the cxx glue with the project's toolchain, and links them into a Rust crate built through Corrosion. The generated files depend on the headers, `ferrobridge.toml` and every header they included, so they are regenerated when those change.
- **How users get it:** `FetchContent` of this repository at the release tag; the module checks that the `ferrobridge` command has the same version (ADR-0013).
- **Headers that need generated files:** bridge generation is ordered after the build steps that produce the headers it reads (needed for PX4's message headers).
- **A sample CMake project** in the repo, built in CI on Linux and macOS.
- **The PX4 demo:** an out-of-tree PX4 module in `demo/px4/` (loaded with `EXTERNAL_MODULES_LOCATION`, SITL only) whose Rust code uses `geo` and `mathlib` through one generated bridge; outside the main workspace like the `AP_Math` demo, `MIT OR Apache-2.0` (it contains no PX4 code).
- **Fixes for gaps the PX4 measurement finds**, only where they block the demo and fit the existing design; anything bigger needs its own ADR or moves to v0.3.

**Out of scope (v0.2):**
- Bazel support — Crubit already solves it
- Other build systems (Meson, waf, Make without CMake) — only through `compile_commands.json`, no dedicated integration
- PX4 on flight hardware (NuttX and other embedded targets) — v0.2 targets the SITL host build only
- The C++ → Rust direction — left to Zngur; interoperating with it is a later goal
- Function templates, inheritance mapped to traits, callbacks/lambdas — v0.3 and later
- `ferrobridge explain` — later (M8 found no need for it in the CMake integration)
- PX4's `matrix` library as a demo target — needs inheritance mapping and non-type template parameters (M8: 18% reachable)
- Features that work only on nightly Rust — stable Rust only
- Pure C libraries — bindgen is enough
- Guessing the ownership of unannotated raw pointers — the tool does not guess, it asks the user

## Delivery Milestones
| # | Milestone | Outcome | Status | Plan |
|---|---|---|---|---|
| 1 | Reading headers | The user gives a C++ header and the tool shows what it sees inside (functions, types) in a readable form | complete | `.claude/plans/m1-reading-headers.plan.md` |
| 2 | First bridge | A simple C++ class can be called from a Rust test without a hand-written bridge | complete | `.claude/plans/m2-first-bridge.plan.md` |
| 3 | Ownership annotations | The user writes annotations for ambiguous pointers; in an ambiguous case the tool asks instead of generating code | complete | `.claude/plans/m3-ownership-annotations.plan.md` |
| 4 | Safe API | The user can create and use a C++ object without writing any `unsafe`; exceptions arrive as `Result` | complete | `.claude/plans/m4-safe-api.plan.md` |
| 5 | Build pipeline and demo skeleton | `build.rs` integration with `ferrobridge-build`; ≥ 85% of `AP_Math`'s non-template API (≥ 30% overall) usable from Rust against a real ArduPilot checkout | complete | `.claude/plans/m5-build-pipeline-and-demo.plan.md` |
| 6 | Cross-header types, templates and fields | ≥ 80% of the `AP_Math` API usable from Rust (including `Vector3f`, `Matrix3f`, `Quaternion`). A type defined in one header can be used in functions in other headers too. Template instantiation enters the MVP in this milestone, with an ADR | complete | `.claude/plans/m6-templates-and-value-types.plan.md` |
| 7 | v0.1 release | The tool is on crates.io; README, examples and demo are public | complete | `.claude/plans/m7-v0.1-release.plan.md` |
| 8 | PX4 measurement and CMake design | A PX4 library is chosen and measured with the current tool (coverage, blockers, how its headers get their flags and generated files); the v0.2 coverage target is set; an ADR decides the CMake integration design | complete | `.claude/plans/m8-px4-measurement-and-cmake-design.plan.md` |
| 9 | Flags as data | `ferrobridge generate` takes flags from a response file or a `compile_commands.json` entry, filters what libclang does not need (with a warning per kind), writes a depfile and the cxx C++ glue; the `AP_Math` demo and e2e tests keep passing | complete | `.claude/plans/m9-flags-as-data.plan.md` |
| 10 | CMake integration | With `ferrobridge_add_bridge()` and Corrosion, a sample CMake project adds a Rust target that calls a C++ library through a generated bridge, including a header generated during the build; ≤ 10 CMake lines; it builds in CI on Linux and macOS and regenerates when a header (also an indirectly included one), a flag or `ferrobridge.toml` changes | pending | - |
| 11 | PX4 demo | An out-of-tree PX4 SITL module (`demo/px4/`) calls `geo` and `mathlib` from Rust through one bridge; ≥ 60% of their 146 public items bound, `geo` fully; no flag is repeated by hand | pending | - |
| 12 | v0.2 release | v0.2.0 on crates.io with the CMake module documented; CHANGELOG, README and the PX4 demo are public | pending | - |

## Open Questions
**v0.2:**
- [x] Which PX4 library is the target? **`geo` + `mathlib` as one bridge**: 63% reachable with 0.1.0, `geo` is a real compiled CMake target, and `mathlib` needs a header generated by PX4's build. `matrix` reaches only 18% (inheritance, `size_t` template parameters). M8 measurement, 2026-11-28.
- [x] CMake integration design: **build time** (`add_custom_command` with a depfile); the bridge C++ is compiled by CMake; Corrosion's stable `corrosion_import_crate` only; flags from target properties through a response file; users get the module with `FetchContent`. ADR-0013 (accepted 2026-11-28).
- [x] Minimum versions: **CMake 3.22**, **Corrosion ≥ 0.5** (tested with `v0.5.2`). ADR-0013.
- [x] PX4 demo: **`demo/px4/`**, an out-of-tree PX4 module against an external checkout, generated files only in the build directory, license `MIT OR Apache-2.0`. ADR-0013.
- [x] `ferrobridge explain` is **not** in v0.2: the measurement and the spike needed no more than the existing warnings (2026-11-28).

**Resolved:**
- [x] Code compiled with exceptions disabled (`-fno-exceptions`): the `[library] exceptions = false` promise in `ferrobridge.toml`; functions return values directly, and if the promise is broken the program ends with `std::terminate` (not UB). ferrobridge's own checks stay `Result`. Decided 2026-11-27, ADR-0011.
- [x] Where ownership annotations live: **a separate `ferrobridge.toml` file** (third-party headers cannot be edited); a C++ attribute in the header may be an extra source later. Decided 2026-11-27, ADR-0006.
- [x] Minimum libclang version: **16** (the function needed to detect `= delete` arrived in 16). Decided 2026-11-27, Milestone 2 plan; checked in CI since v0.1.
- [x] Project name: **ferrobridge** is kept; all seven crate names were free on crates.io (checked 2026-11-27, M7 plan).
- [x] License: **MIT OR Apache-2.0**, the project is fully open source (decided 2026-11-26, ADR-0003)

## Risks
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Ownership is mapped wrongly, causing UB | Medium | Critical | If ambiguous, don't generate code, ask the user; the most restrictive mapping by default |
| Scope grows, the release slips | High | High | The out-of-scope list; an ADR is required for every feature outside the scope |
| Crubit adds Cargo support, closing the gap | Medium | High | Quarterly competitor check; focus on CMake, the annotation file and explanatory diagnostics as differences |
| Differences between libclang versions break compatibility | High | Medium | Test with several LLVM versions (16 in CI); isolate the header reading layer |
| The PX4 build is too large or too special to integrate with (its own CMake wrappers, generated headers, many build configurations) | Medium (M8: SITL builds unchanged with CMake 4.5; out-of-tree modules are supported) | High | Prove the CMake module on a small sample project before PX4 (M10); SITL only; pin a PX4 tag |
| The chosen PX4 library needs C++ features ferrobridge does not support (e.g. non-type template parameters) | Low (M8 chose `geo` + `mathlib` for this reason) | High | The target (≥ 60%) is reachable with 0.1.0's features; anything more needs an ADR or moves to v0.3 |
| Version skew between the cxx glue ferrobridge writes (`cxx-gen`) and the `cxx` crate in the user's lockfile | Medium | High | Pin the `cxx` range in the `ferrobridge` runtime crate; test a mismatched patch version in M10 (ADR-0013) |
| Several Rust modules in one binary (e.g. PX4's `px4`) as separate `staticlib`s duplicate the Rust standard library | High if users try it | Medium | Document "one Rust crate per final binary"; the demo uses one crate (ADR-0013) |
| Corrosion changes or its cxx support stays experimental | Medium | Medium | Depend only on Corrosion's stable API (`corrosion_import_crate`); pin a tested version |
| Compiler flags read from CMake do not match what libclang needs (e.g. GCC-only flags) | Medium | Medium | Filter or translate flags in one place, with a warning for every flag that is dropped |
| The assumption about user demand turns out wrong | Medium | High | Collect feedback from the v0.1 announcements before Milestone 10 |
| Writing the annotation file is too much burden for users | Medium | Medium | Measure annotation lines on PX4 as on `AP_Math`; strengthen the inference rules |

---
*Status: DRAFT — requirements only. Implementation plans are made with `/ecc:plan`.*
