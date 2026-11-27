//! End-to-end tests of the `ferrobridge generate` command.

// This file is test code only; the test exception in `clippy.toml` does not cover helper
// functions, so it is allowed explicitly here.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name).join("input.h")
}

/// A unique, clean output directory per test.
fn out_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("ferrobridge-cli-{}", std::process::id()))
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn ferrobridge(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferrobridge"))
        .args(args)
        .output()
        .unwrap()
}

/// The fixtures the e2e crate builds (the same list as `crates/ferrobridge-e2e/build.rs`).
const E2E_FIXTURES: [&str; 11] = [
    "classes",
    "enums",
    "strings",
    "ownership",
    "vectors",
    "slices",
    "inline_and_constants",
    "multi_header",
    "value_types",
    "operators",
    "no_exceptions",
];

/// All `.h` files in the fixture directory (not subdirectories), in name order: the bridge's
/// headers. Most fixtures have only `input.h`.
fn bridge_headers(name: &str) -> Vec<PathBuf> {
    let dir = root().join("tests/fixtures").join(name);
    let mut headers: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "h"))
        .collect();
    headers.sort();
    headers
}

/// Generates with the settings the `ferrobridge-e2e` crate uses. If the fixture has a
/// `ferrobridge.toml` next to it, it is passed with `--config`.
fn generate_like_e2e(name: &str, out: &Path) -> Output {
    let config = fixture(name).with_file_name("ferrobridge.toml");
    let headers = bridge_headers(name);
    let mut args = vec!["generate".to_owned()];
    args.extend(headers.iter().map(|h| h.to_str().unwrap().to_owned()));
    args.extend(["--out-dir".to_owned(), out.to_str().unwrap().to_owned()]);
    for header in &headers {
        let file = header.file_name().unwrap().to_str().unwrap();
        args.extend(["--include-as".to_owned(), format!("{name}/{file}")]);
    }
    args.extend([
        "--shim-include".to_owned(),
        format!("{name}/shim.h"),
        "--shim-namespace".to_owned(),
        format!("ferrobridge::{name}"),
    ]);
    if config.exists() {
        args.push("--config".to_owned());
        args.push(config.to_str().unwrap().to_owned());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    ferrobridge(&args)
}

/// Enables the commented-out values in the draft; for returns, picks the first option of each table.
/// Enables the first option for every table and key (the lines of the same key are alternatives).
/// The examples in the draft's annotation header (`#   { slice = "n" } ...`) are not keys.
fn uncomment_first_choices(draft: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut table = String::new();
    let mut lines = Vec::new();
    for line in draft.lines() {
        if line.starts_with('[') {
            table = line.to_owned();
            lines.push(line.to_owned());
            continue;
        }
        let choice = line.strip_prefix("# ").and_then(|value| {
            let (key, _) = value.split_once(" = ")?;
            let is_value = value.contains(" = \"") || value.contains(" = {");
            (is_value && !key.is_empty() && !key.contains(' ')).then_some((key, value))
        });
        match choice {
            Some((key, value)) if seen.insert((table.clone(), key.to_owned())) => {
                lines.push(value.to_owned());
            }
            _ => lines.push(line.to_owned()),
        }
    }
    lines.join("\n")
}

fn write_config(test: &str, text: &str) -> PathBuf {
    let dir = out_dir(test);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ferrobridge.toml");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn generate_writes_three_files_and_reports_skipped_items() {
    let out = out_dir("writes");
    let output = generate_like_e2e("classes", &out);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {stderr}");
    for file in ["bridge.rs", "shim.h", "shim.cc"] {
        assert!(out.join(file).is_file(), "{file} was not written");
    }
    assert!(
        stderr.contains("⚠ geo::Vehicle::total_km"),
        "stderr: {stderr}"
    );
    assert!(stdout.contains("bound"), "stdout: {stdout}");
}

/// Are the generator and the committed files in `ferrobridge-e2e` in sync? If this fails,
/// regenerate the files with the command in `ferrobridge-e2e/README.md`.
#[test]
fn generated_files_match_the_committed_e2e_files() {
    for name in E2E_FIXTURES {
        let out = out_dir(&format!("sync-{name}"));
        let output = generate_like_e2e(name, &out);
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let committed = root()
            .join("crates/ferrobridge-e2e/src/generated")
            .join(name);
        for file in ["bridge.rs", "shim.h", "shim.cc"] {
            let fresh = fs::read_to_string(out.join(file)).unwrap();
            let stored = fs::read_to_string(committed.join(file)).unwrap_or_default();
            assert!(
                fresh == stored,
                "{name}/{file} differs from the committed file; regenerate the e2e files"
            );
        }
    }
}

#[test]
fn defaults_derive_include_and_namespace_from_the_header() {
    let out = out_dir("defaults");
    let header = fixture("free_functions");
    let output = ferrobridge(&[
        "generate",
        header.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shim_h = fs::read_to_string(out.join("shim.h")).unwrap();
    assert!(shim_h.contains("#include \"input.h\""), "{shim_h}");
    assert!(
        shim_h.contains("namespace ferrobridge::input {"),
        "{shim_h}"
    );
    let bridge = fs::read_to_string(out.join("bridge.rs")).unwrap();
    assert!(bridge.contains("include!(\"shim.h\")"), "{bridge}");
}

#[test]
fn invalid_shim_namespace_is_rejected() {
    let out = out_dir("invalid-ns");
    let header = fixture("free_functions");
    let output = ferrobridge(&[
        "generate",
        header.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--shim-namespace",
        "not valid",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("namespace"));
    assert!(!out.join("bridge.rs").exists());
}

#[test]
fn refuses_to_overwrite_files_it_did_not_generate() {
    let out = out_dir("overwrite");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("bridge.rs"), "// hand-written bridge\n").unwrap();
    let header = fixture("free_functions");
    let args = [
        "generate",
        header.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
    ];

    let output = ferrobridge(&args);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--force"));
    assert_eq!(
        fs::read_to_string(out.join("bridge.rs")).unwrap(),
        "// hand-written bridge\n"
    );

    // It overwrites the files it generated itself.
    let mut forced = args.to_vec();
    forced.push("--force");
    assert!(ferrobridge(&forced).status.success());
    assert!(ferrobridge(&args).status.success());
}

#[test]
fn suggest_prints_a_draft_that_resolves_the_warnings_once_filled() {
    let header = fixture("classes");
    let output = ferrobridge(&["suggest", header.to_str().unwrap()]);
    let draft = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        draft.contains("[functions.\"geo::Vehicle::read_odometer\".params]"),
        "{draft}"
    );
    assert!(draft.contains("# out = \"borrowed\""), "{draft}");
    // `void*` is not solved by an annotation: it does not enter the draft.
    assert!(!draft.contains("raw_handle"), "{draft}");

    let config = write_config("suggest-filled", &uncomment_first_choices(&draft));
    let out = out_dir("suggest-generate");
    let output = ferrobridge(&[
        "generate",
        header.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!stderr.contains("read_odometer"), "stderr: {stderr}");
    assert!(stderr.contains("raw_handle"), "stderr: {stderr}");
    let bridge = fs::read_to_string(out.join("bridge.rs")).unwrap();
    assert!(
        bridge.contains("fn raw_read_odometer(self: &Vehicle, out: &mut f64)"),
        "{bridge}"
    );
    assert!(
        bridge.contains("pub fn read_odometer(&self, out: &mut f64)"),
        "{bridge}"
    );
    assert!(
        bridge.contains("Assumptions based on annotations (1)"),
        "{bridge}"
    );

    // Asked again with the existing annotations, there is nothing left to say.
    let output = ferrobridge(&[
        "suggest",
        header.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "# Nothing needs an annotation.\n"
    );
}

#[test]
fn annotations_that_do_not_match_the_header_fail_with_lines() {
    let config = write_config(
        "bad-config",
        "[functions.\"geo::Vehicle::read_odometr\".params]\nout = \"borrowed\"\n",
    );
    let out = out_dir("bad-config-out");
    let output = ferrobridge(&[
        "generate",
        fixture("classes").to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains(
            "ferrobridge.toml:1: `geo::Vehicle::read_odometr` was not found in the header"
        ),
        "stderr: {stderr}"
    );
    assert!(
        !out.join("bridge.rs").exists(),
        "no file must be written on error"
    );
}

#[test]
fn malformed_annotations_fail_with_lines() {
    let config = write_config("typo-config", "[functions.\"f\"]\nreturns = \"owend\"\n");
    let output = ferrobridge(&[
        "suggest",
        fixture("classes").to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("ferrobridge.toml:2: "), "stderr: {stderr}");
    assert!(stderr.contains("owend"), "stderr: {stderr}");

    let output = ferrobridge(&[
        "suggest",
        fixture("classes").to_str().unwrap(),
        "--config",
        "/missing/ferrobridge.toml",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read"));
}

#[test]
fn unnecessary_annotations_are_warned_about() {
    let config = write_config(
        "unneeded-config",
        "[functions.\"geo::Vehicle::drive\".params]\nkm = \"borrowed\"\n",
    );
    let out = out_dir("unneeded-out");
    let output = ferrobridge(&[
        "generate",
        fixture("classes").to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stderr.contains("⚠ ferrobridge.toml:2: `geo::Vehicle::drive` parameter `km`"),
        "stderr: {stderr}"
    );
}
