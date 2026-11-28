# ferrobridge — Product Requirements Document (PRD)

> First written: 26 November 2026. Updated for v0.2: 28 November 2026.

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
| PX4 library coverage | Set in Milestone 8 from the measurement (as for `AP_Math` in M5) | Supported items / public items of the chosen PX4 library (the tool's report + manual check) |
| Flags repeated by hand | 0 include paths or defines in the CMake integration | Read from the CMake target or `compile_commands.json`; checked in the sample project and the PX4 demo |
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
- **Flags from the build system:** the CLI and `ferrobridge-build` can take include paths, defines and the language standard from a `compile_commands.json` entry instead of repeating them by hand.
- **CMake integration:** a CMake module (working name `ferrobridge_add_bridge(...)`) that, for a given C++ target and headers, generates the bridge at build time with the target's own flags, builds the shim, and links it into a Rust target built through Corrosion. The generated files depend on the headers and on `ferrobridge.toml`, so they are regenerated when those change.
- **How users get it:** a documented way to bring the CMake module into a project (the exact form, e.g. `find_package` or `FetchContent`, is decided in an ADR).
- **Headers that need generated files:** bridge generation is ordered after the build steps that produce the headers it reads (needed for PX4's message headers).
- **A sample CMake project** in the repo, built in CI on Linux and macOS.
- **The PX4 demo:** a Rust module in a PX4 SITL build that uses a PX4 library through a generated bridge, kept outside the main workspace like the `AP_Math` demo.
- **Fixes for gaps the PX4 measurement finds**, only where they block the demo and fit the existing design; anything bigger needs its own ADR or moves to v0.3.

**Out of scope (v0.2):**
- Bazel support — Crubit already solves it
- Other build systems (Meson, waf, Make without CMake) — only through `compile_commands.json`, no dedicated integration
- PX4 on flight hardware (NuttX and other embedded targets) — v0.2 targets the SITL host build only
- The C++ → Rust direction — left to Zngur; interoperating with it is a later goal
- Function templates, inheritance mapped to traits, callbacks/lambdas — v0.3 and later
- `ferrobridge explain` — later, unless the PX4 work shows it is needed to debug CMake builds (open question)
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
| 8 | PX4 measurement and CMake design | A PX4 library is chosen and measured with the current tool (coverage, blockers, how its headers get their flags and generated files); the v0.2 coverage target is set; an ADR decides the CMake integration design | pending | - |
| 9 | Flags from `compile_commands.json` | The CLI and `ferrobridge-build` read include paths, defines and the standard from a compilation database; the `AP_Math` demo and e2e tests keep passing | pending | - |
| 10 | CMake integration | A sample CMake project adds a Rust target that calls a C++ library through a generated bridge with the CMake module and Corrosion; it builds in CI on Linux and macOS and regenerates when a header or `ferrobridge.toml` changes | pending | - |
| 11 | PX4 demo | A Rust module in a PX4 SITL build calls the chosen PX4 library; the coverage target from Milestone 8 is reached; no flag is repeated by hand | pending | - |
| 12 | v0.2 release | v0.2.0 on crates.io with the CMake module documented; CHANGELOG, README and the PX4 demo are public | pending | - |

## Open Questions
**v0.2:**
- [ ] Which PX4 library is the target? Candidates: `src/lib/matrix` (header-only templates), `mathlib`, `geo`. Decided in Milestone 8 by measurement, like `AP_Math` in M5.
- [ ] CMake integration design: generate at configure time or at build time (custom command); how it builds on Corrosion (`corrosion_import_crate`, its experimental cxx support, or our own target); how users get the module (`find_package`, `FetchContent`, or a command that writes it). Decided in an ADR in Milestone 8.
- [ ] Minimum CMake and Corrosion versions (Corrosion 0.5 needs CMake 3.15; its master branch needs 3.22).
- [ ] Where the PX4 demo lives and what its license is. PX4 is BSD-3-Clause, so the GPL constraints of ADR-0008 do not apply, but generated files from a third-party code base may still stay out of the repo.
- [ ] Does `ferrobridge explain` enter v0.2? Only if debugging the CMake integration needs it.

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
| The PX4 build is too large or too special to integrate with (its own CMake wrappers, generated headers, many build configurations) | High | High | Measure first (Milestone 8); prove the CMake module on a small sample project before PX4; SITL only |
| The chosen PX4 library needs C++ features ferrobridge does not support (e.g. non-type template parameters) | Medium | High | The measurement decides the library and the target; gaps that need a new design get an ADR or move to v0.3 |
| Corrosion changes or its cxx support stays experimental | Medium | Medium | Depend only on Corrosion's stable API (`corrosion_import_crate`); pin a tested version |
| Compiler flags read from CMake do not match what libclang needs (e.g. GCC-only flags) | Medium | Medium | Filter or translate flags in one place, with a warning for every flag that is dropped |
| The assumption about user demand turns out wrong | Medium | High | Collect feedback from the v0.1 announcements before Milestone 10 |
| Writing the annotation file is too much burden for users | Medium | Medium | Measure annotation lines on PX4 as on `AP_Math`; strengthen the inference rules |

---
*Status: DRAFT — requirements only. Implementation plans are made with `/ecc:plan`.*
