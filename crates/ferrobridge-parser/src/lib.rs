//! The layer that reads C++ headers with libclang and turns them into IR.
//!
//! libclang's C API is reached through `clang-sys` (see
//! `docs/adr/0001-libclang-via-clang-sys.md`). The library is loaded at run time, not at build
//! time (the `clang-sys` `runtime` feature). So if libclang cannot be found, the program does not
//! crash on startup; a clear error is returned.
//!
//! The entry point is [`parse_header`].

mod clang;
mod lower;

use std::ffi::CString;
use std::fmt;
use std::path::{Path, PathBuf};

use ferrobridge_ir::Module;

/// Header parsing settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    /// The C++ standard; passed to clang as `-std=<value>`.
    pub std: String,
    /// Extra arguments passed to clang as they are (e.g. `-I include`, `-DFOO=1`).
    pub extra_args: Vec<String>,
    /// Files to include, in order, before the header (like clang's `-include` argument:
    /// resolved against the working directory first, then the include paths). For headers that
    /// do not compile on their own. Declarations in these files are not taken into the IR.
    pub prelude: Vec<String>,
    /// Class templates to instantiate: (Rust name, C++ spelling), e.g. `("Vector3f",
    /// "Vector3<float>")`. The spelling is resolved in the global scope after the headers are included.
    /// Instantiations become `Class` in the IR (with `instance_of` set).
    pub instantiate: Vec<(String, String)>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            std: "c++17".to_owned(),
            extra_args: Vec::new(),
            prelude: Vec::new(),
            instantiate: Vec::new(),
        }
    }
}

/// An error reported by libclang.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The file containing the error; empty if libclang cannot give a location.
    pub file: String,
    /// The line number, starting at 1.
    pub line: u32,
    /// The column number, starting at 1.
    pub column: u32,
    /// libclang's error message.
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

/// Errors of the parser layer.
#[derive(Debug, thiserror::Error)]
pub enum ParserError {
    /// The libclang dynamic library could not be found or loaded.
    #[error("could not load libclang: {0}")]
    LoadFailed(String),
    /// libclang was loaded but its version could not be read.
    #[error("could not read the libclang version")]
    VersionUnavailable,
    /// The given header file does not exist.
    #[error("header not found: {}", .0.display())]
    FileNotFound(PathBuf),
    /// A path or argument cannot be passed to libclang (not UTF-8, or contains a NUL character).
    #[error("argument that cannot be passed to libclang: {0}")]
    InvalidArgument(String),
    /// libclang could not open the file at all.
    #[error("libclang could not open the header (error code {0})")]
    ClangFailed(i32),
    /// The header has compile errors.
    #[error("could not parse the header: {} errors", diagnostics.len())]
    ParseFailed {
        /// Diagnostics at the error and fatal error levels. Warnings are not included.
        diagnostics: Vec<Diagnostic>,
    },
}

/// Loads libclang if it is not loaded on this thread.
fn ensure_loaded() -> Result<(), ParserError> {
    if clang_sys::is_loaded() {
        return Ok(());
    }
    clang_sys::load().map_err(ParserError::LoadFailed)
}

/// Returns the version text of the loaded libclang (e.g. `"Homebrew clang version 23.1.6"`).
///
/// # Errors
///
/// Returns [`ParserError::LoadFailed`] if libclang cannot be found, and
/// [`ParserError::VersionUnavailable`] if the version text cannot be read.
pub fn libclang_version() -> Result<String, ParserError> {
    ensure_loaded()?;
    let version = clang::version();
    if version.is_empty() {
        Err(ParserError::VersionUnavailable)
    } else {
        Ok(version)
    }
}

/// Reads a C++ header and turns its declarations into IR.
///
/// Only the declarations of the `header` file itself are taken; those in `#include`d files and
/// the [`ParseOptions::prelude`] are not. Unsupported constructs are not dropped; they appear in
/// the IR with their reasons. For several headers, see [`parse_headers`].
///
/// # Errors
///
/// Returns [`ParserError::FileNotFound`] if the file does not exist, [`ParserError::LoadFailed`]
/// if libclang cannot be loaded, and [`ParserError::ParseFailed`] if the header has compile
/// errors.
pub fn parse_header(header: &Path, options: &ParseOptions) -> Result<Module, ParserError> {
    parse_headers(&[header], options)
}

/// Reads several headers for a single bridge: they are all included in the same translation
/// unit, in the given order, and the declarations of all of them are taken. So a type defined in
/// one header can be used in the functions of another.
///
/// The headers are not given to libclang directly as the "main file"; a translation unit
/// generated in memory that `#include`s them is read instead. So a header's `#pragma once` guard
/// works even when the header includes itself again indirectly (`#pragma once` does not work in the main file).
///
/// # Errors
///
/// The same as [`parse_header`]; in addition, [`ParserError::InvalidArgument`] if the header list is empty.
pub fn parse_headers<P: AsRef<Path>>(
    headers: &[P],
    options: &ParseOptions,
) -> Result<Module, ParserError> {
    if headers.is_empty() {
        return Err(ParserError::InvalidArgument(
            "the header list is empty".to_owned(),
        ));
    }
    let mut paths = Vec::with_capacity(headers.len());
    for header in headers {
        let header = header.as_ref();
        if !header.is_file() {
            return Err(ParserError::FileNotFound(header.to_path_buf()));
        }
        let canonical = std::fs::canonicalize(header)
            .map_err(|_| ParserError::FileNotFound(header.to_path_buf()))?;
        let text = canonical
            .to_str()
            .ok_or_else(|| ParserError::InvalidArgument(canonical.display().to_string()))?
            .to_owned();
        paths.push(text);
    }
    let mut unit = synthetic_unit(&paths)?;
    let aliases = instantiation_lines(&mut unit, &options.instantiate)?;
    let header_names = paths
        .iter()
        .map(|p| to_c_string(p))
        .collect::<Result<Vec<_>, _>>()?;
    // The resource directory is found from the loaded library's path, so load it first.
    ensure_loaded()?;
    let args = clang_args(options)?;

    let index = clang::Index::new().ok_or(ParserError::ClangFailed(clang_sys::CXError_Failure))?;
    let tu = clang::TranslationUnit::parse(
        &index,
        &unit.name,
        &args,
        &[(unit.name.clone(), to_c_string(&unit.contents)?)],
    )
    .map_err(ParserError::ClangFailed)?;

    let unit_name = unit.name.to_string_lossy();
    let errors: Vec<Diagnostic> = tu
        .diagnostics()
        .into_iter()
        .filter(|d| d.severity >= clang_sys::CXDiagnostic_Error)
        .map(|d| user_facing(d, &unit_name, &unit.labels))
        .collect();
    if !errors.is_empty() {
        return Err(ParserError::ParseFailed {
            diagnostics: errors,
        });
    }

    // If the headers were parsed, they are certainly in the translation unit.
    let targets = header_names
        .iter()
        .map(|name| {
            tu.file(name)
                .ok_or(ParserError::ClangFailed(clang_sys::CXError_Failure))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(lower::lower(&tu, targets, &aliases))
}

/// Positions a diagnostic relative to the files the user sees.
///
/// A diagnostic located in the synthetic translation unit (e.g. an unclosed parenthesis in a
/// header is noticed at the end of the file, and the end of the file is now in the synthetic unit) tells the user nothing.
/// If it has a first note pointing to another file ("to match this '('"), the diagnostic is moved there and the
/// note's text is appended to the message. Otherwise it is left as is; no diagnostic is dropped.
fn user_facing(
    diagnostic: clang::RawDiagnostic,
    unit_name: &str,
    labels: &[(u32, String)],
) -> Diagnostic {
    let clang::RawDiagnostic {
        location,
        message,
        notes,
        ..
    } = diagnostic;
    if location.file == unit_name {
        // An error on a line generated from a `[templates]` entry: show the entry by name.
        if let Some((_, label)) = labels.iter().find(|(line, _)| *line == location.line) {
            return Diagnostic {
                file: String::new(),
                line: 0,
                column: 0,
                message: format!("{label}: {message}"),
            };
        }
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

/// A translation unit generated in memory that includes the header.
struct SyntheticUnit {
    /// The name of the file that is not on disk: next to the header, so the path in diagnostics makes sense.
    name: CString,
    contents: String,
    /// (line, entry) pairs: an error on that line is shown to the user with the entry's name.
    labels: Vec<(u32, String)>,
}

/// For every `[templates]` entry, adds a `using` alias and a `static_assert` that forces the
/// class to be instantiated (the members' declarations are instantiated, not their definitions).
///
/// The spelling enters the unit as text; so that nothing but a type can be written, spellings
/// containing `;`, `{`, `}`, `"`, `#` or a line break are rejected.
fn instantiation_lines(
    unit: &mut SyntheticUnit,
    instantiate: &[(String, String)],
) -> Result<Vec<lower::InstanceAlias>, ParserError> {
    let mut aliases = Vec::with_capacity(instantiate.len());
    for (index, (rust_name, spelling)) in instantiate.iter().enumerate() {
        if spelling.trim().is_empty() || spelling.contains([';', '{', '}', '"', '#', '\n', '\r']) {
            return Err(ParserError::InvalidArgument(format!(
                "`[templates]` {rust_name} = {spelling:?}: must be a C++ type spelling"
            )));
        }
        let alias = format!("__ferrobridge_inst_{index}");
        let line = u32::try_from(unit.contents.lines().count() + 1).unwrap_or(u32::MAX);
        unit.contents.push_str(&format!(
            "using {alias} = {spelling};\nstatic_assert(sizeof({alias}) > 0, \"\");\n"
        ));
        let label = format!("`[templates]` {rust_name} = \"{spelling}\"");
        unit.labels.push((line, label.clone()));
        unit.labels.push((line + 1, label));
        aliases.push(lower::InstanceAlias {
            rust_name: rust_name.clone(),
            alias,
        });
    }
    Ok(aliases)
}

/// A translation unit with one `#include "<header>"` line for each of `headers` (absolute paths).
/// Its name is next to the first header.
///
/// There are no escape sequences inside `#include "..."`; paths containing `"` or a line break
/// cannot be written on this line and are rejected.
fn synthetic_unit<S: AsRef<str>>(headers: &[S]) -> Result<SyntheticUnit, ParserError> {
    let mut contents = String::new();
    for header in headers {
        let header = header.as_ref();
        if header.contains(['"', '\n', '\r']) {
            return Err(ParserError::InvalidArgument(header.to_owned()));
        }
        contents.push_str(&format!("#include \"{header}\"\n"));
    }
    let first = headers
        .first()
        .ok_or_else(|| ParserError::InvalidArgument("the header list is empty".to_owned()))?;
    Ok(SyntheticUnit {
        name: to_c_string(&format!("{}.ferrobridge.cc", first.as_ref()))?,
        contents,
        labels: Vec::new(),
    })
}

fn to_c_string(text: &str) -> Result<CString, ParserError> {
    CString::new(text).map_err(|_| ParserError::InvalidArgument(text.to_owned()))
}

/// Compiler arguments to pass to libclang.
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

/// The resource directory containing the compiler's built-in headers (`stdarg.h`, `stddef.h`...).
///
/// A libclang loaded at run time (e.g. Homebrew's) may not find this directory itself; then
/// headers such as `<string>` and `<memory>` cannot be compiled. The directory is the
/// `clang/<version>/` folder next to the loaded library (the same layout on macOS and Linux distributions).
/// `None` if the user gave `-resource-dir` or the directory cannot be found.
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

/// `.../lib/clang/<highest version>/` for `.../lib/libclang.dylib`; folders not containing
/// `include/stdarg.h` are ignored.
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

/// On macOS, finds the SDK path with `xcrun` unless the user gave one. Homebrew libclang
/// can find the system headers (such as `<cstdint>`) only this way.
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
            "unexpected version text: {version}"
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
        let unit = synthetic_unit(&["/src/geo.h"]).unwrap();
        assert_eq!(unit.name.to_str().unwrap(), "/src/geo.h.ferrobridge.cc");
        assert_eq!(unit.contents, "#include \"/src/geo.h\"\n");
    }

    #[test]
    fn synthetic_unit_includes_every_header_in_order() {
        let unit = synthetic_unit(&["/src/a.h", "/src/b.h"]).unwrap();
        assert_eq!(unit.name.to_str().unwrap(), "/src/a.h.ferrobridge.cc");
        assert_eq!(
            unit.contents,
            "#include \"/src/a.h\"\n#include \"/src/b.h\"\n"
        );
        assert!(synthetic_unit::<&str>(&[]).is_err());
    }

    #[test]
    fn header_path_that_cannot_be_included_is_rejected() {
        for path in ["/src/a\"b.h", "/src/a\nb.h"] {
            assert!(matches!(
                synthetic_unit(&[path]),
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
            &[],
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
            &[],
        );
        assert_eq!(d.to_string(), "/src/geo.h.ferrobridge.cc:1:1: boom");
        let d = user_facing(
            raw("/src/geo.h", 7, "bad", Vec::new()),
            "/src/geo.h.ferrobridge.cc",
            &[],
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
