//! Code generation from the binding plan (ADR-0002, ADR-0004).
//!
//! Three files are generated:
//! - `bridge.rs`: the `#[cxx::bridge]` module (the Rust side cxx compiles).
//! - `shim.h` / `shim.cc`: C++ helpers for things cxx cannot call directly
//!   (constructor factories, static methods, classes, `std::string` and `std::vector`s
//!   returned by value, annotated pointers) and
//!   `rust::behavior::trycatch`, which catches all C++ exceptions (safety rule 4).
//!
//! The generated files are seen, reviewed and committed by the user (ADR-0004).

mod bridge;
mod safe;
mod shim;
mod spelling;

use std::collections::HashSet;

use ferrobridge_analysis::{BridgePlan, Callee, FunctionPlan, TypeRef};

/// Code generation settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenOptions {
    /// The paths the shim uses to `#include` the bridge's headers, in order (e.g.
    /// `["mylib/vehicle.h"]`). There must be at least one.
    pub header_includes: Vec<String>,
    /// The path the bridge and `shim.cc` use to `#include` `shim.h`.
    pub shim_include: String,
    /// The C++ namespace of the shim functions (e.g. `"ferrobridge::vehicle"`). Must be specific to the
    /// header, so the shims of different headers in the same program do not clash.
    pub shim_namespace: String,
    /// Files the shim includes before the header (for headers that do not compile on their
    /// own; the same as `prelude` in the parser). Must be resolvable from the include paths.
    pub prelude: Vec<String>,
}

/// The contents of the generated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFiles {
    /// `bridge.rs`
    pub bridge_rs: String,
    /// `shim.h`
    pub shim_h: String,
    /// `shim.cc`
    pub shim_cc: String,
}

/// Code generation errors.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// The namespace given as a setting is not a valid C++ namespace path.
    #[error("invalid shim namespace: `{0}` (e.g. `ferrobridge::vehicle`)")]
    InvalidNamespace(String),
    /// A path that cannot be written on an `#include "..."` line (contains `"` or a line break, or is empty).
    #[error("value that cannot be used as an `#include` path: {0:?}")]
    InvalidInclude(String),
    /// A name cannot be used as a Rust identifier.
    #[error("name that cannot be used as a Rust identifier: `{0}`")]
    InvalidIdentifier(String),
    /// The generated Rust code could not be parsed; this is a bug in ferrobridge.
    #[error("the generated bridge code is invalid (a ferrobridge bug, please report it): {0}")]
    Internal(String),
}

/// Generates the three files using the plan.
///
/// # Errors
///
/// Returns an error if the settings are invalid or a name cannot be used in Rust.
pub fn generate(
    plan: &BridgePlan,
    options: &CodegenOptions,
) -> Result<GeneratedFiles, CodegenError> {
    if !is_cpp_namespace(&options.shim_namespace) {
        return Err(CodegenError::InvalidNamespace(
            options.shim_namespace.clone(),
        ));
    }
    // These values are written into the generated C++ as `#include "..."`; they must not break the
    // line and inject code.
    if options.header_includes.is_empty() {
        return Err(CodegenError::InvalidInclude(String::new()));
    }
    let includes = options
        .header_includes
        .iter()
        .chain([&options.shim_include])
        .chain(&options.prelude);
    for include in includes {
        if !is_include_path(include) {
            return Err(CodegenError::InvalidInclude(include.clone()));
        }
    }
    let shims = shim_functions(plan);
    let header = header_comment(plan, options);
    Ok(GeneratedFiles {
        bridge_rs: format!(
            "{}\n{}",
            comment(&header, "//"),
            bridge::generate(plan, options, &shims)?
        ),
        shim_h: format!(
            "{}\n{}",
            comment(&header, "//"),
            shim::header(options, &shims, &plan.classes)
        ),
        shim_cc: format!(
            "{}\n{}",
            comment(&header, "//"),
            shim::source(options, &shims, &plan.constants, &plan.classes)
        ),
    })
}

/// A function called through a C++ shim. Its name is the same in Rust and in C++.
pub(crate) struct ShimFn<'p> {
    pub(crate) name: String,
    pub(crate) plan: &'p FunctionPlan,
}

/// All functions that need a shim, in bridge order.
///
/// The names of those in free scope (free functions, constructors, static methods) were already
/// made unique in analysis. Method shims get the name `<Class>_<method>` and are made unique with
/// a sequence number if needed.
fn shim_functions(plan: &BridgePlan) -> Vec<ShimFn<'_>> {
    let mut used: HashSet<String> = plan
        .functions
        .iter()
        .chain(plan.classes.iter().flat_map(|c| &c.constructors))
        .chain(plan.classes.iter().flat_map(|c| &c.methods))
        .filter(|f| !matches!(f.callee, Callee::Method { .. }))
        .map(|f| f.rust_name.clone())
        .collect();
    let mut shims = Vec::new();
    for class in &plan.classes {
        for constructor in &class.constructors {
            shims.push(ShimFn {
                name: constructor.rust_name.clone(),
                plan: constructor,
            });
        }
        for method in class.methods.iter().filter(|m| m.needs_shim()) {
            let name = if let Callee::Method { class, .. } = &method.callee {
                unique(&mut used, method_shim_name(class, method))
            } else {
                method.rust_name.clone()
            };
            shims.push(ShimFn { name, plan: method });
        }
    }
    for function in plan.functions.iter().filter(|f| f.needs_shim()) {
        shims.push(ShimFn {
            name: function.rust_name.clone(),
            plan: function,
        });
    }
    shims
}

fn method_shim_name(class: &TypeRef, method: &FunctionPlan) -> String {
    format!("{}_{}", class.rust_name, method.rust_name)
}

fn unique(used: &mut HashSet<String>, candidate: String) -> String {
    let mut name = candidate.clone();
    let mut counter = 2;
    while !used.insert(name.clone()) {
        name = format!("{candidate}_{counter}");
        counter += 1;
    }
    name
}

fn is_include_path(path: &str) -> bool {
    !path.is_empty() && !path.contains(['"', '\n', '\r', '\0'])
}

fn is_cpp_namespace(path: &str) -> bool {
    !path.is_empty()
        && path.split("::").all(|segment| {
            let mut chars = segment.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// The comment at the top of every generated file: where it was generated from and what was not bound.
fn header_comment(plan: &BridgePlan, options: &CodegenOptions) -> Vec<String> {
    let mut lines = vec![
        "@generated by ferrobridge — do not edit by hand; regenerate when the header changes."
            .to_owned(),
        match options.header_includes.as_slice() {
            [one] => format!("Source header: {one}"),
            many => format!("Source headers: {}", many.join(", ")),
        },
    ];
    if !plan.skipped.is_empty() {
        lines.push(String::new());
        lines.push(format!("Items not bound ({}):", plan.skipped.len()));
        for skipped in &plan.skipped {
            lines.push(format!(
                "  - {} ({}:{}): {}",
                skipped.name, skipped.location.file, skipped.location.line, skipped.reason
            ));
        }
    }
    if !plan.assumptions.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Assumptions based on annotations ({}). These are promises in `ferrobridge.toml`; if one",
            plan.assumptions.len()
        ));
        lines.push("is wrong, the generated code can cause undefined behavior:".to_owned());
        for assumption in &plan.assumptions {
            lines.push(format!(
                "  - {} {} `{}` ({}): {}",
                assumption.name,
                assumption.subject,
                assumption.annotation,
                assumption.origin,
                assumption.promise
            ));
        }
    }
    if !plan.notes.is_empty() {
        lines.push(String::new());
        lines.push("Notlar:".to_owned());
        for note in &plan.notes {
            lines.push(format!(
                "  - {} ({}:{}): {}",
                note.name, note.location.file, note.location.line, note.reason
            ));
        }
    }
    lines
}

fn comment(lines: &[String], prefix: &str) -> String {
    lines
        .iter()
        .map(|line| {
            if line.is_empty() {
                format!("{prefix}\n")
            } else {
                format!("{prefix} {line}\n")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_shim_namespace() {
        assert!(is_cpp_namespace("ferrobridge::vehicle"));
        assert!(is_cpp_namespace("_x::y2"));
        assert!(!is_cpp_namespace(""));
        assert!(!is_cpp_namespace("a::"));
        assert!(!is_cpp_namespace("a b"));
        assert!(!is_cpp_namespace("2a"));
    }

    #[test]
    fn include_paths_cannot_break_out_of_the_include_line() {
        let options = |header: &str, prelude: &str| CodegenOptions {
            header_includes: vec![header.to_owned()],
            shim_include: "x/shim.h".to_owned(),
            shim_namespace: "ferrobridge::x".to_owned(),
            prelude: vec![prelude.to_owned()],
        };
        let plan = BridgePlan::default();
        assert!(generate(&plan, &options("x/input.h", "x/pre.h")).is_ok());
        for (header, prelude) in [
            ("x.h\"\n#include <evil>", "p.h"),
            ("x.h", "p.h\nint main() {}"),
            ("", "p.h"),
        ] {
            assert!(matches!(
                generate(&plan, &options(header, prelude)),
                Err(CodegenError::InvalidInclude(_))
            ));
        }
    }

    #[test]
    fn unique_appends_a_counter() {
        let mut used = HashSet::from(["a".to_owned(), "a_2".to_owned()]);
        assert_eq!(unique(&mut used, "a".to_owned()), "a_3");
        assert_eq!(unique(&mut used, "b".to_owned()), "b");
    }
}
