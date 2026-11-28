//! The `generate` feature end to end: a temporary crate's `build.rs` reads a C++ header with
//! libclang, generates the bridge into `OUT_DIR` and compiles it; the crate's test calls it from Rust.
//!
//! The temporary crates are set up under `target/ferrobridge-build-it/` and share a target
//! directory (dependencies are compiled once). No network is used (`--offline`):
//! the workspace's `Cargo.lock` is copied, and all versions must be in the local cache.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn it_dir() -> PathBuf {
    root().join("target/ferrobridge-build-it")
}

const HEADER: &str = r#"#pragma once
#include <cstdint>

namespace geo {
int32_t add(int32_t a, int32_t b);
static inline int32_t twice(int32_t x) { return 2 * x; }
constexpr double SCALE = 2.5;
// Does not compile on its own: `Meters` comes from the prelude.
Meters to_meters(int32_t centimeters);
uint32_t sum(const uint8_t* bytes, uint32_t len);
}  // namespace geo
"#;

const PRELUDE: &str = "#pragma once\nusing Meters = double;\n";

const IMPL: &str = r#"#include "units.h"
#include "geo.h"

namespace geo {
int32_t add(int32_t a, int32_t b) { return a + b; }
Meters to_meters(int32_t centimeters) { return centimeters / 100.0; }
uint32_t sum(const uint8_t* bytes, uint32_t len) {
    uint32_t total = 0;
    for (uint32_t i = 0; i < len; ++i) total += bytes[i];
    return total;
}
}  // namespace geo
"#;

const BUILD_RS: &str = r#"fn main() {
    let mut build = ferrobridge_build::Build::new();
    build
        .include_dir("cpp")
        .file("cpp/geo.cc")
        .generate(
            ferrobridge_build::Generate::new("geo")
                .header_as("cpp/geo.h", "geo.h")
                .prelude("units.h")
                .config("ferrobridge.toml"),
        );
    if let Err(error) = build.compile("fb_build_it") {
        panic!("{error}");
    }
}
"#;

const LIB_RS: &str = "pub mod geo {\n    ferrobridge::include_bridge!(\"geo\");\n}\n";

const TEST_RS: &str = r#"use crate_under_test::geo;

#[test]
fn generated_bridge_is_callable() -> ferrobridge::Result<()> {
    assert_eq!(geo::add(2, 3)?, 5);
    assert_eq!(geo::twice(21)?, 42);
    assert_eq!(geo::SCALE, 2.5);
    assert_eq!(geo::to_meters(250)?, 2.5);
    assert_eq!(geo::sum(&[1, 2, 3])?, 6);
    Ok(())
}
"#;

/// Writes the temporary crate into the `name` directory; `toml` is the contents of `ferrobridge.toml`.
///
/// Every crate must have a different package name: Cargo computes the root package's build
/// identity independently of its path; two crates with the same name in a shared target directory
/// would consider each other's `build.rs` output "fresh".
fn write_crate(name: &str, toml: &str) -> PathBuf {
    let package = format!("fb_build_it_{}", name.replace('-', "_"));
    let dir = it_dir().join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("cpp")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();
    let root = root();
    let manifest = format!(
        r#"[package]
name = "{package}"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
cxx = "1.0.198"
ferrobridge = {{ path = "{runtime}" }}

[build-dependencies]
ferrobridge-build = {{ path = "{build}", features = ["generate"] }}
"#,
        runtime = root.join("crates/ferrobridge").display(),
        build = root.join("crates/ferrobridge-build").display(),
    );
    fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    fs::copy(root.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    fs::write(dir.join("cpp/geo.h"), HEADER).unwrap();
    fs::write(dir.join("cpp/units.h"), PRELUDE).unwrap();
    fs::write(dir.join("cpp/geo.cc"), IMPL).unwrap();
    fs::write(dir.join("ferrobridge.toml"), toml).unwrap();
    fs::write(dir.join("build.rs"), BUILD_RS).unwrap();
    fs::write(dir.join("src/lib.rs"), LIB_RS).unwrap();
    fs::write(
        dir.join("tests/it.rs"),
        TEST_RS.replace("crate_under_test", &package),
    )
    .unwrap();
    dir
}

fn cargo_test(dir: &Path) -> Output {
    Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", it_dir().join("target"))
        // The settings of the outer `cargo test` must not leak into the inner build.
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .unwrap()
}

#[test]
fn build_rs_generates_compiles_and_links_a_bridge() {
    let dir = write_crate(
        "ok",
        "[functions.\"geo::sum\".params]\nbytes = { slice = \"len\" }\n",
    );
    let output = cargo_test(&dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("test result: ok. 1 passed"), "{stdout}");
    // The generated files are in `OUT_DIR`, not in the crate's own directory.
    assert!(!dir.join("src/bridge.rs").exists());
}

#[test]
fn annotation_errors_stop_the_build_with_their_line() {
    let dir = write_crate(
        "bad-annotation",
        "[functions.\"geo::sum\".params]\nbytes = { slice = \"length\" }\n",
    );
    let output = cargo_test(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("ferrobridge.toml:2"), "{stderr}");
    assert!(
        stderr.contains("has no parameter named `length`"),
        "{stderr}"
    );
}

const DB_HEADER: &str = r#"#pragma once
#include <cstdint>
#include "geo_config.h"
#ifndef GEO_ENABLED
#error "GEO_ENABLED must come from compile_commands.json"
#endif

namespace geo {
int32_t scale(int32_t x);
}  // namespace geo
"#;

const DB_IMPL: &str = r#"#include "geo.h"

namespace geo {
// `GEO_FACTOR` reaches the C++ compiler only through the compilation database.
int32_t scale(int32_t x) { return GEO_FACTOR * x; }
}  // namespace geo
"#;

const DB_BUILD_RS: &str = r#"fn main() {
    let mut build = ferrobridge_build::Build::new();
    build
        .compile_commands("build/compile_commands.json", "cpp/geo.cc")
        .file("cpp/geo.cc")
        .generate(ferrobridge_build::Generate::new("geo").header("cpp/geo.h"));
    if let Err(error) = build.compile("fb_build_it_db") {
        panic!("{error}");
    }
}
"#;

const DB_TEST_RS: &str = r#"use crate_under_test::geo;

#[test]
fn flags_from_the_database_reach_libclang_and_the_compiler() -> ferrobridge::Result<()> {
    assert_eq!(geo::scale(2)?, 6);
    Ok(())
}
"#;

/// The lines the crate's build script printed on stdout, from its newest run. Cargo keeps them in
/// `build/<package>-<hash>/output` (older layout) or `build/<package>/<hash>/run/stdout`.
fn build_script_lines(package: &str) -> Vec<String> {
    fn walk(dir: &Path, package: &str, found: &mut Vec<(std::time::SystemTime, PathBuf)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let text = path.to_string_lossy();
            if path.is_dir() {
                walk(&path, package, found);
            } else if text.contains(package)
                && (path.ends_with("output") || path.ends_with("run/stdout"))
            {
                found.push((fs::metadata(&path).unwrap().modified().unwrap(), path));
            }
        }
    }
    let mut found = Vec::new();
    walk(&it_dir().join("target/debug/build"), package, &mut found);
    found.sort();
    let (_, newest) = found.last().unwrap();
    fs::read_to_string(newest)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn flags_come_from_a_compile_commands_entry() {
    let dir = write_crate("db", "");
    let package = "fb_build_it_db";
    fs::create_dir_all(dir.join("cpp/inc")).unwrap();
    fs::create_dir_all(dir.join("build")).unwrap();
    fs::remove_file(dir.join("ferrobridge.toml")).unwrap();
    fs::write(dir.join("cpp/geo.h"), DB_HEADER).unwrap();
    fs::write(dir.join("cpp/geo.cc"), DB_IMPL).unwrap();
    fs::write(dir.join("cpp/inc/geo_config.h"), "#pragma once\n").unwrap();
    // A unique line: Cargo compares contents, and the build script's output is read below, so it
    // must run in this test even if an earlier run left a fresh build.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    fs::write(
        dir.join("build.rs"),
        format!("// run {nonce}\n{DB_BUILD_RS}"),
    )
    .unwrap();
    fs::write(
        dir.join("tests/it.rs"),
        DB_TEST_RS.replace("crate_under_test", package),
    )
    .unwrap();
    let database = format!(
        r#"[{{
  "directory": {build:?},
  "command": "c++ -I../cpp -I../cpp/inc -DGEO_ENABLED -DGEO_FACTOR=3 -Wall -Werror -O2 -o geo.o -c ../cpp/geo.cc",
  "file": "../cpp/geo.cc"
}}]"#,
        build = dir.join("build").to_str().unwrap(),
    );
    fs::write(dir.join("build/compile_commands.json"), database).unwrap();

    let output = cargo_test(&dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("test result: ok. 1 passed"), "{stdout}");

    let script = build_script_lines(package);
    let has = |line: &str| script.iter().any(|l| l == line);
    let rerun = |path: &Path| format!("cargo:rerun-if-changed={}", path.display());
    // The header that was read is watched; the include directory it came from is not scanned.
    assert!(
        has(&rerun(&dir.join("cpp/inc/geo_config.h"))),
        "{script:#?}"
    );
    assert!(!has(&rerun(&dir.join("cpp/inc"))), "{script:#?}");
    // The database itself, not `<entry directory>/compile_commands.json`.
    assert!(
        has(&rerun(&dir.join("build/compile_commands.json"))),
        "{script:#?}"
    );
    assert!(
        script
            .iter()
            .any(|l| l.starts_with("ferrobridge: flags: dropped 2 warning flags")),
        "{script:#?}"
    );
    assert!(
        !script
            .iter()
            .any(|l| l.starts_with("cargo:warning=ferrobridge: flags")),
        "{script:#?}"
    );
}
