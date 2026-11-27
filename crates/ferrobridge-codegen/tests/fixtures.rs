//! Snapshot tests of the three files generated from the fixture headers.
//!
//! Zincir: `ferrobridge-parser` → `ferrobridge-analysis::check` → `ferrobridge-codegen::generate`.
//! The snapshots are under `tests/snapshots/`; review changes with `cargo insta review`.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use ferrobridge_codegen::{CodegenOptions, GeneratedFiles, generate};
use ferrobridge_parser::{ParseOptions, parse_header};

fn generated(name: &str) -> GeneratedFiles {
    let header = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .join("input.h");
    let config = header.with_file_name("ferrobridge.toml");
    let annotations = if config.exists() {
        ferrobridge_analysis::Annotations::from_file(&config).unwrap()
    } else {
        ferrobridge_analysis::Annotations::default()
    };
    let options = ParseOptions {
        instantiate: annotations.instantiations(),
        ..ParseOptions::default()
    };
    let module = parse_header(&header, &options).unwrap();
    let plan = ferrobridge_analysis::check(&module, &annotations).unwrap();
    generate(
        &plan,
        &CodegenOptions {
            header_includes: vec![format!("{name}/input.h")],
            shim_include: format!("{name}/shim.h"),
            shim_namespace: format!("ferrobridge::{name}"),
            prelude: Vec::new(),
        },
    )
    .unwrap()
}

fn snapshot(name: &str) {
    let files = generated(name);
    insta::assert_snapshot!(format!("{name}__bridge.rs"), files.bridge_rs);
    insta::assert_snapshot!(format!("{name}__shim.h"), files.shim_h);
    insta::assert_snapshot!(format!("{name}__shim.cc"), files.shim_cc);
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
fn ownership() {
    snapshot("ownership");
}

#[test]
fn vectors() {
    snapshot("vectors");
}

#[test]
fn slices() {
    snapshot("slices");
}

#[test]
fn operators() {
    snapshot("operators");
}

#[test]
fn no_exceptions() {
    snapshot("no_exceptions");
}

#[test]
fn value_types() {
    snapshot("value_types");
}

#[test]
fn inline_and_constants() {
    snapshot("inline_and_constants");
}

#[test]
fn generated_bridge_is_valid_rust() {
    for name in [
        "classes",
        "enums",
        "strings",
        "ownership",
        "vectors",
        "destructors",
        "free_functions",
        "namespaces",
        "slices",
        "inline_and_constants",
        "value_types",
        "operators",
        "no_exceptions",
    ] {
        let files = generated(name);
        if let Err(error) = syn::parse_file(&files.bridge_rs) {
            panic!("{name}: invalid Rust: {error}\n{}", files.bridge_rs);
        }
    }
}

#[test]
fn skipped_items_are_listed_in_the_generated_files() {
    let files = generated("classes");
    for needle in ["geo::Vehicle::total_km", "geo::Vehicle::raw_handle"] {
        assert!(files.bridge_rs.contains(needle), "bridge.rs: {needle}");
        assert!(files.shim_h.contains(needle), "shim.h: {needle}");
    }
}

#[test]
fn assumptions_are_listed_in_every_generated_file() {
    let files = generated("ownership");
    for (name, text) in [
        ("bridge.rs", &files.bridge_rs),
        ("shim.h", &files.shim_h),
        ("shim.cc", &files.shim_cc),
    ] {
        assert!(
            text.contains("Assumptions based on annotations"),
            "{name}: no assumptions block"
        );
        assert!(
            text.contains("own::make_widget return `owned` (ferrobridge.toml:4)"),
            "{name}"
        );
    }
    // A file without annotations has no block.
    assert!(!generated("classes").bridge_rs.contains("Assumptions"));
}

#[test]
fn shim_includes_the_prelude_before_the_header() {
    let header = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/prelude");
    let options = ParseOptions {
        prelude: vec![header.join("prelude.h").to_str().unwrap().to_owned()],
        ..ParseOptions::default()
    };
    let module = parse_header(&header.join("input.h"), &options).unwrap();
    let plan = ferrobridge_analysis::check(&module, &ferrobridge_analysis::Annotations::default())
        .unwrap();
    let files = generate(
        &plan,
        &CodegenOptions {
            header_includes: vec!["prelude/input.h".to_owned()],
            shim_include: "prelude/shim.h".to_owned(),
            shim_namespace: "ferrobridge::prelude".to_owned(),
            prelude: vec!["prelude/prelude.h".to_owned()],
        },
    )
    .unwrap();
    let prelude = files.shim_h.find("#include \"prelude/prelude.h\"").unwrap();
    let header = files.shim_h.find("#include \"prelude/input.h\"").unwrap();
    assert!(prelude < header, "{}", files.shim_h);
}
