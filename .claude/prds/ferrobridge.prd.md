# ferrobridge — Product Requirements Document (PRD)

> Date: 26 November 2026

## Problem
Developers on teams that have a large C++ code base and want to write new code in Rust have to write bridge code by hand for every C++ function and class. The existing automatic tool is either in maintenance mode or works only with Bazel. Because a C++ header carries no ownership or lifetime information, the developer guesses it. A wrong guess can lead to undefined behavior (UB). If the problem is not solved, teams postpone moving to Rust or live with unsafe, hand-written intermediate layers that are hard to maintain.

## Evidence
**At the ecosystem level (November 2026 research):**
- The Rust Foundation's Rust/C++ Interop Initiative lists 40 interop problems in its September 2026 report. It notes "relatively little progress" on producing solutions outside of external libraries.
- Google is investing in Crubit for Chromium (July 2026). Large organizations have the same problem.
- autocxx is officially in maintenance mode. Crubit describes itself as "not yet viable for general purpose use" and supports only Bazel; CMake support is not planned.
- Many companies write their own internal solution on top of bindgen (Rust Foundation report). This points to hand-made, temporary solutions.

**At the user level:**
- Assumption: target developers are assumed to feel this pain and to be willing to use an automatic tool. Ways to validate: (1) scanning autocxx and cxx GitHub issues, (2) reviewing Rust discussions on the ArduPilot/PX4 forums, (3) showing the real-world demo to the community and collecting feedback.

## Users
- **Primary:** a Rust developer who needs to use an existing C++ library from Rust and uses **Cargo** (later CMake). Example domains: robotics/UAVs, embedded systems, game engines, simulation. The need appears when a new Rust module has to reach old C++ code.
- **Not targeted:**
  - Teams using Bazel (Crubit solves this)
  - People who want to call Rust code from C++ (Zngur solves this)
  - Users of pure C libraries (bindgen is enough)

## Hypothesis
We believe that **automatically generating a safe Rust API from C++ headers and a small ownership annotation file** will **free Rust developers who use Cargo and have a C++ library** **from writing bridges and `unsafe` by hand**.
**We will know we are right when:** at least **80%** of the public API of ArduPilot's `AP_Math` library can be called from Rust with no hand-written bridge and without the user writing any `unsafe`.

## Success Metrics
| Metric | Target | How it is measured |
|---|---|---|
| `AP_Math` public API coverage | ≥ 80% | Supported symbols / total public symbols (the tool's own report + manual check) |
| `unsafe` in user code | 0 | Scan the demo crate for `unsafe` |
| Constructs skipped silently | 0 (100% reported with an explanatory warning) | Negative tests for unsupported constructs |
| Annotation lines needed | TBD — measured in the first prototype (lines per 100 API items) | `ferrobridge.toml` line count / number of generated items |
| External use after v0.1 | TBD — decided after the release | crates.io downloads, GitHub issues/stars, feedback |

## Scope
**MVP:**
- Direction: Rust → C++ (calling C++ from Rust)
- Supported constructs: free functions, classes and methods, enums
- Supported standard types: `std::string`, `std::unique_ptr` (Milestone 2); `std::vector` (Milestone 3, 2026-11-27)
- C++ exceptions come back as `Result` in Rust
- Ownership information is taken from an annotation file the user writes
- Cargo build integration
- An explanatory warning for every unsupported construct
- An end-to-end demo on `AP_Math`

**Out of scope:**
- Bazel support — Crubit already solves it
- The C++ → Rust direction — left to Zngur; interoperating with it is a later goal
- General templates, inheritance/virtual, operator overloading, callbacks/lambdas — postponed to v0.2 and later (2026-11-27: after the `AP_Math` measurement, template instantiation and public fields were to be reconsidered in M6; see `.claude/plans/m5-apmath-measurement.md`)
- CMake integration — the first goal of v0.2 (tested with PX4)
- Features that work only on nightly Rust — the MVP must work on stable Rust
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
| 6 | Cross-header types, templates and fields | ≥ 80% of the `AP_Math` API usable from Rust (including `Vector3f`, `Matrix3f`, `Quaternion`). A type defined in one header can be used in functions in other headers too (e.g. `Vector2f` from `vector2.h` in `control.h`). Template instantiation enters the MVP in this milestone, with an ADR | complete | `.claude/plans/m6-templates-and-value-types.plan.md` |
| 7 | v0.1 release | The tool is on crates.io; README, examples and demo are public | pending | — |

## Open Questions
- [x] Code compiled with exceptions disabled (`-fno-exceptions`): the `[library] exceptions = false` promise in `ferrobridge.toml`; functions return values directly, and if the promise is broken the program ends with `std::terminate` (not UB). ferrobridge's own checks stay `Result`. Decided 2026-11-27, ADR-0011.
- [x] Where ownership annotations live: **a separate `ferrobridge.toml` file** (third-party headers cannot be edited); a C++ attribute in the header may be an extra source in v0.2. Decided 2026-11-27, ADR-0006.
- [x] Minimum libclang version: **16** (the function needed to detect `= delete` arrived in 16; CI will test 21–23). Decided 2026-11-27, Milestone 2 plan.
- [ ] Will the project name be finalized? (check availability on crates.io)
- [x] License: **MIT OR Apache-2.0**, the project is fully open source (decided 2026-11-26, ADR-0003)

## Risks
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Ownership is mapped wrongly, causing UB | Medium | Critical | If ambiguous, don't generate code, ask the user; the most restrictive mapping by default |
| Scope grows, the MVP slips | High | High | The out-of-scope list; an ADR is required for every feature outside the MVP |
| Crubit adds Cargo support, closing the gap | Medium | High | Quarterly competitor check; focus on CMake, the annotation file and explanatory diagnostics as differences |
| Differences between libclang versions break compatibility | High | Medium | Test with several LLVM versions; isolate the header reading layer |
| The assumption about user demand turns out wrong | Medium | High | Apply the validation methods (issue scan, forums, demo feedback) before Milestone 5 |
| Writing the annotation file is too much burden for users | Medium | Medium | Measure annotation lines in the prototype; strengthen the inference rules |

---
*Status: DRAFT — requirements only. Implementation plans are made with `/ecc:plan`.*
