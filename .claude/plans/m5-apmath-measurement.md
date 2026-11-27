# Measurement before Milestone 5: ferrobridge on `AP_Math` (2026-11-27)

**Goal:** M5's success criterion is "≥ 80% of the `AP_Math` public API usable from Rust". Before starting the plan, measure where today's ferrobridge (`40dce02`, end of M4) stands.

**Source:** ArduPilot `master`, commit `32e4f8e` (2026-11-27). Cloned shallow and sparse outside the repo. No GPLv3 code was copied into this repo (ADR-0003); only numbers and symbol names are here.

## Summary
- **Today: ~20%.** Roughly 86 of 424 public API items are bound. What is bound: free functions taking `float`/`int` in headers such as `AP_Math.h`, `control.h`, `crc.h`, `location.h`, and four small classes (`SCurve`, `SplineCurve`, `chirp`, `AP_GeodesicGrid`).
- **The main obstacle is templates.** The class templates `Vector2/3`, `Matrix3` and `QuaternionT` are the core of the library. They contain ~228 public members. On top of that, 27 free functions are skipped because they take types such as `Vector3f`, and 23 because they are templates themselves.
- **Reaching 80% needs several capabilities outside today's scope, together.** Rough estimate:
  - Class template instantiation and public fields: ~74%.
  - Plus function template instantiation and pointer + length support: ~79–80%.
  - With operators: ~95%.
- **The measurement also found two limits in the tool that need fixing** (see "Findings" below).

## Method
- `ferrobridge generate` and `ferrobridge parse` were run for every `libraries/AP_Math/*.h`.
- clang arguments: `-std=c++17`, `-I libraries`, SITL defines (`CONFIG_HAL_BOARD=HAL_BOARD_SITL`, `CONFIG_HAL_BOARD_SUBTYPE=HAL_BOARD_SUBTYPE_NONE`, `HAL_PROGRAM_SIZE_LIMIT_KB=4096`) and an empty `hwdef.h` stub. waf generates this file during the build.
- The four headers that do not compile on their own (`SCurve`, `SplineCurve`, `div1000`, `rotations`) were preloaded with `-include stdint.h -include AP_Math/AP_Math.h`. In ArduPilot they are always included after `AP_Math.h`.
- Because ferrobridge does not read template members (a whole template becomes a single "skipped" item), class bodies were counted **roughly** with a separate script: public methods, operators, constructors and fields only. These numbers may be off by ±10%.

## Numbers
| | Items |
|---|---|
| Bound functions/methods/constructors | **86** |
| Skipped (excluding the template classes themselves) | 109 |
| Public members of template classes (rough) | ~228 (`Vector3` 56, `Vector2` 49, `Matrix3` 46, `QuaternionT` 52, `VectorN` 17, `MatrixN` 6, `PolyFit` 2) |
| Not measurable (`rotations.h`: the `Rotation` enum) | 1 |
| **Total (denominator)** | **~424** |

Reasons for the skipped items (excluding template classes):
| Reason | Items | Note |
|---|---|---|
| Parameter/return type `Vector2f`, `Vector3f`... (template) | 27 | Solved by class template instantiation |
| Function that is itself a template (`is_zero<T>`, `constrain_value<T>`, `wrap_180`...) | 23 | Needs function template instantiation |
| Pointer + length (`crc_*(const uint8_t* buf, uint16_t len)`, `fill_nanf`) | 25 | **Cannot be annotated with `borrowed`:** a buffer, not a single object. Needs slice support (postponed to v0.2 in M3) |
| Non-const primitive `T&` (`control.h` output parameters) | 8 | **Can be bound today with `ferrobridge.toml`** |
| `static inline` functions (`radians`, `degrees`, `sq`, `cd_to_rad`...) | 8 | See finding 3 |
| Global/static constants (`WGS84_A`, `NaNf`...) | 9 | Can be generated as Rust `const` (libclang can evaluate the constant value) |
| Unrecognized declarations (the template specialization `Vector3<float>::is_zero` ×4, the `AP_PARAMDEFV` macro ×1) | 5 | |
| Other (`float[5]` array parameter, `void*`, `size_t`, `Rotation` from another header) | 4 | |

## Cumulative scenarios (rough)
| Capability added | Gained | Total bound | Coverage |
|---|---|---|---|
| Today | | 86 | 20% |
| + `ferrobridge.toml` for the `control.h` output parameters (just writing it) | +8 | 94 | 22% |
| + allowing `static inline` header functions | +8 | 102 | 24% |
| + pointer + length → `&[T]` | +25 | 127 | 30% |
| + class template instantiation (`Vector2f/3f`, `Matrix3f`, `Quaternion`) + public fields (without operators) | +186 | 313 | 74% |
| + function template instantiation | +23 | 336 | 79% |
| + constants (`const`) | +9 | 345 | 81% |
| + operators (`Add`, `Mul`, `Index`...) | +69 | 414 | 98% |

> **Correction (2026-11-27, after M5):** this table silently assumes that cross-header types work. ferrobridge binds every header separately; a function using a type defined in another header is skipped (e.g. `rotation_equal(Rotation, Rotation)` in `AP_Math.h`).
> - About 159 of the +186 in the "class template instantiation" row are the template classes' own members. The other 27 are functions using `Vector2f/3f` from other headers: `control.h`, `location.h`, `SCurve.h`, `SplineCurve.h`, `AP_Math.h`.
> - These 27 functions do not open up with template instantiation alone; cross-header types are needed too.
> - Without cross-header types that row would be ~68% instead of roughly 74%.
> - That is why cross-header types were added to M6's scope.

## Findings (things to fix in the tool)
1. **A header that includes itself indirectly cannot be read.** `matrixN.h`, `vectorN.h` and `rotations.h` include themselves again through `AP_Math.h`. ferrobridge gives the header to libclang as the "main file"; `#pragma once` does not protect the main file and the class is defined twice (`redefinition of 'VectorN'`). Fix: parse a synthetic translation unit containing `#include "<header>"` and take only the declarations from that header.
2. **Headers that do not compile on their own** (`SCurve.h` uses `Vector3f` without including it, `rotations.h` uses `uint8_t`). Today this can be worked around with `-- -include ...`; it needs documenting, or a `prelude` setting in `ferrobridge.toml`.
3. **`static inline` functions may be rejected more strictly than necessary.** The rule says "internal-linkage functions cannot be called from outside the header". But the C++ file generated by cxx and the shim includes the header; a `static inline` function is visible in that translation unit. Hypothesis: they can be bound safely. To be verified (the rule for types in an anonymous namespace needs separate thought).
4. **`AP_Math.h` pulls in a large part of ArduPilot.** Through `AP_Param` (the `AP_PARAMDEFV` macro), 22 more libraries are included (28 in total). `hwdef.h` is also generated by waf. A stub was enough for parsing. But for the demo crate to **compile and link** the `.cpp` files, dependencies such as `AP_HAL` and `AP_InternalError` may need stubs. This is M5's biggest build risk.
5. **`-fno-exceptions`:** because the demo crate compiles `AP_Math` itself, exceptions can be enabled; `AP_Math` does not throw anyway. The question really stays open for users embedding into an existing `-fno-exceptions` build. Not a blocker for the demo.

## Options for M5's scope
1. **Keep the goal (≥ 80%).** M5 takes in class and function template instantiation (the PRD's v0.2 goal: `instantiate = ["Vector3<float>"]`), public fields, pointer + length → slices and finding 1. The scope is very large: probably needs splitting into two milestones.
2. **Split the goal into an intermediate step.** M5 = `ferrobridge-build` + the demo crate + findings 1–3 + constants + slices; measurement ~30–35%. Templates and fields move to M6, the v0.1 release to M7. The milestone table in the PRD changes.
3. **Redefine the goal.** Make the criterion "≥ 80% of the non-template API". ~59% today (86/145); ~88% with slices, `static inline` and output parameters. But an `AP_Math` demo without `Vector3f` would not be convincing.

**Recommendation:** option 2. Template instantiation is ferrobridge's biggest design task, postponed since M1, and deserves its own milestone. M5 first brings up the build pipeline and the demo skeleton on a real library. This is the riskiest part (finding 4); it also lays solid ground for the template work.

## Reproducing
```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/ArduPilot/ardupilot.git
cd ardupilot && git sparse-checkout set libraries/AP_Math libraries/AP_Common libraries/AP_HAL libraries/AP_HAL_SITL libraries/AP_Param ...
# (missing libraries were added one by one from parse errors; 28 libraries in total)
mkdir -p ../stubs && : > ../stubs/hwdef.h
ferrobridge generate libraries/AP_Math/vector3.h --out-dir /tmp/out -- \
  -I libraries -I ../stubs -DCONFIG_HAL_BOARD=HAL_BOARD_SITL \
  -DCONFIG_HAL_BOARD_SUBTYPE=HAL_BOARD_SUBTYPE_NONE -DHAL_PROGRAM_SIZE_LIMIT_KB=4096
```

## After M5 (2026-11-27)

The same method; ArduPilot `437a3301cc` (a newer commit than in the measurement), all `AP_Math/*.h` headers with `--prelude AP_Math/AP_Math.h`. Bound item = function/method/constructor + constant + enum. Template class members again ~228 by rough count.

| | Bound | Non-template API | Total |
|---|---|---|---|
| M4 (measurement) | 86 | 86/145 = 59% | 86/424 = 20% |
| M5, without annotations | 99 | 99/146 = 68% | 99/424 = 23% |
| **M5, with the demo's annotations** (`demo/ap_math/annotations/`) | **131** | **131/146 = 90%** | **131/424 = 31%** |

The goals (non-template ≥ 85%, total ≥ 30%) were met. Findings 1–3 are closed; all headers are read (including `vectorN.h`, `matrixN.h`, `rotations.h`). The `Rotation` enum is bound too.

The 15 items left in the non-template API:
- **Deliberately not annotated:** `crc_crc4` has no length parameter; `crc8_dvb_s2_update` takes `void*`.
- **Platform-dependent `long`:** `wrap_360_cd(long)`.
- **A type from another header:** `rotation_equal(Rotation, Rotation)`. `Rotation` is in `rotations.h`; cross-header types are not bound yet.
- **Constants whose value is computed with a function call (4):** `NaNf`, `DEG_TO_RAD_DOUBLE`, `RAD_TO_DEG_DOUBLE`, `WGS84_E`.
- **Unrecognized declarations:** the `AP_PARAMDEFV` macro and template specializations.

A finding from reading the sources: despite its name, `crc8_dvb_update` uses 0x07, not DVB-S2's polynomial (0xD5); i.e. it is CRC-8/SMBUS. The demo test shows this by comparing it with `crc8_dvb_s2`.

## After M6 (2026-11-27)

The same method: ArduPilot `437a3301cc`, all `AP_Math/*.h` in a single bridge, `--prelude AP_Math/AP_Math.h`, the demo's `ferrobridge.toml`. Bound item = function/method/constructor + constant + enum.

**A real count instead of a rough estimate.** Template class members are now really bound or skipped with a reason, so real numbers replaced M5's rough estimate of ~228. Members are counted **once** per template: the members of the `double` instantiations (181 bound, 10 skipped) were subtracted. For the members of the three templates that are not instantiated (`VectorN`, `MatrixN`, `PolyFit`), M5's rough count (25) was used.

| | Bound | Denominator | Coverage |
|---|---|---|---|
| M4 (measurement) | 86 | ~424 | 20% |
| M5 | 131 | ~424 | 31% |
| M6, `float` instantiations only | 321 | 412 | 78% |
| **M6, the demo's configuration** (`float` + `double`) | **337** | **412** | **81.8%** |
| ↳ also counting the 12 public fields of the value types (M5's estimate included fields) | 349 | 424 | 82% |

**Why are the `double` instantiations needed?** In SITL, `postype_t` is `double`. So some of the functions in `SCurve`, `SplineCurve`, `location.h` and `control.h` take `Vector3p` (`Vector3<double>`); they open up only with the `double` instantiations. The denominator coming out as 412 in both configurations shows the count is consistent.

The 53 items left:
- **Function templates:** ~23 (`wrap_180<T>`, `is_zero<T>`, `constrain_value<T>`, `sq`, `MIN`/`MAX`...).
- **Deliberately not bound:**
  - `operator[]`, without bounds checks
  - `operator()`, `operator=`
  - `crc_crc4`
  - the function taking `void*`
  - 4 constants with libm calls
- **Arrays:** `float[5]`, `T[4]`.
- **Polygon functions:** raw pointer + length, but templated on `Vector2<T>`.
- **Other:** `long`, unrecognized declarations (`AP_PARAMDEFV`, template specializations).
