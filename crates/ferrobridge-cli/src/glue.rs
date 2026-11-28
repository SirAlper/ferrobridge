//! The cxx C++ glue for a generated `bridge.rs` (`generate --cxx-glue`), written through `cxx-gen`
//! so that a build system that compiles the C++ itself (CMake, ADR-0013) needs no `cxxbridge` tool.

use std::str::FromStr;

use crate::GENERATED_MARKER;

/// The `cxx-gen` version; the dependency is pinned to it (`Cargo.toml`).
pub(crate) const CXX_GEN_VERSION: &str = "0.7.198";

/// The glue files, relative to the output directory.
pub(crate) const GLUE_FILES: [&str; 3] = ["bridge.rs.cc", "bridge.rs.h", "rust/cxx.h"];

/// The three glue files, each starting with the `@generated` line.
pub(crate) struct Glue {
    /// `bridge.rs.cc`: the C++ side of the bridge's functions.
    pub(crate) implementation: String,
    /// `bridge.rs.h`: the C++ declarations of the bridge's shared types and functions.
    pub(crate) header: String,
    /// `rust/cxx.h`: cxx's C++ runtime header.
    pub(crate) cxx_h: String,
}

/// Generates the glue from the text of a generated `bridge.rs`.
pub(crate) fn generate(bridge_rs: &str) -> Result<Glue, String> {
    let tokens = proc_macro2::TokenStream::from_str(bridge_rs)
        .map_err(|error| format!("could not read the generated bridge.rs: {error}"))?;
    let code = cxx_gen::generate_header_and_cc(tokens, &cxx_gen::Opt::default())
        .map_err(|error| format!("cxx-gen could not generate the C++ glue: {error}"))?;
    let marker = format!(
        "{GENERATED_MARKER} {} (cxx-gen {CXX_GEN_VERSION}) — do not edit by hand; regenerate with \
         `ferrobridge generate --cxx-glue`.\n",
        env!("CARGO_PKG_VERSION")
    );
    let text = |bytes: Vec<u8>| {
        String::from_utf8(bytes)
            .map(|text| format!("{marker}{text}"))
            .map_err(|_| "cxx-gen produced text that is not UTF-8".to_owned())
    };
    Ok(Glue {
        implementation: text(code.implementation)?,
        header: text(code.header)?,
        cxx_h: format!("{marker}{}", cxx_gen::HEADER),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIDGE: &str = r#"
#[cxx::bridge(namespace = "demo")]
mod ffi {
    unsafe extern "C++" {
        include!("shim.h");
        fn add(a: i32, b: i32) -> Result<i32>;
    }
}
"#;

    #[test]
    fn every_glue_file_starts_with_the_marker() {
        let glue = generate(BRIDGE).unwrap();
        for text in [&glue.implementation, &glue.header, &glue.cxx_h] {
            assert!(text.starts_with(GENERATED_MARKER), "{text}");
            assert!(text.lines().next().unwrap().contains("cxx-gen 0.7.198"));
        }
        assert!(glue.implementation.contains("#include \"shim.h\""));
        assert!(
            glue.implementation.contains("demo$cxxbridge1$"),
            "{}",
            glue.implementation
        );
    }

    #[test]
    fn a_bridge_cxx_cannot_read_is_an_error() {
        assert!(
            generate("#[cxx::bridge] mod ffi { extern \"C++\" { fn f(x: &mut &mut i32); } }")
                .is_err()
        );
        assert!(generate("fn (").is_err());
    }
}
