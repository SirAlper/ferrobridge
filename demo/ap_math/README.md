# The AP_Math demo

ArduPilot's [`AP_Math`](https://github.com/ArduPilot/ardupilot/tree/master/libraries/AP_Math) library used from Rust. No bridge or `unsafe` is written by hand; `build.rs` generates the bridge during the build with `ferrobridge-build`.

```rust
use ferrobridge_demo_ap_math::{self as ap, Quaternion, Rotation, Vector3f};

let a = Vector3f { x: 1.0, y: 2.0, z: 2.0 };       // a real Rust struct
let b = a + Vector3f { x: 0.0, y: 1.0, z: 0.0 } * 2.0;  // C++ operators, through `std::ops`
let len = a.length();                               // no `Result`: ArduPilot does not use exceptions

let mut q = Quaternion::new_noargs();
q.from_euler_f32_f32_f32(0.0, 0.0, std::f32::consts::FRAC_PI_2);
let nose = q * Vector3f { x: 1.0, y: 0.0, z: 0.0 };   // quaternion × vector

let mut accel = Vector3f { x: 0.0, y: 0.0, z: -9.80665 };
accel.rotate(Rotation::ROTATION_ROLL_180);          // an enum from another header

let crc = !ap::crc_crc32(!0, b"123456789")?;        // `const uint8_t*` + length → `&[u8]`
```

## Run it in 5 minutes

```bash
git clone --depth 1 https://github.com/ArduPilot/ardupilot.git ~/ardupilot
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"      # macOS; on Linux, libclang's directory
ARDUPILOT_DIR=~/ardupilot cargo run --manifest-path demo/ap_math/Cargo.toml --example tour
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

`examples/tour.rs` shows, in order:
- vector algebra
- a quaternion from Euler angles, compared with a matrix
- sensor mounting with `Rotation`
- jerk-limited acceleration from `control.h`
- CRC
- WGS84 constants

A full checkout is not needed. The 28 libraries the build includes are enough (found with `clang -M`):

```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/ArduPilot/ardupilot.git ~/ardupilot
git -C ~/ardupilot sparse-checkout set \
  libraries/AC_Fence libraries/AP_Arming libraries/AP_BoardConfig libraries/AP_CANManager \
  libraries/AP_Common libraries/AP_CustomRotations libraries/AP_Filesystem libraries/AP_HAL \
  libraries/AP_HAL_SITL libraries/AP_InertialSensor libraries/AP_InternalError libraries/AP_Logger \
  libraries/AP_MSP libraries/AP_Math libraries/AP_Mission libraries/AP_Mount libraries/AP_Networking \
  libraries/AP_Param libraries/AP_ROMFS libraries/AP_Rally libraries/AP_RangeFinder libraries/AP_Relay \
  libraries/AP_Scripting libraries/AP_SerialManager libraries/AP_Terrain libraries/AP_Vehicle \
  libraries/GCS_MAVLink libraries/StorageManager
```

**Verified with:**
- ArduPilot `437a3301cc`, macOS (arm64), Homebrew LLVM 23.
- Both a full checkout and the 28-library sparse clone above.
- CI (`.github/workflows/ci.yml`) builds with the same sparse clone on Linux and macOS and runs the tests and the tour.

## Compared with hand-written cxx

Binding `Vector3f` with cxx by hand means writing all of this:
- a `#[cxx::bridge]` module
- the alias `using Vector3f = Vector3<float>;` on the C++ side
- a `#[repr(C)]` Rust struct with exactly the same fields in the same order and types, and a justified `unsafe impl cxx::ExternType` for it
- `static_assert`s verifying that the layout really is the same
- a C++ wrapper function for every operator, because cxx cannot bind operator names
- an `impl std::ops::Add/Mul/...` for every operator
- separate Rust names for overloads with the same name

All of this for the 51 members of `Vector3f`. Then repeated for `Vector2f`, `Matrix3f`, `Quaternion` and the `double` versions. And updated by hand whenever a field or signature changes.

With ferrobridge, the same work is four lines in `ferrobridge.toml`:

```toml
[templates]
Vector3f = "Vector3<float>"
Matrix3f = "Matrix3<float>"
Quaternion = "QuaternionT<float>"
```

**The demo in total:**
- **59 annotation lines** in `ferrobridge.toml` produce **more than 8,000 lines of generated code** (`bridge.rs` + `shim.h` + `shim.cc`).
- That includes 8 value-type `unsafe impl`s (all justified) and 48 layout `static_assert`s.
- All of it is regenerated during the build.

## What is bound

| | |
|---|---|
| **Value types** | `Vector2f/3f`, `Matrix3f`, `Quaternion` and the `double` versions. Rust structs; fields are read and written; methods take `&self`/`&mut self`; the `+ - * / %`, `+= -= *= /=`, `==` and unary `-` operators through `std::ops` |
| **Opaque classes** | `SCurve`, `chirp`: an owning wrapper, the C++ destructor runs when it is dropped. `SplineCurve` has no public constructor; its methods can be used on objects obtained in other ways |
| **Free functions** | Angle conversions (`radians_f32`, `degrees`, `wrap_360_f32`), constraining, unit conversions, the controller helpers in `control.h` (`&mut f32` output parameters), CRCs (`&[u8]`, `&[u32]`) |
| **Enum** | `Rotation` (a real Rust enum) |
| **Constants** | `WGS84_*` (`pub const`; bit-for-bit identical to the C++ value) |

**Coverage:**
- About **82%** of the `AP_Math` API is bound: 337/412 items, plus the 12 fields of the value types.
- The same method as the M5 measurement: template members are counted once per template; with `float` instantiations only, the result is 78%.
- It was 31% in M5 and 20% before M5.
- Details: `.claude/plans/m5-apmath-measurement.md` → "After M6".

## Known limits

- **Function templates are not bound:** `wrap_180<T>`, `is_zero<T>`, `constrain_value<T>` (v0.2).
- **`operator[]` is not bound.** In ArduPilot `[]` does no bounds checks (`MATH_CHECK_INDEXES` is off). `v[5]` from safe Rust would be undefined behavior. Use the fields (`v.x`).
- **Overloads get type suffixes:** `from_euler_f32_f32_f32`, `to_euler_mut_f32_mut_f32_mut_f32` (ADR-0005). The rule is clear, but it produces long names.
- **ferrobridge cannot know the preconditions of C++ functions.** If a C++ function has undefined behavior when its precondition is broken, a call to it from safe Rust has it too. This is a limit of every binding tool. What ferrobridge can do: turn pointers and lengths into slices and catch overflows, turn null returns into `Err`, and not bind an unchecked `[]`.
- **Constructors of opaque classes** return the owning wrapper inside `Result` (`SCurve::new()?`).
- **Broken promises.** `[library] exceptions = false` is right for ArduPilot: the firmware is compiled with `-fno-exceptions` and `AP_Math` has no `throw`. If the promise were broken, the program would end with `std::terminate`; it would not be undefined behavior.

## Annotations (`ferrobridge.toml`)

Headers do not say who owns pointers and references. Every line in `ferrobridge.toml` is a promise made by reading the ArduPilot source. For example, "the `buf` of `crc_crc32` is an array of `size` elements and is not kept":

```toml
[functions.crc_crc32.params]
buf = { slice = "size" }
```

In Rust this promise becomes `crc_crc32(crc: u32, buf: &[u8])`. If the slice does not fit in a `uint32_t`, the call returns `Err`. The deliberately unannotated ones (`crc_crc4` and a function taking `void*`) are written in the file with reasons. All promises are listed at the top of the generated `bridge.rs` under "Assumptions based on annotations".

`stubs/ap_stubs.cc` provides the four pieces `AP_Math` needs from outside itself: `AP_InternalError`, `AP_CustomRotations`, `AP_HAL::panic` and the `AP::*` accessors. Their limits are written at the top of the file.

## License

This directory is licensed **GPL-3.0-or-later** (`LICENSE`), because it links against GPL-3.0-licensed ArduPilot. ferrobridge itself stays `MIT OR Apache-2.0` (ADR-0003, ADR-0008).
- ArduPilot code does not enter this repository; `ARDUPILOT_DIR` points to a checkout.
- Files generated from the GPL headers are not committed; they are generated into `OUT_DIR` under `target/`.
