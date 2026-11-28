//! Flag sources read through libclang (compilation databases) and the list of files a
//! translation unit read, against small projects written into a temporary directory.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use ferrobridge_parser::flags::{FlagKind, from_compilation_database};
use ferrobridge_parser::{ParseOptions, ParserError, parse_headers_with_inclusions};

/// A unique, clean, canonical directory per test.
fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("ferrobridge-parser-flags-{}", std::process::id()))
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A JSON string literal for a path.
fn json(path: &Path) -> String {
    format!("{:?}", path.to_str().unwrap())
}

/// A project with two entries: one written with `"arguments"`, one with a quoted `"command"`.
fn project(test: &str) -> PathBuf {
    let dir = temp_dir(test);
    fs::create_dir_all(dir.join("build")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.cpp"), "").unwrap();
    fs::write(dir.join("src/b.cpp"), "").unwrap();
    let build = dir.join("build");
    let database = format!(
        r#"[
  {{
    "directory": {build},
    "arguments": ["/usr/bin/c++", "-DFROM_ARGS", "-I../include", "-Wall", "-c", "../src/a.cpp"],
    "file": "../src/a.cpp"
  }},
  {{
    "directory": {build},
    "command": "ccache c++ -DX=\"a b\" -I inc -std=gnu++17 -O2 -o b.o -c {b_raw}",
    "file": {b}
  }}
]"#,
        build = json(&build),
        b = json(&dir.join("src/b.cpp")),
        b_raw = dir.join("src/b.cpp").display(),
    );
    fs::write(build.join("compile_commands.json"), database).unwrap();
    dir
}

/// libclang adds `--driver-mode=g++` after the compiler (the filter drops it silently).
fn without_driver_mode(arguments: &[String]) -> Vec<&str> {
    arguments
        .iter()
        .map(String::as_str)
        .filter(|arg| !arg.starts_with("--driver-mode="))
        .collect()
}

#[test]
fn an_entry_written_with_arguments_is_read() {
    let dir = project("arguments");
    let command = from_compilation_database(&dir.join("build"), &dir.join("src/a.cpp")).unwrap();
    assert_eq!(command.directory, dir.join("build"));
    assert_eq!(
        without_driver_mode(&command.arguments),
        ["-DFROM_ARGS", "-I../include", "-Wall", "-c", "../src/a.cpp"]
    );
    let flags = command.flags();
    let build = dir.join("build");
    assert_eq!(
        flags.clang_args(),
        [
            "-DFROM_ARGS".to_owned(),
            format!("-I{}", build.join("../include").display())
        ]
    );
    assert_eq!(flags.dropped.warnings, ["-Wall"]);
}

#[test]
fn a_command_string_is_split_like_clang_splits_it() {
    let dir = project("command");
    // The JSON file itself instead of its directory.
    let database = dir.join("build/compile_commands.json");
    let command = from_compilation_database(&database, &dir.join("src/b.cpp")).unwrap();
    // `ccache` and the compiler are not arguments.
    assert_eq!(without_driver_mode(&command.arguments)[0], "-DX=a b");
    let flags = command.flags();
    assert_eq!(flags.std.as_deref(), Some("gnu++17"));
    let kinds: Vec<FlagKind> = flags.flags.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, [FlagKind::Define, FlagKind::IncludeDir]);
    assert_eq!(
        flags.clang_args(),
        [
            "-DX=a b".to_owned(),
            format!("-I{}", dir.join("build/inc").display())
        ]
    );
}

#[test]
fn a_relative_source_matches_by_its_trailing_components() {
    let dir = project("suffix");
    let command = from_compilation_database(&dir.join("build"), Path::new("src/b.cpp")).unwrap();
    assert!(command.file.ends_with("src/b.cpp"));
}

#[test]
fn a_missing_entry_lists_the_closest_files() {
    let dir = project("missing");
    fs::write(dir.join("src/c.cpp"), "").unwrap();
    let error = from_compilation_database(&dir.join("build"), &dir.join("src/c.cpp")).unwrap_err();
    let ParserError::NoCompileCommand { closest, .. } = &error else {
        panic!("{error}");
    };
    assert_eq!(closest.len(), 2, "{closest:?}");
    assert!(error.to_string().contains("has no entry for"), "{error}");
    assert!(error.to_string().contains("closest:"), "{error}");
}

#[test]
fn a_directory_without_a_database_is_an_error() {
    let dir = temp_dir("no-database");
    let error = from_compilation_database(&dir, Path::new("a.cpp")).unwrap_err();
    assert!(
        matches!(error, ParserError::CompilationDatabase { .. }),
        "{error}"
    );
    let error = from_compilation_database(&dir.join("flags.json"), Path::new("a.cpp")).unwrap_err();
    assert!(
        error.to_string().contains("compile_commands.json"),
        "{error}"
    );
}

#[test]
fn every_file_the_unit_read_is_listed() {
    let dir = temp_dir("inclusions");
    fs::create_dir_all(dir.join("deep")).unwrap();
    fs::write(
        dir.join("top.h"),
        "#pragma once\n#include \"middle.h\"\ninline int top() { return middle(); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("middle.h"),
        "#pragma once\n#include \"deep/bottom.h\"\ninline int middle() { return bottom(); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("deep/bottom.h"),
        "#pragma once\ninline int bottom() { return 1; }\n",
    )
    .unwrap();
    fs::write(dir.join("prelude.h"), "#pragma once\n").unwrap();
    let options = ParseOptions {
        prelude: vec![dir.join("prelude.h").to_str().unwrap().to_owned()],
        ..ParseOptions::default()
    };
    let parsed = parse_headers_with_inclusions(&[dir.join("top.h")], &options).unwrap();
    assert_eq!(
        parsed.inclusions,
        [
            dir.join("deep/bottom.h"),
            dir.join("middle.h"),
            dir.join("prelude.h"),
            dir.join("top.h"),
        ]
    );
    assert!(!parsed.module.items.is_empty());
}
