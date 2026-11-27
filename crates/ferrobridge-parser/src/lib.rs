//! C++ header'larını libclang ile okuyup IR'a çeviren katman.
//!
//! libclang'ın C API'sine `clang-sys` üzerinden erişilir (bkz.
//! `docs/adr/0001-libclang-via-clang-sys.md`). Kütüphane derleme anında değil, çalışma anında
//! yüklenir (`clang-sys` `runtime` özelliği). Böylece libclang bulunamazsa program açılışta
//! çökmez; anlaşılır bir hata döner.
//!
//! Giriş noktası [`parse_header`].

mod clang;
mod lower;

use std::ffi::CString;
use std::fmt;
use std::path::{Path, PathBuf};

use ferrobridge_ir::Module;

/// Header ayrıştırma ayarları.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    /// C++ standardı; clang'a `-std=<değer>` olarak geçer.
    pub std: String,
    /// clang'a olduğu gibi geçirilecek ek argümanlar (ör. `-I include`, `-DFOO=1`).
    pub extra_args: Vec<String>,
    /// Header'dan önce, sırayla include edilecek dosyalar (clang'ın `-include` argümanı gibi:
    /// önce çalışma dizinine, sonra include yollarına göre çözülür). Kendi başına derlenmeyen
    /// header'lar için. Bu dosyalardaki bildirimler IR'a alınmaz.
    pub prelude: Vec<String>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            std: "c++17".to_owned(),
            extra_args: Vec::new(),
            prelude: Vec::new(),
        }
    }
}

/// libclang'ın bildirdiği bir hata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Hatanın bulunduğu dosya; libclang konum veremezse boş.
    pub file: String,
    /// 1'den başlayan satır numarası.
    pub line: u32,
    /// 1'den başlayan sütun numarası.
    pub column: u32,
    /// libclang'ın hata mesajı.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(
                f,
                "{}:{}:{}: {}",
                self.file, self.line, self.column, self.message
            )
        }
    }
}

/// Parser katmanının hataları.
#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    /// libclang dinamik kütüphanesi bulunamadı veya yüklenemedi.
    #[error("libclang yüklenemedi: {0}")]
    LoadFailed(String),
    /// libclang yüklendi ama sürüm bilgisi okunamadı.
    #[error("libclang sürüm bilgisi okunamadı")]
    VersionUnavailable,
    /// Verilen header dosyası yok.
    #[error("header bulunamadı: {}", .0.display())]
    FileNotFound(PathBuf),
    /// Yol veya argüman libclang'a geçirilemiyor (UTF-8 değil ya da NUL karakteri içeriyor).
    #[error("libclang'a geçirilemeyen argüman: {0}")]
    InvalidArgument(String),
    /// libclang dosyayı hiç açamadı.
    #[error("libclang header'ı açamadı (hata kodu {0})")]
    ClangFailed(i32),
    /// Header'da derleme hataları var.
    #[error("header ayrıştırılamadı: {} hata", diagnostics.len())]
    ParseFailed {
        /// Hata ve ölümcül hata seviyesindeki tanılar. Uyarılar dahil değildir.
        diagnostics: Vec<Diagnostic>,
    },
}

/// libclang bu iş parçacığında yüklü değilse yükler.
fn ensure_loaded() -> Result<(), ParserError> {
    if clang_sys::is_loaded() {
        return Ok(());
    }
    clang_sys::load().map_err(ParserError::LoadFailed)
}

/// Yüklenen libclang'ın sürüm metnini döndürür (ör. `"Homebrew clang version 23.1.6"`).
///
/// # Errors
///
/// libclang bulunamazsa [`ParserError::LoadFailed`], sürüm metni okunamazsa
/// [`ParserError::VersionUnavailable`] döner.
pub fn libclang_version() -> Result<String, ParserError> {
    ensure_loaded()?;
    let version = clang::version();
    if version.is_empty() {
        Err(ParserError::VersionUnavailable)
    } else {
        Ok(version)
    }
}

/// Bir C++ header'ını okur ve içindeki bildirimleri IR'a çevirir.
///
/// Yalnızca `header` dosyasının kendi bildirimleri alınır; `#include` edilen dosyalardakiler ve
/// [`ParseOptions::prelude`] alınmaz. Desteklenmeyen yapılar atılmaz, IR'da nedenleriyle
/// birlikte yer alır.
///
/// Header libclang'a doğrudan "ana dosya" olarak verilmez; onu `#include` eden, bellekte
/// üretilmiş bir çeviri birimi okunur. Böylece header'ın `#pragma once` koruması, header kendini
/// dolaylı olarak yeniden include ettiğinde de çalışır (ana dosyada `#pragma once` işlemez).
///
/// # Errors
///
/// Dosya yoksa [`ParserError::FileNotFound`], libclang yüklenemezse
/// [`ParserError::LoadFailed`], header'da derleme hatası varsa [`ParserError::ParseFailed`]
/// döner.
pub fn parse_header(header: &Path, options: &ParseOptions) -> Result<Module, ParserError> {
    if !header.is_file() {
        return Err(ParserError::FileNotFound(header.to_path_buf()));
    }
    let header = std::fs::canonicalize(header)
        .map_err(|_| ParserError::FileNotFound(header.to_path_buf()))?;
    let header_text = header
        .to_str()
        .ok_or_else(|| ParserError::InvalidArgument(header.display().to_string()))?;
    let unit = synthetic_unit(header_text)?;
    let header_c = to_c_string(header_text)?;
    // Kaynak dizini yüklenen kütüphanenin yolundan bulunduğu için önce yükle.
    ensure_loaded()?;
    let args = clang_args(options)?;

    let index = clang::Index::new().ok_or(ParserError::ClangFailed(clang_sys::CXError_Failure))?;
    let tu = clang::TranslationUnit::parse(
        &index,
        &unit.name,
        &args,
        &[(unit.name.clone(), unit.contents)],
    )
    .map_err(ParserError::ClangFailed)?;

    let unit_name = unit.name.to_string_lossy();
    let errors: Vec<Diagnostic> = tu
        .diagnostics()
        .into_iter()
        .filter(|d| d.severity >= clang_sys::CXDiagnostic_Error)
        .map(|d| user_facing(d, &unit_name))
        .collect();
    if !errors.is_empty() {
        return Err(ParserError::ParseFailed {
            diagnostics: errors,
        });
    }

    // Header ayrıştırıldıysa çeviri biriminde mutlaka vardır.
    let target = tu
        .file(&header_c)
        .ok_or(ParserError::ClangFailed(clang_sys::CXError_Failure))?;
    Ok(lower::lower(&tu, target))
}

/// Tanıyı kullanıcının gördüğü dosyalara göre konumlandırır.
///
/// Konumu sentetik çeviri biriminde olan bir tanı (ör. header'da kapanmamış bir parantez dosya
/// sonunda fark edilir, dosya sonu da artık sentetik birimdedir) kullanıcıya bir şey söylemez.
/// Başka bir dosyayı gösteren ilk notu varsa ("to match this '('") tanı oraya taşınır ve notun
/// metni mesaja eklenir. Yoksa olduğu gibi bırakılır; hiçbir tanı atılmaz.
fn user_facing(diagnostic: clang::RawDiagnostic, unit_name: &str) -> Diagnostic {
    let clang::RawDiagnostic {
        location,
        message,
        notes,
        ..
    } = diagnostic;
    if location.file == unit_name {
        if let Some(note) = notes
            .into_iter()
            .find(|n| !n.location.file.is_empty() && n.location.file != unit_name)
        {
            return Diagnostic {
                file: note.location.file,
                line: note.location.line,
                column: note.location.column,
                message: format!("{message} ({})", note.message),
            };
        }
    }
    Diagnostic {
        file: location.file,
        line: location.line,
        column: location.column,
        message,
    }
}

/// Header'ı include eden, bellekte üretilmiş çeviri birimi.
struct SyntheticUnit {
    /// Diskte olmayan dosyanın adı: header'ın yanında, böylece tanılardaki yol anlaşılır olur.
    name: CString,
    contents: CString,
}

/// `header` (mutlak yol) için `#include "<header>"` içeren çeviri birimi.
///
/// `#include "..."` içinde kaçış dizisi yoktur; `"` ya da satır sonu içeren yollar bu satıra
/// yazılamaz ve reddedilir.
fn synthetic_unit(header: &str) -> Result<SyntheticUnit, ParserError> {
    if header.contains(['"', '\n', '\r']) {
        return Err(ParserError::InvalidArgument(header.to_owned()));
    }
    Ok(SyntheticUnit {
        name: to_c_string(&format!("{header}.ferrobridge.cc"))?,
        contents: to_c_string(&format!("#include \"{header}\"\n"))?,
    })
}

fn to_c_string(text: &str) -> Result<CString, ParserError> {
    CString::new(text).map_err(|_| ParserError::InvalidArgument(text.to_owned()))
}

/// libclang'a geçirilecek derleyici argümanları.
fn clang_args(options: &ParseOptions) -> Result<Vec<CString>, ParserError> {
    let mut args = vec![
        "-x".to_owned(),
        "c++".to_owned(),
        format!("-std={}", options.std),
    ];
    if let Some(sysroot) = default_sysroot(&options.extra_args) {
        args.push("-isysroot".to_owned());
        args.push(sysroot);
    }
    if let Some(resource_dir) = default_resource_dir(&options.extra_args) {
        args.push("-resource-dir".to_owned());
        args.push(resource_dir);
    }
    args.extend(options.extra_args.iter().cloned());
    for file in &options.prelude {
        args.push("-include".to_owned());
        args.push(file.clone());
    }
    args.iter().map(|arg| to_c_string(arg)).collect()
}

/// Derleyicinin yerleşik header'larını (`stdarg.h`, `stddef.h`...) içeren kaynak dizini.
///
/// Çalışma anında yüklenen libclang (ör. Homebrew'unki) bu dizini kendisi bulamayabilir; o
/// zaman `<string>`, `<memory>` gibi header'lar derlenemez. Dizin, yüklenen kütüphanenin
/// yanındaki `clang/<sürüm>/` klasörüdür (macOS'ta ve Linux dağıtımlarında aynı düzen).
/// Kullanıcı `-resource-dir` verdiyse ya da dizin bulunamazsa `None`.
fn default_resource_dir(extra_args: &[String]) -> Option<String> {
    if extra_args
        .iter()
        .any(|arg| arg.starts_with("-resource-dir"))
    {
        return None;
    }
    let library = clang_sys::get_library()?;
    resource_dir_near(library.path())
}

/// `.../lib/libclang.dylib` için `.../lib/clang/<en yüksek sürüm>/`; `include/stdarg.h`
/// içermeyen klasörler yok sayılır.
fn resource_dir_near(library: &Path) -> Option<String> {
    let clang_dir = library.parent()?.join("clang");
    let mut candidates: Vec<(u32, PathBuf)> = std::fs::read_dir(clang_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("include").join("stdarg.h").is_file())
        .filter_map(|path| {
            let major = path
                .file_name()?
                .to_str()?
                .split('.')
                .next()?
                .parse()
                .ok()?;
            Some((major, path))
        })
        .collect();
    candidates.sort();
    let (_, best) = candidates.pop()?;
    best.to_str().map(str::to_owned)
}

/// macOS'ta, kullanıcı kendisi vermediyse SDK yolunu `xcrun` ile bulur. Homebrew libclang
/// sistem header'larını (`<cstdint>` gibi) ancak bu yolla bulabilir.
#[cfg(target_os = "macos")]
fn default_sysroot(extra_args: &[String]) -> Option<String> {
    let user_set = extra_args
        .iter()
        .any(|arg| arg.starts_with("-isysroot") || arg.starts_with("--sysroot"));
    if user_set {
        return None;
    }
    let output = std::process::Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !path.is_empty()).then_some(path)
}

#[cfg(not(target_os = "macos"))]
fn default_sysroot(_extra_args: &[String]) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_libclang_version() {
        let version = libclang_version().unwrap();
        assert!(
            version.contains("clang version"),
            "beklenmeyen sürüm metni: {version}"
        );
    }

    #[test]
    fn default_args_select_cpp_and_standard() {
        let args = clang_args(&ParseOptions::default()).unwrap();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(&args[..3], ["-x", "c++", "-std=c++17"]);
    }

    #[test]
    fn user_sysroot_is_not_overridden() {
        let options = ParseOptions {
            extra_args: vec!["-isysroot".to_owned(), "/custom/sdk".to_owned()],
            ..ParseOptions::default()
        };
        let args = clang_args(&options).unwrap();
        let count = args
            .iter()
            .filter(|a| a.to_str() == Ok("-isysroot"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn prelude_becomes_include_arguments_after_user_arguments() {
        let options = ParseOptions {
            extra_args: vec!["-DX=1".to_owned()],
            prelude: vec!["a.h".to_owned(), "b.h".to_owned()],
            ..ParseOptions::default()
        };
        let args = clang_args(&options).unwrap();
        let args: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            &args[args.len() - 5..],
            ["-DX=1", "-include", "a.h", "-include", "b.h"]
        );
    }

    #[test]
    fn synthetic_unit_includes_the_header_by_absolute_path() {
        let unit = synthetic_unit("/src/geo.h").unwrap();
        assert_eq!(unit.name.to_str().unwrap(), "/src/geo.h.ferrobridge.cc");
        assert_eq!(unit.contents.to_str().unwrap(), "#include \"/src/geo.h\"\n");
    }

    #[test]
    fn header_path_that_cannot_be_included_is_rejected() {
        for path in ["/src/a\"b.h", "/src/a\nb.h"] {
            assert!(matches!(
                synthetic_unit(path),
                Err(ParserError::InvalidArgument(_))
            ));
        }
    }

    fn raw(
        file: &str,
        line: u32,
        message: &str,
        notes: Vec<clang::RawDiagnostic>,
    ) -> clang::RawDiagnostic {
        clang::RawDiagnostic {
            severity: clang_sys::CXDiagnostic_Error,
            location: clang::Location {
                file: file.to_owned(),
                line,
                column: 1,
            },
            message: message.to_owned(),
            notes,
        }
    }

    #[test]
    fn diagnostic_in_synthetic_unit_moves_to_the_note_in_the_header() {
        let note = raw("/src/geo.h", 4, "to match this '('", Vec::new());
        let d = user_facing(
            raw("/src/geo.h.ferrobridge.cc", 1, "expected ')'", vec![note]),
            "/src/geo.h.ferrobridge.cc",
        );
        assert_eq!(
            d.to_string(),
            "/src/geo.h:4:1: expected ')' (to match this '(')"
        );
    }

    #[test]
    fn diagnostic_without_a_usable_note_is_kept() {
        let d = user_facing(
            raw("/src/geo.h.ferrobridge.cc", 1, "boom", Vec::new()),
            "/src/geo.h.ferrobridge.cc",
        );
        assert_eq!(d.to_string(), "/src/geo.h.ferrobridge.cc:1:1: boom");
        let d = user_facing(
            raw("/src/geo.h", 7, "bad", Vec::new()),
            "/src/geo.h.ferrobridge.cc",
        );
        assert_eq!(d.line, 7);
    }

    #[test]
    fn nul_in_argument_is_rejected() {
        let options = ParseOptions {
            extra_args: vec!["-DFOO\0".to_owned()],
            ..ParseOptions::default()
        };
        assert!(matches!(
            clang_args(&options),
            Err(ParserError::InvalidArgument(_))
        ));
    }
}
