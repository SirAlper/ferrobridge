//! `generate` özelliği uçtan uca: geçici bir crate'in `build.rs`'i bir C++ header'ını libclang
//! ile okuyup bridge'i `OUT_DIR`'e üretir, derler; crate'in testi Rust'tan çağırır.
//!
//! Geçici crate'ler `target/ferrobridge-build-it/` altında kurulur ve ortak bir hedef
//! dizinini paylaşır (bağımlılıklar bir kez derlenir). Ağ kullanılmaz (`--offline`):
//! workspace'in `Cargo.lock`'u kopyalanır, bütün sürümler yerel önbellekte olmalı.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
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
// Kendi başına derlenmiyor: `Meters` prelude'dan geliyor.
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

/// Geçici crate'i `name` dizinine yazar; `toml` `ferrobridge.toml`'un içeriği.
///
/// Her crate'in paket adı farklı olmalı: Cargo kök paketin derleme kimliğini yolundan bağımsız
/// hesaplar; aynı adlı iki crate ortak hedef dizininde birbirinin `build.rs` çıktısını "güncel"
/// sayardı.
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
        // Dıştaki `cargo test`'in ayarları içerideki derlemeye sızmasın.
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
    // Üretilen dosyalar `OUT_DIR`'de, crate'in kendi dizininde değil.
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
        stderr.contains("`length` adında parametresi yok"),
        "{stderr}"
    );
}
