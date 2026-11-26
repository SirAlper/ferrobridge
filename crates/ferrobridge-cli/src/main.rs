//! `ferrobridge` komut satırı aracı.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor(),
    }
}

fn doctor() -> ExitCode {
    match ferrobridge_parser::libclang_version() {
        Ok(version) => {
            println!("✓ libclang bulundu: {version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("✗ {error}");
            eprintln!(
                "  İpucu: `brew install llvm` ve ardından \
                 `export LIBCLANG_PATH=\"$(brew --prefix llvm)/lib\"`"
            );
            ExitCode::FAILURE
        }
    }
}
