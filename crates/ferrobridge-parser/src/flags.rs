//! Compiler flags that come from a build system: a flags file or a `compile_commands.json` entry
//! (ADR-0013).
//!
//! [`filter`] keeps what libclang needs to read the headers the way the compiler does (defines,
//! include paths, forced includes, the target, flags that change the language) and drops the rest.
//! Nothing is dropped silently except what is known to be meaningless for reading a header (the
//! compiler, `-c`, `-o <file>`, the source file, `-x <lang>`, dependency-file flags): warnings,
//! optimization and code generation flags are summarized in a note per kind, and unknown flags are
//! listed in a warning ([`Dropped::reports`]).

use std::ffi::CString;
use std::path::{Path, PathBuf};

use crate::ParserError;

/// The flags that are kept from a flag source, and what was dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompilerFlags {
    /// The C++ standard from `-std=<value>` (the last one wins); not repeated in [`Self::flags`].
    pub std: Option<String>,
    /// The kept flags, in their original order, with relative paths made absolute.
    pub flags: Vec<Flag>,
    /// What was dropped, for the reports.
    pub dropped: Dropped,
}

impl CompilerFlags {
    /// The kept flags as libclang arguments (without the standard).
    pub fn clang_args(&self) -> Vec<String> {
        self.flags
            .iter()
            .flat_map(|flag| flag.args.iter().cloned())
            .collect()
    }

    /// Adds the flags of another source after these. A standard in `other` wins.
    pub fn append(&mut self, other: CompilerFlags) {
        if other.std.is_some() {
            self.std = other.std;
        }
        self.flags.extend(other.flags);
        self.dropped.append(other.dropped);
    }
}

/// A kept flag, as one or two arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    /// What the flag does.
    pub kind: FlagKind,
    /// The arguments, normalized: `-DNAME=value`, `-U NAME` as `-UNAME`, `-I<absolute path>`;
    /// other path flags as two arguments (`-isystem`, `<absolute path>`).
    pub args: Vec<String>,
}

/// The kinds of kept flags. `ferrobridge-build` passes them to the C++ compiler differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagKind {
    /// `-DNAME` or `-DNAME=value`.
    Define,
    /// `-UNAME`.
    Undefine,
    /// `-I<dir>`.
    IncludeDir,
    /// Other preprocessor flags: `-isystem`, `-iquote`, `-idirafter`, `-iframework`, `-F`,
    /// `-include`, `-imacros`.
    Preprocessor,
    /// The target: `-arch`, `--target=`, `-m32`/`-m64`, `-march=`/`-mcpu=`/`-mfpu=`/`-mfloat-abi=`,
    /// deployment targets, `--sysroot`, `-isysroot`, `--gcc-toolchain=`.
    Target,
    /// Flags that change the language or the library: exceptions, RTTI, `char` signedness,
    /// `-stdlib=`, `-pthread` and similar.
    Language,
}

/// The flags that were dropped, by kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dropped {
    /// Warning and diagnostic flags (`-W…`, `-w`, `-pedantic`).
    pub warnings: Vec<String>,
    /// Optimization and debug flags (`-O…`, `-g…`).
    pub optimization: Vec<String>,
    /// Code generation and linker flags (`-f…` not in the keep list, `-Wl,…`, `-L`, `-l`).
    pub code_generation: Vec<String>,
    /// Flags ferrobridge does not know; a flag with its value is one entry (`-Xclang -foo`).
    pub unknown: Vec<String>,
    /// Flags that need a value but are the last argument.
    pub missing_value: Vec<String>,
}

impl Dropped {
    fn append(&mut self, other: Dropped) {
        self.warnings.extend(other.warnings);
        self.optimization.extend(other.optimization);
        self.code_generation.extend(other.code_generation);
        self.unknown.extend(other.unknown);
        self.missing_value.extend(other.missing_value);
    }

    /// One note per summarized kind and one warning for unknown flags and for each flag without a
    /// value. Empty if nothing worth reporting was dropped.
    pub fn reports(&self) -> Vec<Report> {
        let mut reports = Vec::new();
        for (flags, kind) in [
            (&self.warnings, "warning"),
            (&self.optimization, "optimization and debug"),
            (&self.code_generation, "code generation and linker"),
        ] {
            if !flags.is_empty() {
                reports.push(Report::Note(format!(
                    "flags: dropped {} {kind} {} ({}): they do not change how headers are read",
                    flags.len(),
                    plural(flags.len()),
                    examples(flags)
                )));
            }
        }
        if !self.unknown.is_empty() {
            let pronoun = if self.unknown.len() == 1 {
                "it"
            } else {
                "them"
            };
            reports.push(Report::Warning(format!(
                "flags: dropped {} {} ferrobridge does not know: {}; pass {pronoun} after `--` if \
                 libclang needs {pronoun}",
                self.unknown.len(),
                plural(self.unknown.len()),
                self.unknown.join(", "),
            )));
        }
        for flag in &self.missing_value {
            reports.push(Report::Warning(format!(
                "flags: `{flag}` has no value (it is the last argument); dropped"
            )));
        }
        reports
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "flag" } else { "flags" }
}

/// The first three flags, then `…` if there are more.
fn examples(flags: &[String]) -> String {
    let mut text = flags
        .iter()
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if flags.len() > 3 {
        text.push_str(", …");
    }
    text
}

/// A message about dropped flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// Known not to matter for reading headers (`ℹ`).
    Note(String),
    /// May matter; the user should look (`⚠`).
    Warning(String),
}

impl Report {
    /// The text, without a symbol.
    pub fn message(&self) -> &str {
        match self {
            Report::Note(text) | Report::Warning(text) => text,
        }
    }
}

/// Flags whose value is the next argument when written alone, and joined otherwise (`-I dir`,
/// `-Idir`), in the order they are tried: a longer name comes before a name that is its prefix.
const WITH_VALUE: [(&str, Action); 22] = [
    ("-include-pch", Action::Unknown),
    (
        "-include",
        Action::Keep(FlagKind::Preprocessor, PathRule::IfFound),
    ),
    (
        "-imacros",
        Action::Keep(FlagKind::Preprocessor, PathRule::IfFound),
    ),
    (
        "-isysroot",
        Action::Keep(FlagKind::Target, PathRule::Always),
    ),
    ("-isystem-after", Action::Unknown),
    (
        "-isystem",
        Action::Keep(FlagKind::Preprocessor, PathRule::Always),
    ),
    (
        "-iquote",
        Action::Keep(FlagKind::Preprocessor, PathRule::Always),
    ),
    (
        "-idirafter",
        Action::Keep(FlagKind::Preprocessor, PathRule::Always),
    ),
    (
        "-iframework",
        Action::Keep(FlagKind::Preprocessor, PathRule::Always),
    ),
    ("-iprefix", Action::Unknown),
    ("-iwithprefixbefore", Action::Unknown),
    ("-iwithprefix", Action::Unknown),
    ("-ivfsoverlay", Action::Unknown),
    ("-imultilib", Action::Unknown),
    ("-I", Action::Keep(FlagKind::IncludeDir, PathRule::Always)),
    ("-D", Action::Keep(FlagKind::Define, PathRule::Never)),
    ("-U", Action::Keep(FlagKind::Undefine, PathRule::Never)),
    ("-F", Action::Keep(FlagKind::Preprocessor, PathRule::Always)),
    ("-MF", Action::Silent),
    ("-MT", Action::Silent),
    ("-MQ", Action::Silent),
    ("-MJ", Action::Silent),
];

/// Flags whose value is always the next argument.
const SEPARATE_VALUE: [(&str, Action); 12] = [
    ("-o", Action::Silent),
    ("-x", Action::Silent),
    ("-arch", Action::Keep(FlagKind::Target, PathRule::Never)),
    ("-target", Action::Keep(FlagKind::Target, PathRule::Never)),
    ("--target", Action::Keep(FlagKind::Target, PathRule::Never)),
    (
        "--sysroot",
        Action::Keep(FlagKind::Target, PathRule::Always),
    ),
    ("-Xclang", Action::Unknown),
    ("-Xpreprocessor", Action::Unknown),
    ("-Xassembler", Action::Unknown),
    ("-Xlinker", Action::Unknown),
    ("--param", Action::Unknown),
    ("-Xarch_device", Action::Unknown),
];

/// Single flags that change the language or the library.
const LANGUAGE: [&str; 42] = [
    "-fexceptions",
    "-fno-exceptions",
    "-fcxx-exceptions",
    "-fno-cxx-exceptions",
    "-frtti",
    "-fno-rtti",
    "-fsigned-char",
    "-fno-signed-char",
    "-funsigned-char",
    "-fno-unsigned-char",
    "-fshort-enums",
    "-fno-short-enums",
    "-fshort-wchar",
    "-fno-short-wchar",
    "-fms-extensions",
    "-fno-ms-extensions",
    "-fms-compatibility",
    "-fno-ms-compatibility",
    "-fchar8_t",
    "-fno-char8_t",
    "-fblocks",
    "-fno-blocks",
    "-ffast-math",
    "-fno-fast-math",
    "-ffinite-math-only",
    "-fno-finite-math-only",
    "-fgnu-keywords",
    "-fno-gnu-keywords",
    "-fwchar-type=",
    "-pthread",
    // These change predefined macros, keywords or the declarations a header sees.
    "-fopenmp",
    "-ffreestanding",
    "-fno-builtin",
    "-fno-operator-names",
    "-fsized-deallocation",
    "-fno-sized-deallocation",
    "-faligned-allocation",
    "-fno-aligned-allocation",
    "-fdeclspec",
    "-fno-declspec",
    "-fdelayed-template-parsing",
    "-fno-delayed-template-parsing",
];

/// Prefixes of single flags that select the target.
const TARGET_PREFIXES: [&str; 8] = [
    "--target=",
    "--gcc-toolchain=",
    "-march=",
    "-mcpu=",
    "-mfpu=",
    "-mfloat-abi=",
    "-mmacosx-version-min=",
    "-mmacos-version-min=",
];

/// Single flags that are meaningless for reading a header.
const SILENT: [&str; 12] = [
    "-c", "-S", "-E", "-pipe", "-M", "-MM", "-MD", "-MMD", "-MP", "-MG", "-v", "--",
];

/// What happens to a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Keep(FlagKind, PathRule),
    Silent,
    Unknown,
}

/// How a flag's value is made absolute against the entry's directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathRule {
    /// The value is not a path.
    Never,
    /// A relative path is joined to the directory.
    Always,
    /// Joined to the directory only if the file is there (`-include visibility.h` is otherwise
    /// resolved through the include paths, as the compiler does).
    IfFound,
}

/// Keeps what libclang needs to read headers like the compiler, and records what was dropped.
///
/// `args` are compiler arguments without the compiler itself. `directory` is the directory the
/// compiler runs in (a `compile_commands.json` entry's `directory`); relative paths are made
/// absolute against it. With `None`, paths are left as they are (relative to where ferrobridge runs).
///
/// `-std=` moves to [`CompilerFlags::std`]. Source files (`*.c`, `*.cc`, `*.cpp`, …) are dropped
/// without a message; other arguments that are not flags are reported as unknown.
pub fn filter<S: AsRef<str>>(args: &[S], directory: Option<&Path>) -> CompilerFlags {
    let mut out = CompilerFlags::default();
    let mut args = args.iter().map(AsRef::as_ref);
    while let Some(arg) = args.next() {
        if let Some((name, action)) = SEPARATE_VALUE.iter().find(|(name, _)| *name == arg) {
            match args.next() {
                Some(value) => apply(&mut out, *action, name, value, directory),
                None => out.dropped.missing_value.push((*name).to_owned()),
            }
            continue;
        }
        if let Some((name, action)) = WITH_VALUE.iter().find(|(name, _)| arg.starts_with(name)) {
            if arg == *name {
                match args.next() {
                    Some(value) => apply(&mut out, *action, name, value, directory),
                    None => out.dropped.missing_value.push((*name).to_owned()),
                }
            } else {
                apply(&mut out, *action, name, &arg[name.len()..], directory);
            }
            continue;
        }
        single(&mut out, arg, directory);
    }
    out
}

/// Records a flag with its value.
fn apply(out: &mut CompilerFlags, action: Action, name: &str, value: &str, dir: Option<&Path>) {
    match action {
        Action::Silent => {}
        Action::Unknown => out.dropped.unknown.push(format!("{name} {value}")),
        Action::Keep(kind, rule) => {
            let value = match rule {
                PathRule::Never => value.to_owned(),
                PathRule::Always => absolute(value, dir),
                PathRule::IfFound => absolute_if_found(value, dir),
            };
            let args = match (kind, name) {
                (FlagKind::Define | FlagKind::Undefine | FlagKind::IncludeDir, _) => {
                    vec![format!("{name}{value}")]
                }
                (_, "-target" | "--target") => vec![format!("--target={value}")],
                (_, "--sysroot") => vec![format!("--sysroot={value}")],
                _ => vec![name.to_owned(), value],
            };
            out.flags.push(Flag { kind, args });
        }
    }
}

/// What happens to a flag without a separate value.
enum Single {
    Std(String),
    Silent,
    Keep(FlagKind, String),
    Warning,
    Optimization,
    CodeGeneration,
    Unknown,
}

/// Records a flag without a separate value.
fn single(out: &mut CompilerFlags, arg: &str, directory: Option<&Path>) {
    match classify(arg, directory) {
        Single::Std(std) => out.std = Some(std),
        Single::Silent => {}
        Single::Keep(kind, arg) => out.flags.push(Flag {
            kind,
            args: vec![arg],
        }),
        Single::Warning => out.dropped.warnings.push(arg.to_owned()),
        Single::Optimization => out.dropped.optimization.push(arg.to_owned()),
        Single::CodeGeneration => out.dropped.code_generation.push(arg.to_owned()),
        Single::Unknown => out.dropped.unknown.push(arg.to_owned()),
    }
}

fn classify(arg: &str, directory: Option<&Path>) -> Single {
    if let Some(std) = arg
        .strip_prefix("-std=")
        .or_else(|| arg.strip_prefix("--std="))
    {
        return Single::Std(std.to_owned());
    }
    if let Some(sysroot) = arg.strip_prefix("--sysroot=") {
        return Single::Keep(
            FlagKind::Target,
            format!("--sysroot={}", absolute(sysroot, directory)),
        );
    }
    let language = LANGUAGE
        .iter()
        .any(|flag| arg == *flag || (flag.ends_with('=') && arg.starts_with(flag)));
    if SILENT.contains(&arg)
        || arg.starts_with("--driver-mode=")
        || (arg.starts_with("-x") && arg.len() > 2)
        || (!arg.starts_with('-') && is_source_file(arg))
    {
        Single::Silent
    } else if language || arg.starts_with("-stdlib=") || arg.starts_with("-fpack-struct") {
        Single::Keep(FlagKind::Language, arg.to_owned())
    } else if TARGET_PREFIXES.iter().any(|prefix| arg.starts_with(prefix))
        || matches!(arg, "-m32" | "-m64" | "-mthumb" | "-marm")
    {
        Single::Keep(FlagKind::Target, arg.to_owned())
    } else if arg.starts_with("-Wl,") || arg.starts_with("-Wa,") {
        Single::CodeGeneration
    } else if arg.starts_with("-Wp,") || arg.starts_with("-fmodules") {
        // `-Wp,` passes arguments to the preprocessor; modules change what a header sees.
        Single::Unknown
    } else if arg.starts_with("-W")
        || matches!(
            arg,
            "-w" | "-pedantic" | "-pedantic-errors" | "--pedantic" | "-Qunused-arguments"
        )
    {
        Single::Warning
    } else if arg.starts_with("-O") || arg.starts_with("-g") {
        Single::Optimization
    } else if arg.starts_with("-f") || arg.starts_with("-L") || arg.starts_with("-l") {
        Single::CodeGeneration
    } else {
        Single::Unknown
    }
}

fn is_source_file(arg: &str) -> bool {
    Path::new(arg).extension().is_some_and(|ext| {
        ["c", "cc", "cpp", "cxx", "c++", "C", "cp", "m", "mm"]
            .iter()
            .any(|known| ext == *known)
    })
}

fn absolute(path: &str, directory: Option<&Path>) -> String {
    match directory {
        Some(dir) if Path::new(path).is_relative() => dir.join(path).to_string_lossy().into_owned(),
        _ => path.to_owned(),
    }
}

fn absolute_if_found(path: &str, directory: Option<&Path>) -> String {
    match directory {
        Some(dir) if Path::new(path).is_relative() && dir.join(path).is_file() => {
            dir.join(path).to_string_lossy().into_owned()
        }
        _ => path.to_owned(),
    }
}

/// Reads a flags file: one argument per line, exactly as it is passed to the compiler. There is no
/// quoting; blank lines are ignored. CMake writes such a file with
/// `file(GENERATE … CONTENT "$<JOIN:…,\n>")`.
///
/// # Errors
///
/// [`ParserError::FlagsFile`] if the file cannot be read.
pub fn read_flags_file(path: &Path) -> Result<Vec<String>, ParserError> {
    let text = std::fs::read_to_string(path).map_err(|error| ParserError::FlagsFile {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect())
}

/// A `compile_commands.json` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileCommand {
    /// The `compile_commands.json` file the entry was read from, canonical (to watch it for
    /// changes).
    pub database: PathBuf,
    /// The directory the compiler runs in.
    pub directory: PathBuf,
    /// The source file, absolute.
    pub file: PathBuf,
    /// The compiler arguments without the compiler (split by libclang the way clang splits a
    /// `"command"` string). libclang may add `--driver-mode=…` first; [`filter`] drops it.
    pub arguments: Vec<String>,
}

impl CompileCommand {
    /// The entry's arguments through [`filter`], with paths made absolute against its directory.
    pub fn flags(&self) -> CompilerFlags {
        filter(&self.arguments, Some(&self.directory))
    }
}

/// Compiler launchers that may come before the compiler in a command.
const LAUNCHERS: [&str; 4] = ["ccache", "sccache", "distcc", "icecc"];

/// Reads the entry for `source` from a compilation database, through libclang.
///
/// `database` is a `compile_commands.json` file or the directory containing it. The entry is chosen
/// by comparing canonical paths. A relative `source` that does not exist from the working
/// directory is matched by its trailing path components (`src/lib/geo/geo.cpp`) if exactly one
/// entry ends with them.
///
/// # Errors
///
/// [`ParserError::CompilationDatabase`] if the database cannot be read, and
/// [`ParserError::NoCompileCommand`] if there is no entry (or more than one by suffix) for `source`.
pub fn from_compilation_database(
    database: &Path,
    source: &Path,
) -> Result<CompileCommand, ParserError> {
    let fail = |message: &str| ParserError::CompilationDatabase {
        path: database.to_path_buf(),
        message: message.to_owned(),
    };
    let dir = if database.is_dir() {
        database.to_path_buf()
    } else if database
        .file_name()
        .is_some_and(|n| n == "compile_commands.json")
    {
        match database.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        }
    } else {
        return Err(fail(
            "give a `compile_commands.json` file or the directory that contains it",
        ));
    };
    if !dir.join("compile_commands.json").is_file() {
        return Err(fail("`compile_commands.json` not found"));
    }
    let dir_text = dir
        .to_str()
        .ok_or_else(|| ParserError::InvalidArgument(dir.display().to_string()))?;
    let dir_c =
        CString::new(dir_text).map_err(|_| ParserError::InvalidArgument(dir_text.to_owned()))?;
    crate::ensure_loaded()?;
    let raw = crate::clang::compile_commands(&dir_c)
        .ok_or_else(|| fail("libclang could not read it (is it valid JSON?)"))?;

    let database_file = dir.join("compile_commands.json");
    let database_file = std::fs::canonicalize(&database_file).unwrap_or(database_file);
    let entries: Vec<CompileCommand> = raw
        .into_iter()
        .map(|entry| {
            let directory = PathBuf::from(entry.directory);
            let file = directory.join(entry.filename);
            let mut arguments = entry.arguments.into_iter();
            // The compiler; and if it was a launcher (`ccache c++ …`), the compiler after it.
            if arguments.next().is_some_and(|first| is_launcher(&first)) {
                arguments.next();
            }
            CompileCommand {
                database: database_file.clone(),
                directory,
                file,
                arguments: arguments.collect(),
            }
        })
        .collect();

    let wanted = std::fs::canonicalize(source).ok();
    if let Some(wanted) = &wanted {
        if let Some(entry) = entries
            .iter()
            .find(|entry| std::fs::canonicalize(&entry.file).ok().as_ref() == Some(wanted))
        {
            return Ok(entry.clone());
        }
    }
    let by_suffix: Vec<&CompileCommand> = if wanted.is_none() && source.is_relative() {
        entries
            .iter()
            .filter(|entry| entry.file.ends_with(source))
            .collect()
    } else {
        Vec::new()
    };
    if let [entry] = by_suffix.as_slice() {
        return Ok((*entry).clone());
    }
    let closest = if by_suffix.len() > 1 {
        by_suffix
            .iter()
            .map(|entry| entry.file.display().to_string())
            .collect()
    } else {
        closest_files(&entries, source)
    };
    Err(ParserError::NoCompileCommand {
        file: source.to_path_buf(),
        database: database.to_path_buf(),
        closest,
    })
}

fn is_launcher(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .is_some_and(|stem| LAUNCHERS.iter().any(|launcher| stem == *launcher))
}

/// The three entries whose file names share the most trailing path components with `source`.
fn closest_files(entries: &[CompileCommand], source: &Path) -> Vec<String> {
    let wanted: Vec<_> = source.components().rev().collect();
    let mut scored: Vec<(usize, String)> = entries
        .iter()
        .map(|entry| {
            let shared = entry
                .file
                .components()
                .rev()
                .zip(&wanted)
                .take_while(|(a, b)| a == *b)
                .count();
            (shared, entry.file.display().to_string())
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().take(3).map(|(_, file)| file).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PX4 v1.18 `src/lib/geo/geo.cpp` from `build/px4_sitl_default/compile_commands.json`, split
    /// like libclang splits the `"command"` string, without the compiler; `/px4` is the checkout.
    #[rustfmt::skip]
    const PX4_GEO: [&str; 76] = [
        "-DCONFIG_ARCH_BOARD_PX4_SITL", "-DENABLE_LOCKSTEP_SCHEDULER", "-D__CUSTOM_FILE_IO__",
        "-D__PX4_DARWIN", "-D__PX4_POSIX", "-D__STDC_FORMAT_MACROS",
        "-Dnoreturn_function=__attribute__((noreturn))", "-I/px4/boards/px4/sitl/src",
        "-I/px4/platforms/posix/src/px4/common/include", "-I/px4/build/px4_sitl_default",
        "-I/px4/build/px4_sitl_default/src/lib",
        "-I/px4/platforms/posix/src/px4/generic/generic/include", "-I/px4/platforms/common",
        "-I/px4/platforms/common/include", "-I/px4/src", "-I/px4/src/include", "-I/px4/src/lib",
        "-I/px4/src/lib/matrix", "-I/px4/src/modules", "-I/px4/platforms/posix/include",
        "-I/px4/build/px4_sitl_default/external/Install/include", "-O2", "-g", "-DNDEBUG",
        "-std=gnu++17", "-arch", "arm64", "-fPIC", "-g", "-fdata-sections", "-ffunction-sections",
        "-fomit-frame-pointer", "-fmerge-all-constants", "-fno-signed-zeros", "-fno-trapping-math",
        "-freciprocal-math", "-fno-math-errno", "-fno-strict-aliasing", "-fvisibility=hidden",
        "-include", "visibility.h", "-Wall", "-Wextra", "-Werror", "-Warray-bounds", "-Wcast-align",
        "-Wdisabled-optimization", "-Wdouble-promotion", "-Wfatal-errors", "-Wfloat-equal",
        "-Wformat-security", "-Winit-self", "-Wlogical-op", "-Wpointer-arith", "-Wshadow",
        "-Wuninitialized", "-Wunknown-pragmas", "-Wunused-variable",
        "-Wno-missing-field-initializers", "-Wno-missing-include-dirs", "-Wno-unused-parameter",
        "-fcolor-diagnostics", "-fdiagnostics-absolute-paths", "-Qunused-arguments",
        "-Wno-c99-designator", "-Wno-unknown-warning-option", "-Wno-unused-const-variable",
        "-Wno-varargs", "-Wno-vla-cxx-extension", "-Wreorder", "-Wno-overloaded-virtual", "-O2",
        "-o", "src/lib/geo/CMakeFiles/geo.dir/geo.cpp.o", "-c", "/px4/src/lib/geo/geo.cpp",
    ];

    fn of_kind(flags: &CompilerFlags, kind: FlagKind) -> Vec<String> {
        flags
            .flags
            .iter()
            .filter(|flag| flag.kind == kind)
            .flat_map(|flag| flag.args.clone())
            .collect()
    }

    #[test]
    fn px4_geo_keeps_defines_includes_target_and_std() {
        let flags = filter(&PX4_GEO, Some(Path::new("/px4/build/px4_sitl_default")));
        assert_eq!(flags.std.as_deref(), Some("gnu++17"));
        let defines = of_kind(&flags, FlagKind::Define);
        assert_eq!(defines.len(), 8, "{defines:?}");
        assert!(defines.contains(&"-Dnoreturn_function=__attribute__((noreturn))".to_owned()));
        assert!(defines.contains(&"-DNDEBUG".to_owned()));
        assert_eq!(of_kind(&flags, FlagKind::IncludeDir).len(), 14);
        // `visibility.h` is not in the build directory: it stays relative and is found through
        // the include paths, as the compiler finds it.
        assert_eq!(
            of_kind(&flags, FlagKind::Preprocessor),
            ["-include", "visibility.h"]
        );
        assert_eq!(of_kind(&flags, FlagKind::Target), ["-arch", "arm64"]);
        assert!(of_kind(&flags, FlagKind::Language).is_empty());
        assert!(!flags.clang_args().iter().any(|a| a.starts_with("-W")));
        assert!(!flags.clang_args().iter().any(|a| a.starts_with("-std")));

        let dropped = &flags.dropped;
        assert!(dropped.unknown.is_empty(), "{:?}", dropped.unknown);
        assert!(dropped.missing_value.is_empty());
        assert!(dropped.warnings.contains(&"-Werror".to_owned()));
        assert!(dropped.warnings.contains(&"-Wfatal-errors".to_owned()));
        assert!(dropped.warnings.contains(&"-Qunused-arguments".to_owned()));
        assert_eq!(dropped.optimization, ["-O2", "-g", "-g", "-O2"]);
        assert!(dropped.code_generation.contains(&"-fPIC".to_owned()));

        let reports = dropped.reports();
        assert_eq!(reports.len(), 3, "{reports:?}");
        assert!(reports.iter().all(|r| matches!(r, Report::Note(_))));
        assert_eq!(
            reports[0].message(),
            "flags: dropped 28 warning flags (-Wall, -Wextra, -Werror, …): they do not change how \
             headers are read"
        );
    }

    #[test]
    fn joined_and_separate_forms_are_the_same() {
        let joined = filter(
            &[
                "-Iinc",
                "-DX=1",
                "-isysroot/sdk",
                "--target=arm64-apple-macos",
            ],
            None,
        );
        let separate = filter(
            &[
                "-I",
                "inc",
                "-D",
                "X=1",
                "-isysroot",
                "/sdk",
                "-target",
                "arm64-apple-macos",
            ],
            None,
        );
        assert_eq!(joined.flags, separate.flags);
        assert_eq!(
            joined.clang_args(),
            [
                "-Iinc",
                "-DX=1",
                "-isysroot",
                "/sdk",
                "--target=arm64-apple-macos"
            ]
        );
    }

    #[test]
    fn relative_paths_are_made_absolute_against_the_directory() {
        let dir = std::env::temp_dir().join(format!("ferrobridge-flags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.h"), "").unwrap();
        let flags = filter(
            &[
                "-Iinc",
                "-I/abs",
                "-isystem",
                "sys",
                "-include",
                "config.h",
                "-include",
                "elsewhere.h",
                "--sysroot=root",
            ],
            Some(&dir),
        );
        let d = dir.display();
        assert_eq!(
            flags.clang_args(),
            [
                format!("-I{d}/inc"),
                "-I/abs".to_owned(),
                "-isystem".to_owned(),
                format!("{d}/sys"),
                "-include".to_owned(),
                format!("{d}/config.h"),
                "-include".to_owned(),
                "elsewhere.h".to_owned(),
                format!("--sysroot={d}/root"),
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_flags_are_listed_with_their_values() {
        let flags = filter(
            &[
                "-Xclang",
                "-fno-validate-pch",
                "--param",
                "max-inline=9",
                "-frobnicate-x",
                "-zz",
                "-fmodules",
            ],
            None,
        );
        assert_eq!(
            flags.dropped.unknown,
            [
                "-Xclang -fno-validate-pch",
                "--param max-inline=9",
                "-zz",
                "-fmodules"
            ]
        );
        assert_eq!(flags.dropped.code_generation, ["-frobnicate-x"]);
        let warning = flags
            .dropped
            .reports()
            .into_iter()
            .find(|r| matches!(r, Report::Warning(_)))
            .unwrap();
        assert_eq!(
            warning.message(),
            "flags: dropped 4 flags ferrobridge does not know: -Xclang -fno-validate-pch, \
             --param max-inline=9, -zz, -fmodules; pass them after `--` if libclang needs them"
        );
    }

    #[test]
    fn a_flag_without_its_value_is_reported() {
        for args in [&["-DX", "-I"][..], &["-o"][..], &["-include"][..]] {
            let flags = filter(args, None);
            assert_eq!(flags.dropped.missing_value.len(), 1, "{args:?}");
            let report = flags.dropped.reports().pop().unwrap();
            assert!(matches!(report, Report::Warning(_)));
            assert!(report.message().contains("has no value"), "{report:?}");
        }
    }

    #[test]
    fn language_and_target_flags_are_kept() {
        let flags = filter(
            &[
                "-fno-exceptions",
                "-fno-rtti",
                "-funsigned-char",
                "-stdlib=libc++",
                "-m32",
                "-mcpu=cortex-m7",
                "-mmacosx-version-min=13.0",
                "-fpack-struct=4",
                "-pthread",
                "-ffreestanding",
                "--target",
                "armv7m-none-eabi",
            ],
            None,
        );
        assert_eq!(
            of_kind(&flags, FlagKind::Language),
            [
                "-fno-exceptions",
                "-fno-rtti",
                "-funsigned-char",
                "-stdlib=libc++",
                "-fpack-struct=4",
                "-pthread",
                "-ffreestanding"
            ]
        );
        assert_eq!(
            of_kind(&flags, FlagKind::Target),
            [
                "-m32",
                "-mcpu=cortex-m7",
                "-mmacosx-version-min=13.0",
                "--target=armv7m-none-eabi"
            ]
        );
        assert!(flags.dropped.reports().is_empty());
    }

    #[test]
    fn output_language_and_dependency_flags_are_dropped_silently() {
        let flags = filter(
            &[
                "-c",
                "-o",
                "x.o",
                "-x",
                "c++",
                "-MD",
                "-MF",
                "x.d",
                "-MTx.o",
                "main.cpp",
                "--driver-mode=g++",
            ],
            None,
        );
        assert!(flags.flags.is_empty());
        assert_eq!(flags.dropped, Dropped::default());
    }

    #[test]
    fn the_last_std_wins_and_append_keeps_order() {
        let mut first = filter(&["-std=c++14", "-DA"], None);
        first.append(filter(&["-std=gnu++17", "-DB"], None));
        first.append(filter(&["-DC"], None));
        assert_eq!(first.std.as_deref(), Some("gnu++17"));
        assert_eq!(first.clang_args(), ["-DA", "-DB", "-DC"]);
    }

    #[test]
    fn flags_files_hold_one_argument_per_line() {
        let path =
            std::env::temp_dir().join(format!("ferrobridge-flags-{}.txt", std::process::id()));
        std::fs::write(
            &path,
            "-DX=a b\r\n\n-Dnoreturn_function=__attribute__((noreturn))\n   \n-I/x y\n",
        )
        .unwrap();
        assert_eq!(
            read_flags_file(&path).unwrap(),
            [
                "-DX=a b",
                "-Dnoreturn_function=__attribute__((noreturn))",
                "-I/x y"
            ]
        );
        let _ = std::fs::remove_file(&path);
        let error = read_flags_file(&path).unwrap_err();
        assert!(error.to_string().contains("ferrobridge-flags-"), "{error}");
    }
}
