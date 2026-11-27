//! Builds ArduPilot's `AP_Math` library and the bridges generated from it.
//!
//! ArduPilot code does not enter this repository (GPL-3.0; ADR-0003). `ARDUPILOT_DIR` must point
//! to a checkout; the bridges are generated into `OUT_DIR` during the build and not committed (ADR-0008).

use std::path::{Path, PathBuf};

/// What waf defines in the SITL build (the measurement and M5 Task 0).
const DEFINES: [(&str, &str); 6] = [
    ("CONFIG_HAL_BOARD", "HAL_BOARD_SITL"),
    ("CONFIG_HAL_BOARD_SUBTYPE", "HAL_BOARD_SUBTYPE_NONE"),
    ("HAL_PROGRAM_SIZE_LIMIT_KB", "4096"),
    ("__AP_LINE__", "__LINE__"),
    ("__STDC_FORMAT_MACROS", "1"),
    ("APM_BUILD_DIRECTORY", "APM_BUILD_UNKNOWN"),
];

fn main() {
    println!("cargo:rerun-if-env-changed=ARDUPILOT_DIR");
    let libraries = ardupilot_libraries();

    let mut build = ferrobridge_build::Build::new();
    build
        .include_dir(&libraries)
        // On macOS, `feenableexcept` comes from here.
        .include_dir(libraries.join("AP_Common/missing"))
        // `hwdef.h`
        .include_dir("stubs")
        .file("stubs/ap_stubs.cc")
        .files(cpp_sources(&libraries.join("AP_Math")))
        // ArduPilot's own warnings (e.g. unused parameters) must not drown the demo's
        // output; not passed to libclang.
        .flag("-w");
    for (name, value) in DEFINES {
        build.define(name, Some(value));
    }
    // All `AP_Math` headers form a single bridge: a type from one header (`Vector3f`, `Rotation`)
    // can be used in the functions of the others.
    let mut bridge = ferrobridge_build::Generate::new("ap_math")
        // Some headers do not compile on their own (`SCurve.h` does not include `Vector3f`, `rotations.h`
        // does not include `uint8_t`); in ArduPilot they always come after `AP_Math.h`.
        .prelude("AP_Math/AP_Math.h")
        .config("ferrobridge.toml");
    for header in headers(&libraries.join("AP_Math")) {
        let include = format!(
            "AP_Math/{}",
            header.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
        );
        bridge = bridge.header_as(&header, &include);
    }
    build.generate(bridge);
    if let Err(error) = build.compile("ap_math") {
        panic!("{error}");
    }
}

fn ardupilot_libraries() -> PathBuf {
    let Some(dir) = std::env::var_os("ARDUPILOT_DIR") else {
        panic!(
            "`ARDUPILOT_DIR` is not set. It must point to an ArduPilot checkout, e.g.:\n  \
             ARDUPILOT_DIR=~/ardupilot cargo test\n\
             Sparse clone commands: demo/ap_math/README.md"
        );
    };
    let libraries = PathBuf::from(dir).join("libraries");
    if !libraries.join("AP_Math/AP_Math.h").is_file() {
        panic!(
            "`{}` is not an ArduPilot checkout (`libraries/AP_Math/AP_Math.h` is missing).",
            libraries.display()
        );
    }
    libraries
}

/// All headers of `AP_Math`, in name order.
fn headers(dir: &Path) -> Vec<PathBuf> {
    files_with_extension(dir, "h")
}

/// All `.cpp` files of `AP_Math`, in name order (so the build is reproducible).
fn cpp_sources(dir: &Path) -> Vec<PathBuf> {
    files_with_extension(dir, "cpp")
}

fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    files.sort();
    files
}
