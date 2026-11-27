//! `tests/fixtures/<ad>/input.h` dosyalarını okuyup IR'ı JSON snapshot olarak karşılaştırır.
//!
//! Snapshot'lar `tests/snapshots/` altında. Değişiklikleri `cargo insta review` ile inceleyin.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use ferrobridge_parser::{ParseOptions, ParserError, parse_header, parse_headers};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .join("input.h")
}

/// Platforma bağlı ayrıntılar snapshot'a girmez: sabit genişlikli tamsayıların temel adı
/// (`uint64_t` glibc'de `unsigned long`, macOS'ta `unsigned long long`) ve standart kütüphane
/// tipi içeren sınıfların düzeni (`std::string` libstdc++'ta 32, libc++'ta 24 bayt). Üretilen
/// kod ikisine de bakmaz: analiz sabit genişlikte boyutu ve işareti kullanır, böyle sınıflar
/// opak kalır.
fn portable(module: &ferrobridge_ir::Module) -> serde_json::Value {
    fn walk(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("kind").and_then(|k| k.as_str()) == Some("integer")
                    && map.get("fixed_width") == Some(&serde_json::Value::Bool(true))
                {
                    map.insert("builtin".to_owned(), "(platforma bağlı)".into());
                }
                let has_std_field =
                    map.get("fields")
                        .and_then(|f| f.as_array())
                        .is_some_and(|fields| {
                            fields.iter().any(|f| {
                                let text = f.to_string();
                                ["\"string\"", "\"vector\"", "\"unique_ptr\""]
                                    .iter()
                                    .any(|kind| text.contains(&format!("\"kind\":{kind}")))
                            })
                        });
                if has_std_field {
                    map.insert("layout".to_owned(), "(platforma bağlı)".into());
                    if let Some(serde_json::Value::Array(fields)) = map.get_mut("fields") {
                        for field in fields {
                            if let serde_json::Value::Object(f) = field {
                                f.insert("offset_bits".to_owned(), "(platforma bağlı)".into());
                            }
                        }
                    }
                }
                for child in map.values_mut() {
                    walk(child);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(walk),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(module).unwrap();
    walk(&mut value);
    value
}

fn snapshot(name: &str) {
    let module = parse_header(&fixture(name), &ParseOptions::default()).unwrap();
    insta::assert_json_snapshot!(name, portable(&module));
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
fn classes() {
    snapshot("classes");
}

#[test]
fn enums() {
    snapshot("enums");
}

#[test]
fn strings() {
    snapshot("strings");
}

#[test]
fn vectors() {
    snapshot("vectors");
}

#[test]
fn destructors() {
    snapshot("destructors");
}

#[test]
fn self_including_header_is_read_once() {
    // Header ana dosya olarak verilseydi `#pragma once` işlemez, `Counter` iki kez tanımlanırdı.
    snapshot("self_include");
}

#[test]
fn prelude_is_included_before_the_header_but_not_lowered() {
    let dir = fixture("prelude").parent().unwrap().to_path_buf();
    let options = ParseOptions {
        prelude: vec![dir.join("prelude.h").to_str().unwrap().to_owned()],
        ..ParseOptions::default()
    };
    let module = parse_header(&fixture("prelude"), &options).unwrap();
    insta::assert_json_snapshot!("prelude", portable(&module));
}

#[test]
fn header_without_its_prelude_fails_to_parse() {
    let error = parse_header(&fixture("prelude"), &ParseOptions::default()).unwrap_err();
    let ParserError::ParseFailed { diagnostics } = &error else {
        panic!("ParseFailed bekleniyordu, gelen: {error:?}");
    };
    assert!(
        diagnostics.iter().all(|d| d.file.ends_with("input.h")),
        "tanılar header'ı göstermeli: {diagnostics:?}"
    );
}

#[test]
fn inline_functions_and_constants() {
    snapshot("inline_and_constants");
}

#[test]
fn one_bridge_reads_several_headers_and_knows_where_outside_types_live() {
    let dir = fixture("multi_header").parent().unwrap().to_path_buf();
    let module = parse_headers(
        &[dir.join("geometry.h"), dir.join("input.h")],
        &ParseOptions::default(),
    )
    .unwrap();
    insta::assert_json_snapshot!("multi_header", portable(&module));
}

fn instantiate(entries: &[(&str, &str)]) -> ParseOptions {
    ParseOptions {
        instantiate: entries
            .iter()
            .map(|(rust, cpp)| ((*rust).to_owned(), (*cpp).to_owned()))
            .collect(),
        ..ParseOptions::default()
    }
}

#[test]
fn class_templates_are_instantiated_from_their_pattern() {
    let options = instantiate(&[
        ("Vec3f", "geo::Vec3<float>"),
        ("Vec2f", "geo::Vec2<float>"),
        ("Fixed3", "geo::Fixed<float, 3>"),
    ]);
    let module = parse_header(&fixture("templates"), &options).unwrap();
    insta::assert_json_snapshot!("templates", portable(&module));
}

#[test]
fn an_instantiation_that_does_not_compile_names_its_entry() {
    let error = parse_header(
        &fixture("templates"),
        &instantiate(&[("Nope", "geo::Missing<int>")]),
    )
    .unwrap_err();
    let ParserError::ParseFailed { diagnostics } = &error else {
        panic!("ParseFailed bekleniyordu: {error:?}");
    };
    let first = diagnostics[0].to_string();
    assert!(
        first.starts_with("`[templates]` Nope = \"geo::Missing<int>\": "),
        "{first}"
    );
}

#[test]
fn an_instantiation_spelling_cannot_inject_code() {
    let error = parse_header(
        &fixture("templates"),
        &instantiate(&[("X", "int; void evil()")]),
    )
    .unwrap_err();
    assert!(
        matches!(error, ParserError::InvalidArgument(_)),
        "{error:?}"
    );
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

/// Geçici bir header yazar ve okur.
fn parse_text(name: &str, text: &str) -> ferrobridge_ir::Module {
    let dir =
        std::env::temp_dir().join(format!("ferrobridge-parser-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let header = dir.join("input.h");
    std::fs::write(&header, text).unwrap();
    let module = parse_header(&header, &ParseOptions::default()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    module
}

#[test]
fn chained_constants_are_checked_in_linear_time() {
    // Her sabit bir öncekine iki kez dayanıyor: başlatıcılar her başvuruda yeniden gezilseydi
    // 2^60 adım gerekirdi.
    let mut text = String::from("#pragma once\nconstexpr unsigned long long A0 = 1;\n");
    for i in 1..=60 {
        text.push_str(&format!(
            "constexpr unsigned long long A{i} = A{} + A{};\n",
            i - 1,
            i - 1
        ));
    }
    let module = parse_text("chain", &text);
    let last = module.items.last().unwrap();
    let ferrobridge_ir::Item::Constant(last) = last else {
        panic!("sabit bekleniyordu: {last:?}");
    };
    assert_eq!(last.value, ferrobridge_ir::ConstantValue::Unsigned(1 << 60));
}

#[test]
fn platform_dependent_constants_are_rejected() {
    let module = parse_text(
        "sizeof",
        "#pragma once\n#include <cstddef>\nstruct S { int a; double b; };\n\
         constexpr int PTR = sizeof(void*);\nconstexpr int ALIGN = alignof(double);\n\
         constexpr int OFF = offsetof(S, b);\nconstexpr int PLAIN = 4 * 2;\n",
    );
    let kinds: Vec<(String, &str)> = module
        .items
        .iter()
        .filter_map(|item| match item {
            ferrobridge_ir::Item::Constant(c) => Some((c.name.clone(), "constant")),
            ferrobridge_ir::Item::Unsupported(u) if u.construct == "constant" => Some((
                u.name.clone(),
                if u.reason.contains("platform") {
                    "platform"
                } else {
                    "other"
                },
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("PTR".to_owned(), "platform"),
            ("ALIGN".to_owned(), "platform"),
            ("OFF".to_owned(), "platform"),
            ("PLAIN".to_owned(), "constant"),
        ]
    );
}
