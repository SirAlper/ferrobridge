//! `ferrobridge` komut satırı aracı.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ferrobridge_parser::{ParseOptions, ParserError};

/// C++ header'larından güvenli Rust binding'leri üretir.
#[derive(Parser)]
#[command(name = "ferrobridge", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Geliştirme ortamını kontrol eder (libclang erişimi).
    Doctor,
    /// Bir C++ header'ını okur ve içinde bulduklarını (IR) JSON olarak basar.
    Parse {
        /// Okunacak C++ header dosyası.
        header: PathBuf,
        /// C++ standardı (clang'a `-std=<değer>` olarak geçer).
        #[arg(long, default_value = "c++17")]
        std: String,
        /// `--` sonrasında clang'a olduğu gibi geçirilecek argümanlar (ör. `-- -I include`).
        #[arg(last = true)]
        clang_args: Vec<String>,
    },
}

const LIBCLANG_HINT: &str =
    "  İpucu: `brew install llvm` ve ardından `export LIBCLANG_PATH=\"$(brew --prefix llvm)/lib\"`";

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor(),
        Command::Parse {
            header,
            std,
            clang_args,
        } => parse(&header, std, clang_args),
    }
}

fn doctor() -> ExitCode {
    match ferrobridge_parser::libclang_version() {
        Ok(version) => {
            println!("✓ libclang bulundu: {version}");
            ExitCode::SUCCESS
        }
        Err(error) => report(&error),
    }
}

fn parse(header: &std::path::Path, std: String, extra_args: Vec<String>) -> ExitCode {
    let options = ParseOptions { std, extra_args };
    let module = match ferrobridge_parser::parse_header(header, &options) {
        Ok(module) => module,
        Err(error) => return report(&error),
    };
    match serde_json::to_string_pretty(&module) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("✗ IR JSON'a çevrilemedi: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Hatayı stderr'e basar: önce özet, sonra varsa her tanı ve ipucu.
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
