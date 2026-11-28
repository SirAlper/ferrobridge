//! Flags from a build system (`--flags-file`, `--compile-commands --for`), the standard's
//! precedence, the depfile and the cxx C++ glue, end to end through the built binary.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A unique, clean, canonical directory per test.
fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("ferrobridge-cli-flags-{}", std::process::id()))
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn ferrobridge<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(args)
        .output()
        .unwrap()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// A header that compiles only with `-I <dir>/inc` (for `config.h`) and `-DFB_ENABLED`, and only
/// in C++20 when `NEEDS_CXX20` is defined.
fn project(test: &str) -> PathBuf {
    let dir = temp_dir(test);
    for sub in ["inc", "lib", "src", "build"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    fs::write(
        dir.join("inc/config.h"),
        "#pragma once\n#define FB_SCALE 2\n",
    )
    .unwrap();
    fs::write(
        dir.join("lib/api.h"),
        r#"#pragma once
#include <cstdint>
#include "config.h"
#ifndef FB_ENABLED
#error "FB_ENABLED must be defined"
#endif
#if defined(NEEDS_CXX20) && __cplusplus < 202002L
#error "needs C++20"
#endif
namespace api {
std::int32_t scaled(std::int32_t x);
}
"#,
    )
    .unwrap();
    fs::write(dir.join("src/api.cpp"), "#include \"api.h\"\n").unwrap();
    dir
}

fn write_flags(dir: &Path, lines: &[&str]) -> PathBuf {
    let path = dir.join("build/flags.txt");
    fs::write(&path, lines.join("\n")).unwrap();
    path
}

fn generate(dir: &Path, extra: &[String]) -> Output {
    let mut args = vec![
        "generate".to_owned(),
        text(&dir.join("lib/api.h")),
        "--out-dir".to_owned(),
        text(&dir.join("out")),
    ];
    args.extend(extra.iter().cloned());
    ferrobridge(&args)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_header_needing_flags_reads_with_a_flags_file() {
    let dir = project("flags-file");
    // Without flags, `config.h` is not found.
    let output = generate(&dir, &[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("config.h"), "{}", stderr(&output));

    let inc = format!("-I{}", dir.join("inc").display());
    let flags = write_flags(
        &dir,
        &[
            &inc,
            "-DFB_ENABLED=1",
            "-Wall",
            "-Werror",
            "-O2",
            "-Xclang",
            "-fno-validate-pch",
        ],
    );
    let output = generate(&dir, &["--flags-file".to_owned(), text(&flags)]);
    let err = stderr(&output);
    assert!(output.status.success(), "{err}");
    assert!(
        err.contains("ℹ flags: dropped 2 warning flags (-Wall, -Werror)"),
        "{err}"
    );
    assert!(
        err.contains("ℹ flags: dropped 1 optimization and debug flag (-O2)"),
        "{err}"
    );
    assert!(
        err.contains(
            "⚠ flags: dropped 1 flag ferrobridge does not know: -Xclang -fno-validate-pch"
        ),
        "{err}"
    );
    let bridge = fs::read_to_string(dir.join("out/bridge.rs")).unwrap();
    assert!(bridge.contains("fn scaled"), "{bridge}");
}

#[test]
fn arguments_after_the_separator_are_never_filtered() {
    let dir = project("separator");
    let output = generate(
        &dir,
        &[
            "--".to_owned(),
            format!("-I{}", dir.join("inc").display()),
            "-DFB_ENABLED".to_owned(),
            "-Xclang".to_owned(),
            "-fno-validate-pch".to_owned(),
            "-Wall".to_owned(),
        ],
    );
    let err = stderr(&output);
    assert!(output.status.success(), "{err}");
    assert!(!err.contains("flags:"), "{err}");
}

#[test]
fn a_compile_commands_entry_gives_the_flags() {
    let dir = project("compile-commands");
    let database = format!(
        r#"[{{
  "directory": {build:?},
  "command": "c++ -I../inc -DFB_ENABLED=1 -Wall -o api.o -c ../src/api.cpp",
  "file": "../src/api.cpp"
}}]"#,
        build = text(&dir.join("build")),
    );
    fs::write(dir.join("build/compile_commands.json"), database).unwrap();
    let output = generate(
        &dir,
        &[
            "--compile-commands".to_owned(),
            text(&dir.join("build")),
            "--for".to_owned(),
            text(&dir.join("src/api.cpp")),
        ],
    );
    let err = stderr(&output);
    assert!(output.status.success(), "{err}");
    assert!(
        err.contains("ℹ flags: dropped 1 warning flag (-Wall)"),
        "{err}"
    );
    // A depfile lists the database: a changed define or include path regenerates the bridge.
    let depfile = dir.join("bridge.d");
    let output = generate(
        &dir,
        &[
            "--compile-commands".to_owned(),
            text(&dir.join("build")),
            "--for".to_owned(),
            text(&dir.join("src/api.cpp")),
            "--depfile".to_owned(),
            text(&depfile),
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let deps = fs::read_to_string(&depfile).unwrap();
    assert!(
        deps.contains(&text(&dir.join("build/compile_commands.json"))),
        "{deps}"
    );

    // `--for` without `--compile-commands` (and the reverse) is a usage error.
    let output = generate(&dir, &["--for".to_owned(), "src/api.cpp".to_owned()]);
    assert!(!output.status.success());
    // An unknown source names the database and the closest entries.
    let output = generate(
        &dir,
        &[
            "--compile-commands".to_owned(),
            text(&dir.join("build/compile_commands.json")),
            "--for".to_owned(),
            text(&dir.join("lib/api.h")),
        ],
    );
    let err = stderr(&output);
    assert!(!output.status.success());
    assert!(err.contains("✗"), "{err}");
    assert!(err.contains("has no entry for"), "{err}");
    assert!(err.contains("api.cpp"), "{err}");
}

#[test]
fn an_explicit_std_wins_over_the_flag_sources() {
    let dir = project("std");
    let inc = format!("-I{}", dir.join("inc").display());
    let flags = write_flags(&dir, &[&inc, "-DFB_ENABLED", "-DNEEDS_CXX20", "-std=c++20"]);
    let flags_arg = ["--flags-file".to_owned(), text(&flags)];
    // `-std=c++20` from the file is used...
    let output = generate(&dir, &flags_arg);
    assert!(output.status.success(), "{}", stderr(&output));
    // ...unless `--std` says otherwise.
    let mut args = flags_arg.to_vec();
    args.extend(["--std".to_owned(), "c++17".to_owned()]);
    let output = generate(&dir, &args);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("needs C++20"),
        "{}",
        stderr(&output)
    );
    // Without any `-std=`, the default is C++17.
    let flags = write_flags(&dir, &[&inc, "-DFB_ENABLED", "-DNEEDS_CXX20"]);
    let output = generate(&dir, &["--flags-file".to_owned(), text(&flags)]);
    assert!(
        stderr(&output).contains("needs C++20"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn the_depfile_lists_every_header_the_bridge_depends_on() {
    let dir = project("depfile");
    let inc = format!("-I{}", dir.join("inc").display());
    let flags = write_flags(&dir, &[&inc, "-DFB_ENABLED"]);
    let config = dir.join("ferrobridge.toml");
    fs::write(&config, "").unwrap();
    let depfile = dir.join("deps/bridge.d");
    let output = generate(
        &dir,
        &[
            "--flags-file".to_owned(),
            text(&flags),
            "--config".to_owned(),
            text(&config),
            "--depfile".to_owned(),
            text(&depfile),
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let deps = fs::read_to_string(&depfile).unwrap();
    let target = format!("{}:", dir.join("out/bridge.rs").display());
    assert!(deps.starts_with(&target), "{deps}");
    for file in [
        dir.join("lib/api.h"),
        dir.join("inc/config.h"),
        config,
        flags,
    ] {
        assert!(
            deps.lines()
                .any(|line| line.trim().trim_end_matches(" \\") == text(&file)),
            "{} missing:\n{deps}",
            file.display()
        );
    }
    // A system header (through `<cstdint>`) is a dependency too.
    assert!(deps.contains("cstdint"), "{deps}");
}

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn compile_cpp(source: &Path, includes: &[&Path], out: &Path) -> Output {
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let mut command = Command::new(compiler);
    command
        .args(["-std=c++17", "-c"])
        .arg(source)
        .arg("-o")
        .arg(out);
    for dir in includes {
        command.arg("-I").arg(dir);
    }
    command.output().unwrap()
}

#[test]
fn the_cxx_glue_compiles_together_with_the_shim() {
    let dir = temp_dir("glue");
    let out = dir.join("out");
    let fixture = fixture_dir("classes");
    let generate = |force: bool| {
        let mut args = vec![
            "generate".to_owned(),
            text(&fixture.join("input.h")),
            "--out-dir".to_owned(),
            text(&out),
            "--cxx-glue".to_owned(),
        ];
        if force {
            args.push("--force".to_owned());
        }
        ferrobridge(&args)
    };
    let output = generate(false);
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    for file in ["bridge.rs.cc", "bridge.rs.h", "rust/cxx.h"] {
        let contents = fs::read_to_string(out.join(file)).unwrap();
        assert!(
            contents.starts_with("// @generated by ferrobridge") && contents.contains("cxx-gen"),
            "{file}"
        );
        assert!(stdout.contains(file), "{stdout}");
    }
    for source in ["shim.cc", "bridge.rs.cc"] {
        let output = compile_cpp(
            &out.join(source),
            &[&out, &fixture],
            &dir.join(format!("{source}.o")),
        );
        assert!(
            output.status.success(),
            "{source}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // A second run does not refuse the glue files it wrote itself...
    let output = generate(false);
    assert!(output.status.success(), "{}", stderr(&output));
    // ...but does refuse a hand-written one.
    fs::write(out.join("rust/cxx.h"), "// mine\n").unwrap();
    let output = generate(false);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("cxx.h"), "{}", stderr(&output));
    assert!(generate(true).status.success());
}
