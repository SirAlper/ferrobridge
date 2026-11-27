//! ferrobridge bridge'lerini kullanıcının `build.rs` dosyasından derler.
//!
//! İki kullanım var:
//!
//! - **Commit'li dosyalar (varsayılan).** `ferrobridge generate` ile üretilip depoya konmuş
//!   `bridge.rs` / `shim.h` / `shim.cc` derlenir (ADR-0004). libclang gerekmez.
//! - **Derleme sırasında üretim (`generate` özelliği).** Header `build.rs` içinde libclang ile
//!   okunur, bridge `$OUT_DIR/ferrobridge/<ad>/` altına üretilip derlenir. Rust tarafında
//!   `ferrobridge::include_bridge!("<ad>")` ile kullanılır. Üretilmiş dosyaların depoya
//!   girmemesi gerektiğinde (ör. lisansı farklı bir C++ kütüphanesi; ADR-0008) bu yol seçilir.
//!
//! ```no_run
//! // build.rs: `fn main` gövdesi
//! let mut build = ferrobridge_build::Build::new();
//! build
//!     .committed("src/generated/vehicle")
//!     .include_dir("src/generated")
//!     .include_dir("cpp/include")
//!     .file("cpp/vehicle.cc");
//! if let Err(error) = build.compile("vehicle") {
//!     panic!("{error}");
//! }
//! ```
//!
//! Bütün bridge'ler, kullanıcının C++ kaynakları ve shim'ler tek bir statik kütüphanede, aynı
//! include yolları, define'lar ve C++ standardıyla derlenir. `generate` modunda aynı ayarlar
//! header'ı okuyan libclang'a da geçirilir.

use std::path::{Path, PathBuf};

/// Varsayılan C++ standardı (cxx C++11 ister, ferrobridge'in shim'leri C++17).
const DEFAULT_STD: &str = "c++17";
/// Commit'li bir bridge dizininde bulunması gereken dosyalar.
const BRIDGE_FILES: [&str; 3] = ["bridge.rs", "shim.h", "shim.cc"];

/// `build.rs`'te derlemeyi durduran bir hata. `Display` bütün ayrıntıyı (libclang tanıları,
/// `ferrobridge.toml` satırları) çok satırlı olarak verir.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Commit'li bir bridge dizininde beklenen dosya yok.
    #[error(
        "{} bulunamadı: dizinde `ferrobridge generate`'in ürettiği `bridge.rs`, `shim.h` ve \
         `shim.cc` olmalı",
        .0.display()
    )]
    MissingFile(PathBuf),
    /// Cargo'nun `build.rs`'e verdiği bir ortam değişkeni yok.
    #[error("`{0}` ortam değişkeni yok; `compile` yalnızca `build.rs` içinden çağrılabilir")]
    MissingEnv(&'static str),
    /// Bridge adı dosya ve C++ namespace adı olarak kullanılamıyor.
    #[error(
        "geçersiz bridge adı `{0}`: harf ya da `_` ile başlayan, harf, rakam ve `_`'dan oluşan bir ad olmalı"
    )]
    InvalidName(String),
    /// Üretilen dosya yazılamadı.
    #[error("{} yazılamadı: {message}", path.display())]
    Write {
        /// Dosya.
        path: PathBuf,
        /// İşletim sisteminin hata mesajı.
        message: String,
    },
    /// Header okunamadı, açıklamalar geçersiz ya da kod üretilemedi.
    #[error("ferrobridge: {header}: {message}")]
    Generate {
        /// Header'ın yolu.
        header: String,
        /// Ne oldu; birden fazla satır olabilir.
        message: String,
    },
}

/// Bir ya da daha fazla bridge'in ve kullanıcının C++ kaynaklarının derlenmesi.
#[derive(Debug, Clone, Default)]
pub struct Build {
    committed: Vec<PathBuf>,
    #[cfg(feature = "generate")]
    generated: Vec<Generate>,
    include_dirs: Vec<PathBuf>,
    defines: Vec<(String, Option<String>)>,
    files: Vec<PathBuf>,
    std: Option<String>,
    flags: Vec<String>,
}

impl Build {
    /// Boş bir derleme.
    pub fn new() -> Self {
        Self::default()
    }

    /// `ferrobridge generate` çıktısının commit'lendiği dizin (`bridge.rs`, `shim.h`,
    /// `shim.cc`). Üretim sırasında verilen `--include-as` ve `--shim-include` yollarının
    /// çözüleceği kökler [`Build::include_dir`] ile ayrıca verilmeli.
    pub fn committed(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.committed.push(dir.as_ref().to_path_buf());
        self
    }

    /// Derleme sırasında üretilecek bir bridge (`generate` özelliği).
    #[cfg(feature = "generate")]
    pub fn generate(&mut self, bridge: Generate) -> &mut Self {
        self.generated.push(bridge);
        self
    }

    /// C++ include dizini (`-I`). `generate` modunda libclang'a da geçer.
    pub fn include_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.include_dirs.push(dir.as_ref().to_path_buf());
        self
    }

    /// Önişlemci tanımı (`-DNAME` ya da `-DNAME=değer`). `generate` modunda libclang'a da
    /// geçer.
    pub fn define(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        self.defines
            .push((name.to_owned(), value.map(str::to_owned)));
        self
    }

    /// Derlenecek kullanıcı C++ kaynağı (bağlanan kütüphanenin gerçeklemesi).
    pub fn file(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.files.push(path.as_ref().to_path_buf());
        self
    }

    /// Birden fazla C++ kaynağı.
    pub fn files<P: AsRef<Path>>(&mut self, paths: impl IntoIterator<Item = P>) -> &mut Self {
        for path in paths {
            self.file(path);
        }
        self
    }

    /// C++ standardı (varsayılan `c++17`). `generate` modunda libclang'a da geçer.
    pub fn std(&mut self, std: &str) -> &mut Self {
        self.std = Some(std.to_owned());
        self
    }

    /// Yalnızca C++ derleyicisine geçen ek argüman (libclang'a geçmez).
    pub fn flag(&mut self, flag: &str) -> &mut Self {
        self.flags.push(flag.to_owned());
        self
    }

    fn cpp_std(&self) -> &str {
        self.std.as_deref().unwrap_or(DEFAULT_STD)
    }

    /// Bridge'leri (gerekiyorsa önce üretip) ve C++ kaynaklarını `lib_name` adlı statik
    /// kütüphaneye derler, Cargo'ya bağlamasını söyler.
    ///
    /// # Errors
    ///
    /// Commit'li bir dosya eksikse, `generate` modunda header okunamazsa, `ferrobridge.toml`
    /// header'la uyuşmazsa ya da dosyalar yazılamazsa hata döner. C++ derleme hataları
    /// `cxx-build` tarafından basılır ve derlemeyi durdurur.
    pub fn compile(&self, lib_name: &str) -> Result<(), Error> {
        let mut bridges = Vec::new();
        let mut shims = Vec::new();
        #[cfg_attr(not(feature = "generate"), allow(unused_mut))]
        let mut include_dirs = self.include_dirs.clone();

        for dir in &self.committed {
            for file in BRIDGE_FILES {
                let path = dir.join(file);
                if !path.is_file() {
                    return Err(Error::MissingFile(path));
                }
                // `bridge.rs` için satırı `cxx-build` basar.
                if file != "bridge.rs" {
                    rerun_if_changed(&path);
                }
            }
            bridges.push(dir.join("bridge.rs"));
            shims.push(dir.join("shim.cc"));
        }

        #[cfg(feature = "generate")]
        if !self.generated.is_empty() {
            let out_dir = std::env::var_os("OUT_DIR").ok_or(Error::MissingEnv("OUT_DIR"))?;
            let root = PathBuf::from(out_dir).join("ferrobridge");
            for bridge in &self.generated {
                let generated = generate::run(self, bridge, &root)?;
                bridges.push(generated.dir.join("bridge.rs"));
                shims.push(generated.dir.join("shim.cc"));
                include_dirs.extend(generated.include_dirs);
            }
            // `<ad>/shim.h` buradan çözülür.
            include_dirs.push(root);
        }

        for path in &self.files {
            rerun_if_changed(path);
        }
        for dir in &self.include_dirs {
            // Cargo dizinleri kendisi tarar: include edilen bir header değişince yeniden derler.
            rerun_if_changed(dir);
        }

        let mut build = cxx_build::bridges(&bridges);
        build.files(&shims).files(&self.files);
        for dir in &include_dirs {
            build.include(dir);
        }
        for (name, value) in &self.defines {
            build.define(name, value.as_deref());
        }
        build.std(self.cpp_std());
        for flag in &self.flags {
            build.flag(flag);
        }
        build.compile(lib_name);
        Ok(())
    }
}

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

/// Bridge adı: dosya yolu parçası, C++ namespace'i ve Rust makro argümanı olarak kullanılır.
#[cfg(feature = "generate")]
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// İçerik farklıysa yazar. Aynı içeriği yeniden yazmak dosya zamanını değiştirip gereksiz C++
/// derlemesine yol açardı.
#[cfg(feature = "generate")]
fn write_if_changed(path: &Path, contents: &str) -> Result<(), Error> {
    use std::fs;
    if fs::read_to_string(path).is_ok_and(|old| old == contents) {
        return Ok(());
    }
    let error = |e: std::io::Error| Error::Write {
        path: path.to_path_buf(),
        message: e.to_string(),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error)?;
    }
    fs::write(path, contents).map_err(error)
}

#[cfg(feature = "generate")]
pub use generate::Generate;

#[cfg(feature = "generate")]
mod generate {
    use std::path::{Path, PathBuf};

    use ferrobridge_analysis::{Annotations, BridgePlan};
    use ferrobridge_codegen::CodegenOptions;
    use ferrobridge_parser::{ParseOptions, ParserError};

    use crate::{Build, Error, rerun_if_changed, valid_name, write_if_changed};

    /// `build.rs` içinde üretilecek bir bridge: hangi header'lardan, hangi adla, hangi
    /// açıklamalarla. Bir header'da tanımlı tip, aynı bridge'in diğer header'larındaki
    /// fonksiyonlarda kullanılabilir.
    #[derive(Debug, Clone)]
    pub struct Generate {
        /// (header, shim'deki `#include` yolu; `None` ise dosya adı)
        headers: Vec<(PathBuf, Option<String>)>,
        name: String,
        config: Option<PathBuf>,
        prelude: Vec<String>,
        clang_args: Vec<String>,
    }

    impl Generate {
        /// `name` adlı bir bridge. Ad; çıktı dizini (`$OUT_DIR/ferrobridge/<ad>/`), shim
        /// namespace'i (`ferrobridge::<ad>`) ve `ferrobridge::include_bridge!("<ad>")` için
        /// kullanılır. Header'lar [`Generate::header`] / [`Generate::header_as`] ile eklenir.
        pub fn new(name: &str) -> Self {
            Self {
                headers: Vec::new(),
                name: name.to_owned(),
                config: None,
                prelude: Vec::new(),
                clang_args: Vec::new(),
            }
        }

        /// Bridge'e bir header ekler. Shim onu dosya adıyla include eder; header'ın dizini
        /// include yollarına eklenir.
        #[must_use]
        pub fn header(mut self, path: impl AsRef<Path>) -> Self {
            self.headers.push((path.as_ref().to_path_buf(), None));
            self
        }

        /// Bridge'e bir header ekler; shim onu `include` yoluyla (ör. `"AP_Math/crc.h"`)
        /// include eder. Yol [`Build::include_dir`] köklerinden çözülmeli.
        #[must_use]
        pub fn header_as(mut self, path: impl AsRef<Path>, include: &str) -> Self {
            self.headers
                .push((path.as_ref().to_path_buf(), Some(include.to_owned())));
            self
        }

        /// Sahiplik açıklamaları (`ferrobridge.toml`).
        #[must_use]
        pub fn config(mut self, path: impl AsRef<Path>) -> Self {
            self.config = Some(path.as_ref().to_path_buf());
            self
        }

        /// Header'dan önce include edilecek dosya (kendi başına derlenmeyen header'lar için;
        /// `ferrobridge generate --prelude` gibi). Shim de header'dan önce bunu include eder.
        #[must_use]
        pub fn prelude(mut self, include: &str) -> Self {
            self.prelude.push(include.to_owned());
            self
        }

        /// Yalnızca libclang'a geçen ek argüman.
        #[must_use]
        pub fn clang_arg(mut self, arg: &str) -> Self {
            self.clang_args.push(arg.to_owned());
            self
        }
    }

    /// Üretilen bir bridge.
    pub(crate) struct Generated {
        /// `bridge.rs`, `shim.h`, `shim.cc`'nin dizini.
        pub(crate) dir: PathBuf,
        /// C++ derlemesine eklenmesi gereken include dizinleri.
        pub(crate) include_dirs: Vec<PathBuf>,
    }

    pub(crate) fn run(build: &Build, bridge: &Generate, root: &Path) -> Result<Generated, Error> {
        if !valid_name(&bridge.name) {
            return Err(Error::InvalidName(bridge.name.clone()));
        }
        let header_text = bridge
            .headers
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let fail = |message: String| Error::Generate {
            header: header_text.clone(),
            message,
        };
        if bridge.headers.is_empty() {
            return Err(fail(format!(
                "`{}` bridge'ine hiç header eklenmedi (`Generate::header`)",
                bridge.name
            )));
        }
        let paths: Vec<&Path> = bridge.headers.iter().map(|(p, _)| p.as_path()).collect();
        for path in &paths {
            rerun_if_changed(path);
        }

        // Açıklamalar önce: `[templates]` parser'a örnekleme olarak gider.
        let annotations = match &bridge.config {
            Some(path) => {
                rerun_if_changed(path);
                Annotations::from_file(path).map_err(|e| fail(e.to_string()))?
            }
            None => Annotations::default(),
        };
        let mut options = parse_options(build, bridge);
        options.instantiate = annotations.instantiations();
        let module = ferrobridge_parser::parse_headers(&paths, &options)
            .map_err(|error| fail(describe_parser_error(&error)))?;
        let plan = ferrobridge_analysis::check(&module, &annotations).map_err(|issues| {
            let lines: Vec<String> = issues.iter().map(|i| format!("  {i}")).collect();
            fail(format!(
                "`ferrobridge.toml` header'larla uyuşmuyor:\n{}",
                lines.join("\n")
            ))
        })?;

        let mut include_dirs = Vec::new();
        let mut header_includes = Vec::with_capacity(bridge.headers.len());
        for (path, include) in &bridge.headers {
            header_includes.push(match include {
                Some(include) => include.clone(),
                None => {
                    if let Some(parent) = path.parent() {
                        include_dirs.push(parent.to_path_buf());
                    }
                    path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                }
            });
        }
        let options = CodegenOptions {
            header_includes,
            shim_include: format!("{}/shim.h", bridge.name),
            shim_namespace: format!("ferrobridge::{}", bridge.name),
            prelude: bridge.prelude.clone(),
        };
        let files =
            ferrobridge_codegen::generate(&plan, &options).map_err(|e| fail(e.to_string()))?;

        let dir = root.join(&bridge.name);
        write_if_changed(&dir.join("bridge.rs"), &files.bridge_rs)?;
        write_if_changed(&dir.join("shim.h"), &files.shim_h)?;
        write_if_changed(&dir.join("shim.cc"), &files.shim_cc)?;

        for warning in &plan.annotation_warnings {
            println!("cargo:warning=ferrobridge: {warning}");
        }
        let label = match bridge.headers.as_slice() {
            [(path, _)] => path.display().to_string(),
            many => format!("`{}` ({} header)", bridge.name, many.len()),
        };
        summary(&plan, &label, &dir.join("bridge.rs"));
        Ok(Generated { dir, include_dirs })
    }

    /// libclang'a geçen ayarlar: derlemenin include dizinleri, define'ları ve standardı.
    pub(crate) fn parse_options(build: &Build, bridge: &Generate) -> ParseOptions {
        let mut extra_args = Vec::new();
        for dir in &build.include_dirs {
            extra_args.push(format!("-I{}", dir.display()));
        }
        for (name, value) in &build.defines {
            extra_args.push(match value {
                Some(value) => format!("-D{name}={value}"),
                None => format!("-D{name}"),
            });
        }
        extra_args.extend(bridge.clang_args.iter().cloned());
        ParseOptions {
            std: build.cpp_std().to_owned(),
            extra_args,
            prelude: bridge.prelude.clone(),
            instantiate: Vec::new(),
        }
    }

    fn describe_parser_error(error: &ParserError) -> String {
        match error {
            ParserError::ParseFailed { diagnostics } => {
                let lines: Vec<String> = diagnostics.iter().map(|d| format!("  {d}")).collect();
                format!("{error}\n{}", lines.join("\n"))
            }
            ParserError::LoadFailed(_) => format!(
                "{error}\n  `LIBCLANG_PATH` libclang'ın bulunduğu dizini göstermeli (macOS: \
                 `$(brew --prefix llvm)/lib`)."
            ),
            other => other.to_string(),
        }
    }

    /// Bağlanmayan öğe varsa tek bir özet uyarısı: tek tek basmak büyük kütüphanelerde yüzlerce
    /// satır olur; liste üretilen dosyanın başında.
    fn summary(plan: &BridgePlan, header: &str, bridge_rs: &Path) {
        if plan.skipped.is_empty() && plan.suggestions.is_empty() {
            return;
        }
        let mut message = format!(
            "cargo:warning=ferrobridge: {header}: {} öğe bağlanmadı (liste: {})",
            plan.skipped.len(),
            bridge_rs.display()
        );
        if !plan.suggestions.is_empty() {
            message.push_str(&format!(
                "; {} yer açıklama bekliyor (`ferrobridge suggest`)",
                plan.suggestions.len()
            ));
        }
        println!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "generate")]
    #[test]
    fn bridge_names_must_be_identifiers() {
        assert!(valid_name("ap_math"));
        assert!(valid_name("_x2"));
        assert!(!valid_name(""));
        assert!(!valid_name("2x"));
        assert!(!valid_name("ap-math"));
        assert!(!valid_name("a/b"));
    }

    #[cfg(feature = "generate")]
    #[test]
    fn unchanged_files_are_not_rewritten() {
        use std::fs;
        let dir = std::env::temp_dir().join(format!("ferrobridge-build-{}", std::process::id()));
        let path = dir.join("x/bridge.rs");
        write_if_changed(&path, "a").unwrap();
        let first = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_if_changed(&path, "a").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), first);
        write_if_changed(&path, "b").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "b");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_committed_file_is_reported_before_compiling() {
        let error = Build::new()
            .committed("/olmayan/bridge/dizini")
            .compile("x")
            .unwrap_err();
        assert!(matches!(&error, Error::MissingFile(p) if p.ends_with("bridge.rs")));
        assert!(error.to_string().contains("ferrobridge generate"));
    }

    #[cfg(feature = "generate")]
    #[test]
    fn libclang_sees_the_same_include_dirs_defines_and_std() {
        let mut build = Build::new();
        build
            .include_dir("libraries")
            .define("CONFIG_HAL_BOARD", Some("HAL_BOARD_SITL"))
            .define("NDEBUG", None)
            .std("c++20")
            .flag("-w");
        let bridge = Generate::new("crc")
            .header_as("libraries/AP_Math/crc.h", "AP_Math/crc.h")
            .prelude("AP_Math/AP_Math.h")
            .clang_arg("-Wno-everything");
        let options = generate::parse_options(&build, &bridge);
        assert_eq!(options.std, "c++20");
        assert_eq!(
            options.extra_args,
            [
                "-Ilibraries",
                "-DCONFIG_HAL_BOARD=HAL_BOARD_SITL",
                "-DNDEBUG",
                "-Wno-everything"
            ]
        );
        assert_eq!(options.prelude, ["AP_Math/AP_Math.h"]);
    }
}
