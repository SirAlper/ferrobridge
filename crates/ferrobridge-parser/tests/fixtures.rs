//! `tests/fixtures/<ad>/input.h` dosyalarını okuyup IR'ı JSON snapshot olarak karşılaştırır.
//!
//! Snapshot'lar `tests/snapshots/` altında. Değişiklikleri `cargo insta review` ile inceleyin.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use ferrobridge_parser::{ParseOptions, ParserError, parse_header};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .join("input.h")
}

fn snapshot(name: &str) {
    let module = parse_header(&fixture(name), &ParseOptions::default()).unwrap();
    insta::assert_json_snapshot!(name, module);
}

#[test]
fn free_functions() {
    snapshot("free_functions");
}

#[test]
fn primitive_types() {
    snapshot("primitive_types");
}

#[test]
fn namespaces() {
    snapshot("namespaces");
}

#[test]
fn unsupported() {
    snapshot("unsupported");
}

#[test]
fn syntax_error_reports_diagnostics_with_line_numbers() {
    let error = parse_header(&fixture("syntax_error"), &ParseOptions::default()).unwrap_err();
    let ParserError::ParseFailed { diagnostics } = &error else {
        panic!("ParseFailed bekleniyordu, gelen: {error:?}");
    };
    assert!(!diagnostics.is_empty());
    let first = diagnostics[0].to_string();
    assert!(first.contains("input.h:"), "konum yok: {first}");
    assert_eq!(diagnostics[0].line, 4, "tanı: {first}");
}

#[test]
fn missing_file_is_reported() {
    let path = fixture("does_not_exist");
    let error = parse_header(&path, &ParseOptions::default()).unwrap_err();
    assert!(
        matches!(&error, ParserError::FileNotFound(p) if p == &path),
        "beklenmeyen hata: {error:?}"
    );
}
