//! `ferrobridge parse` komutunun uçtan uca testleri: derlenmiş ikili gerçekten çalıştırılır.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .join("input.h")
}

fn ferrobridge(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn parse_prints_ir_as_json() {
    let header = fixture("free_functions");
    let output = ferrobridge(&["parse", header.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"add"), "fonksiyonlar: {names:?}");
    assert!(names.contains(&"clamp"), "fonksiyonlar: {names:?}");
}

#[test]
fn parse_passes_extra_clang_args() {
    let header = fixture("free_functions");
    let output = ferrobridge(&["parse", header.to_str().unwrap(), "--", "-DUNUSED=1"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_reports_syntax_errors_with_exit_code_1() {
    let header = fixture("syntax_error");
    let output = ferrobridge(&["parse", header.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("✗"), "stderr: {stderr}");
    assert!(stderr.contains("input.h:4:"), "stderr: {stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn parse_reports_missing_file() {
    let output = ferrobridge(&["parse", "/nonexistent/header.h"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("header bulunamadı"), "stderr: {stderr}");
}
