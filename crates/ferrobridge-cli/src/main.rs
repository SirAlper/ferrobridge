//! The `ferrobridge` command-line tool.

mod depfile;
mod glue;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ferrobridge_analysis::{Annotations, BridgePlan};
use ferrobridge_codegen::{CodegenOptions, GeneratedFiles};
use ferrobridge_parser::flags::{self, CompilerFlags, Report};
use ferrobridge_parser::{ParseOptions, ParserError};

/// Generates safe Rust bindings from C++ headers.
#[derive(Parser)]
#[command(name = "ferrobridge", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Checks the development environment (access to libclang).
    Doctor,
    /// Reads a C++ header and prints what it found inside (the IR) as JSON.
    Parse {
        /// The C++ header file to read.
        header: PathBuf,
        #[command(flatten)]
        parse: ParseArgs,
    },
    /// Generates a single cxx bridge (`bridge.rs`) and C++ shim (`shim.h`, `shim.cc`) from one or
    /// more C++ headers. A type defined in one header can be used in functions in the others.
    /// Every item that cannot be bound is printed as a warning with its reason.
    Generate {
        /// The C++ header files to read (the bridge's headers).
        #[arg(required = true, num_args = 1..)]
        headers: Vec<PathBuf>,
        /// The directory to write the three files to (created if missing).
        #[arg(long)]
        out_dir: PathBuf,
        /// The path the shim uses to `#include` the header; once per header, in the same
        /// order. Default: the headers' file names.
        #[arg(long)]
        include_as: Vec<String>,
        /// The path the bridge uses to `#include` `shim.h`.
        #[arg(long, default_value = "shim.h")]
        shim_include: String,
        /// The C++ namespace of the shim functions. Default: `ferrobridge::<header name>`.
        #[arg(long)]
        shim_namespace: Option<String>,
        /// The ownership annotation file (`ferrobridge.toml`). If not given, there are no annotations
        /// and pointers with unclear ownership are not bound.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Also overwrite files that ferrobridge did not generate (that look hand-written).
        #[arg(long)]
        force: bool,
        /// Also write a Make-syntax depfile: `<out-dir>/bridge.rs` depends on every file the headers
        /// include (directly or not), the prelude, `--config` and `--flags-file`. For build systems
        /// that regenerate the bridge (CMake's `DEPFILE`).
        #[arg(long, value_name = "PATH")]
        depfile: Option<PathBuf>,
        /// Also write the cxx C++ glue next to the other files: `bridge.rs.cc`, `bridge.rs.h` and
        /// `rust/cxx.h` (generated with the `cxx-gen` version named in their first line). For build
        /// systems that compile the bridge's C++ themselves.
        #[arg(long)]
        cxx_glue: bool,
        #[command(flatten)]
        parse: ParseArgs,
    },
    /// Prints a commented-out `ferrobridge.toml` draft for every pointer/reference that needs an
    /// annotation. Enable the lines you are sure of and use it with `generate --config`.
    Suggest {
        /// The C++ header files to read (the same as those given to `generate`).
        #[arg(required = true, num_args = 1..)]
        headers: Vec<PathBuf>,
        /// Existing annotations; places they resolve do not enter the draft.
        #[arg(long)]
        config: Option<PathBuf>,
        #[command(flatten)]
        parse: ParseArgs,
    },
}

/// Settings for reading the header; shared by `parse`, `generate` and `suggest`.
///
/// Flags reach libclang in this order: `--flags-file` (in the given order), the `--compile-commands`
/// entry, then the arguments after `--`. Flags from files and databases are filtered: what libclang
/// needs (defines, include paths, forced includes, the target, language flags) is kept, warnings,
/// optimization and code generation flags are dropped with a note, and unknown flags are dropped
/// with a warning. Arguments after `--` are never filtered.
#[derive(clap::Args)]
struct ParseArgs {
    /// The C++ standard (passed to clang as `-std=<value>`). Default: the last `-std=` of the
    /// flag sources, otherwise `c++17`.
    #[arg(long)]
    std: Option<String>,
    /// A file to include before the header; for headers that do not compile on their own.
    /// Repeatable, included in the given order; declarations here are not bound.
    /// In `generate`, the shim also includes it before the header: give a name resolvable from
    /// the include paths (e.g. `AP_Math/AP_Math.h`).
    #[arg(long, value_name = "HEADER")]
    prelude: Vec<String>,
    /// A file of compiler flags, one argument per line, exactly as passed to the compiler (no
    /// quoting; blank lines are ignored). Repeatable. Relative paths are left as they are.
    #[arg(long, value_name = "FILE")]
    flags_file: Vec<PathBuf>,
    /// A compilation database (`compile_commands.json` or its directory). The flags of the entry
    /// for `--for` are used; its relative paths are resolved against the entry's directory.
    #[arg(long, value_name = "PATH", requires = "for_source")]
    compile_commands: Option<PathBuf>,
    /// The source file whose `--compile-commands` entry gives the flags (e.g. the library's
    /// `.cpp`). A relative path that does not exist here is matched by its last components.
    #[arg(long = "for", value_name = "SOURCE", requires = "compile_commands")]
    for_source: Option<PathBuf>,
    /// Arguments after `--` passed to clang as they are (e.g. `-- -I include`).
    #[arg(last = true)]
    clang_args: Vec<String>,
}

/// The default C++ standard when neither `--std` nor a flag source gives one.
const DEFAULT_STD: &str = "c++17";

impl ParseArgs {
    /// Reads the flag sources, prints what their filter dropped and builds the parser settings.
    fn into_options(self) -> Result<ParseOptions, ExitCode> {
        let mut flags = CompilerFlags::default();
        for path in &self.flags_file {
            let args = flags::read_flags_file(path).map_err(|e| report(&e))?;
            flags.append(flags::filter(&args, None));
        }
        if let (Some(database), Some(source)) = (&self.compile_commands, &self.for_source) {
            let command =
                flags::from_compilation_database(database, source).map_err(|e| report(&e))?;
            flags.append(command.flags());
        }
        for message in flags.dropped.reports() {
            match message {
                Report::Note(text) => eprintln!("ℹ {text}"),
                Report::Warning(text) => eprintln!("⚠ {text}"),
            }
        }
        let mut extra_args = flags.clang_args();
        extra_args.extend(self.clang_args);
        Ok(ParseOptions {
            // An explicit `--std` wins over a `-std=` from a flag source (ADR-0013).
            std: self
                .std
                .or(flags.std)
                .unwrap_or_else(|| DEFAULT_STD.to_owned()),
            extra_args,
            prelude: self.prelude,
            instantiate: Vec::new(),
        })
    }
}

/// The first line of every generated file; checked before overwriting.
pub(crate) const GENERATED_MARKER: &str = "// @generated by ferrobridge";
const OUTPUT_FILES: [&str; 3] = ["bridge.rs", "shim.h", "shim.cc"];

const LIBCLANG_HINT: &str =
    "  Hint: `brew install llvm`, then `export LIBCLANG_PATH=\"$(brew --prefix llvm)/lib\"`";

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor(),
        Command::Parse {
            header,
            parse: args,
        } => match args.into_options() {
            Ok(options) => parse(&header, &options),
            Err(code) => code,
        },
        Command::Generate {
            headers,
            out_dir,
            include_as,
            shim_include,
            shim_namespace,
            config,
            force,
            depfile,
            cxx_glue,
            parse: args,
        } => {
            if !force {
                let foreign = foreign_files(&out_dir, cxx_glue);
                if !foreign.is_empty() {
                    eprintln!(
                        "✗ these files do not look generated by ferrobridge and were not overwritten: \
                         {}",
                        foreign.join(", ")
                    );
                    eprintln!("  Use `--force` to write them anyway.");
                    return ExitCode::FAILURE;
                }
            }
            let header_includes = if include_as.is_empty() {
                headers.iter().map(|h| file_name(h)).collect()
            } else if include_as.len() == headers.len() {
                include_as
            } else {
                eprintln!(
                    "✗ {} headers but {} `--include-as` given; give one per header, in the same \
                     order.",
                    headers.len(),
                    include_as.len()
                );
                return ExitCode::FAILURE;
            };
            let options = CodegenOptions {
                header_includes,
                shim_include,
                shim_namespace: shim_namespace
                    .unwrap_or_else(|| format!("ferrobridge::{}", identifier_from(&headers[0]))),
                prelude: args.prelude.clone(),
            };
            let outputs = Outputs {
                dir: out_dir,
                depfile,
                cxx_glue,
                flags_files: args.flags_file.clone(),
                compile_commands: args.compile_commands.as_deref().map(database_file),
            };
            match args.into_options() {
                Ok(parse_options) => generate(
                    &headers,
                    &outputs,
                    &options,
                    config.as_deref(),
                    parse_options,
                ),
                Err(code) => code,
            }
        }
        Command::Suggest {
            headers,
            config,
            parse: args,
        } => match args.into_options() {
            Ok(options) => suggest(&headers, config.as_deref(), options),
            Err(code) => code,
        },
    }
}

/// What `generate` writes besides the three files.
struct Outputs {
    /// The output directory.
    dir: PathBuf,
    /// Where to write the depfile, if wanted.
    depfile: Option<PathBuf>,
    /// Whether to write the cxx C++ glue.
    cxx_glue: bool,
    /// The flags files, which the bridge also depends on.
    flags_files: Vec<PathBuf>,
    /// The compilation database, which the bridge also depends on.
    compile_commands: Option<PathBuf>,
}

/// Reads the header, loads the annotations and runs the analysis. On error, prints it and
/// returns the exit code. Unnecessary annotation warnings are printed here. Also returns every
/// file the translation unit read.
fn analyze(
    headers: &[PathBuf],
    config: Option<&Path>,
    options: &ParseOptions,
) -> Result<(BridgePlan, Vec<PathBuf>), ExitCode> {
    // Annotations first: `[templates]` goes to the parser as instantiations.
    let annotations = match config {
        Some(path) => Annotations::from_file(path).map_err(|error| {
            eprintln!("✗ {error}");
            ExitCode::FAILURE
        })?,
        None => Annotations::default(),
    };
    let mut options = options.clone();
    options.instantiate = annotations.instantiations();
    let parsed = ferrobridge_parser::parse_headers_with_inclusions(headers, &options)
        .map_err(|e| report(&e))?;
    let plan = ferrobridge_analysis::check(&parsed.module, &annotations).map_err(|issues| {
        eprintln!("✗ the annotation file does not match the header:");
        for issue in &issues {
            eprintln!("  ✗ {issue}");
        }
        ExitCode::FAILURE
    })?;
    for warning in &plan.annotation_warnings {
        eprintln!("⚠ {warning}");
    }
    Ok((plan, parsed.inclusions))
}

fn suggest(headers: &[PathBuf], config: Option<&Path>, options: ParseOptions) -> ExitCode {
    let plan = match analyze(headers, config, &options) {
        Ok((plan, _)) => plan,
        Err(code) => return code,
    };
    print!("{}", ferrobridge_analysis::render_suggestions(&plan));
    if !plan.suggestions.is_empty() {
        eprintln!(
            "ℹ {} places need an annotation. Enable the lines you are sure of and use them with \
             `generate --config`.",
            plan.suggestions.len()
        );
    }
    ExitCode::SUCCESS
}

fn generate(
    headers: &[PathBuf],
    outputs: &Outputs,
    options: &CodegenOptions,
    config: Option<&Path>,
    parse_options: ParseOptions,
) -> ExitCode {
    let out_dir = outputs.dir.as_path();
    let (plan, inclusions) = match analyze(headers, config, &parse_options) {
        Ok(result) => result,
        Err(code) => return code,
    };
    let files = match ferrobridge_codegen::generate(&plan, options) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("✗ {error}");
            return ExitCode::FAILURE;
        }
    };
    let glue = if outputs.cxx_glue {
        match glue::generate(&files.bridge_rs) {
            Ok(glue) => Some(glue),
            Err(error) => {
                eprintln!("✗ {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    for skipped in &plan.skipped {
        eprintln!(
            "⚠ {} ({}:{}): {}",
            skipped.name, skipped.location.file, skipped.location.line, skipped.reason
        );
    }
    for note in &plan.notes {
        eprintln!(
            "ℹ {} ({}:{}): {}",
            note.name, note.location.file, note.location.line, note.reason
        );
    }

    if let Err(error) = write_files(out_dir, &files, glue.as_ref()) {
        eprintln!(
            "✗ could not write the files ({}): {error}",
            out_dir.display()
        );
        return ExitCode::FAILURE;
    }
    if let Some(path) = &outputs.depfile {
        let mut dependencies = inclusions;
        dependencies.extend(config.map(absolute));
        dependencies.extend(outputs.flags_files.iter().map(|path| absolute(path)));
        dependencies.extend(outputs.compile_commands.as_deref().map(absolute));
        let text = depfile::render(&out_dir.join("bridge.rs"), &dependencies);
        let written = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(path, text));
        if let Err(error) = written {
            eprintln!(
                "✗ could not write the depfile ({}): {error}",
                path.display()
            );
            return ExitCode::FAILURE;
        }
    }
    let bound = plan.functions.len()
        + plan
            .classes
            .iter()
            .map(|c| c.constructors.len() + c.methods.len())
            .sum::<usize>();
    println!(
        "✓ bound {} classes, {} enums, {} constants and {bound} functions/methods; skipped {} items.",
        plan.classes.len(),
        plan.enums.len(),
        plan.constants.len(),
        plan.skipped.len()
    );
    if !plan.assumptions.is_empty() {
        println!(
            "  {} assumptions based on annotations (listed at the top of the generated files).",
            plan.assumptions.len()
        );
    }
    if !plan.suggestions.is_empty() {
        println!(
            "  {} places need an annotation: `ferrobridge suggest {}`",
            plan.suggestions.len(),
            headers
                .iter()
                .map(|h| h.display().to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let glue_files: &[&str] = if glue.is_some() {
        &glue::GLUE_FILES
    } else {
        &[]
    };
    for name in OUTPUT_FILES.iter().chain(glue_files) {
        println!("  {}", out_dir.join(name).display());
    }
    if let Some(path) = &outputs.depfile {
        println!("  {}", path.display());
    }
    ExitCode::SUCCESS
}

/// The `compile_commands.json` file for a `--compile-commands` argument (the file or its directory).
fn database_file(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("compile_commands.json")
    } else {
        path.to_path_buf()
    }
}

/// The canonical path if it exists, otherwise the path as given.
fn absolute(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Output files in `out_dir` that exist but do not start with `@generated by ferrobridge`. With
/// `cxx_glue`, the glue files are checked too.
fn foreign_files(out_dir: &Path, cxx_glue: bool) -> Vec<String> {
    let glue_files: &[&str] = if cxx_glue { &glue::GLUE_FILES } else { &[] };
    OUTPUT_FILES
        .iter()
        .chain(glue_files)
        .map(|name| out_dir.join(name))
        .filter(|path| path.exists())
        .filter(|path| {
            fs::read_to_string(path)
                .map(|content| !content.starts_with(GENERATED_MARKER))
                .unwrap_or(true)
        })
        .map(|path| path.display().to_string())
        .collect()
}

fn write_files(
    out_dir: &Path,
    files: &GeneratedFiles,
    glue: Option<&glue::Glue>,
) -> std::io::Result<()> {
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("bridge.rs"), &files.bridge_rs)?;
    fs::write(out_dir.join("shim.h"), &files.shim_h)?;
    fs::write(out_dir.join("shim.cc"), &files.shim_cc)?;
    if let Some(glue) = glue {
        fs::create_dir_all(out_dir.join("rust"))?;
        fs::write(out_dir.join("bridge.rs.cc"), &glue.implementation)?;
        fs::write(out_dir.join("bridge.rs.h"), &glue.header)?;
        fs::write(out_dir.join("rust/cxx.h"), &glue.cxx_h)?;
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A valid C++ identifier from the header name (`my-lib.h` → `my_lib`).
fn identifier_from(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut ident: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if ident.is_empty() || ident.starts_with(|c: char| c.is_ascii_digit()) {
        ident.insert(0, '_');
    }
    ident
}

fn doctor() -> ExitCode {
    match ferrobridge_parser::libclang_version() {
        Ok(version) => {
            println!("✓ libclang found: {version}");
            ExitCode::SUCCESS
        }
        Err(error) => report(&error),
    }
}

fn parse(header: &Path, options: &ParseOptions) -> ExitCode {
    let module = match ferrobridge_parser::parse_header(header, options) {
        Ok(module) => module,
        Err(error) => return report(&error),
    };
    match serde_json::to_string_pretty(&module) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("✗ could not turn the IR into JSON: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Prints the error to stderr: first the summary, then every diagnostic and hint, if any.
fn report(error: &ParserError) -> ExitCode {
    eprintln!("✗ {error}");
    match error {
        ParserError::ParseFailed { diagnostics } => {
            for diagnostic in diagnostics {
                eprintln!("  ✗ {diagnostic}");
            }
        }
        ParserError::LoadFailed(_) => eprintln!("{LIBCLANG_HINT}"),
        _ => {}
    }
    ExitCode::FAILURE
}
