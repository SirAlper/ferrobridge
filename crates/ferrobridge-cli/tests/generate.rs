//! `ferrobridge generate` komutunun uçtan uca testleri.

// Bu dosya yalnızca test kodu; `clippy.toml`'daki test istisnası yardımcı fonksiyonları
// kapsamadığı için burada açıkça izin veriliyor.
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

/// Test başına benzersiz, temiz bir çıktı dizini.
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

/// `ferrobridge-e2e` crate'inin kullandığı ayarlarla üretir.
fn generate_like_e2e(name: &str, out: &Path) -> Output {
    ferrobridge(&[
        "generate",
        fixture(name).to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--include-as",
        &format!("{name}/input.h"),
        "--shim-include",
        &format!("{name}/shim.h"),
        "--shim-namespace",
        &format!("ferrobridge::{name}"),
    ])
}

#[test]
fn generate_writes_three_files_and_reports_skipped_items() {
    let out = out_dir("writes");
    let output = generate_like_e2e("classes", &out);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stderr: {stderr}");
    for file in ["bridge.rs", "shim.h", "shim.cc"] {
        assert!(out.join(file).is_file(), "{file} yazılmadı");
    }
    assert!(
        stderr.contains("⚠ geo::Vehicle::total_km"),
        "stderr: {stderr}"
    );
    assert!(stdout.contains("bağlandı"), "stdout: {stdout}");
}

/// Üretici ile `ferrobridge-e2e`'deki commit'li dosyalar senkron mu? Başarısız olursa
/// `ferrobridge-e2e/README.md`'deki komutla dosyaları yeniden üretin.
#[test]
fn generated_files_match_the_committed_e2e_files() {
    for name in ["classes", "enums", "strings"] {
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
                "{name}/{file} commit'li dosyadan farklı; e2e dosyalarını yeniden üretin"
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
    fs::write(out.join("bridge.rs"), "// elle yazılmış bridge\n").unwrap();
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
        "// elle yazılmış bridge\n"
    );

    // Kendi ürettiği dosyaların üzerine yazar.
    let mut forced = args.to_vec();
    forced.push("--force");
    assert!(ferrobridge(&forced).status.success());
    assert!(ferrobridge(&args).status.success());
}
