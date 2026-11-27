//! `shim.h` / `shim.cc`: helpers for C++ items cxx cannot call directly.

use std::fmt::Write as _;

use ferrobridge_analysis::{
    BridgeType, Callee, ClassPlan, ConstantPlan, ConstantValue, Pass, Returns,
};

use crate::bridge::SHIM_SELF;
use crate::spelling::{cpp_return, cpp_type};
use crate::{CodegenOptions, ShimFn};

/// The handler that catches all C++ exceptions and passes them to Rust as `Err`. cxx uses it for
/// every call returning `Result<T>`; its default catches only `std::exception`, and the program
/// would end on other exceptions (safety rule 4). Guarded so that it is defined once if several
/// shims are included in the same C++ file.
const TRYCATCH: &str = r#"#ifndef FERROBRIDGE_TRYCATCH_DEFINED
#define FERROBRIDGE_TRYCATCH_DEFINED
namespace rust {
namespace behavior {
template <typename Try, typename Fail>
static void trycatch(Try &&func, Fail &&fail) noexcept try {
  func();
} catch (const ::std::exception &e) {
  fail(e.what());
} catch (...) {
  fail("unknown C++ exception (not derived from std::exception)");
}
}  // namespace behavior
}  // namespace rust
#endif  // FERROBRIDGE_TRYCATCH_DEFINED
"#;

pub(crate) fn header(
    options: &CodegenOptions,
    shims: &[ShimFn<'_>],
    classes: &[ClassPlan],
) -> String {
    let mut out = String::new();
    out.push_str("#pragma once\n\n");
    out.push_str(
        "#include <cstdint>\n#include <exception>\n#include <memory>\n#include <stdexcept>\n\
         #include <string>\n#include <vector>\n\n",
    );
    if classes.iter().any(|c| c.value.is_some()) {
        // For checking the layout of value types (`shim.cc`).
        out.push_str("#include <cstddef>\n#include <type_traits>\n\n");
    }
    if shims.iter().any(|shim| !shim.plan.lengths.is_empty()) {
        // For `rust::Slice` and the length check.
        out.push_str("#include <limits>\n\n#include \"rust/cxx.h\"\n\n");
    }
    for prelude in &options.prelude {
        let _ = writeln!(out, "#include \"{prelude}\"  // prelude");
    }
    for header in &options.header_includes {
        let _ = writeln!(out, "#include \"{header}\"");
    }
    out.push('\n');
    out.push_str(TRYCATCH);
    let _ = writeln!(out, "\nnamespace {} {{\n", options.shim_namespace);
    // So that template instantiations have a name cxx can use.
    let mut aliases = false;
    for class in classes {
        if let Some(spelling) = &class.instance_spelling {
            let _ = writeln!(out, "using {} = ::{spelling};", class.type_ref.rust_name);
            aliases = true;
        }
    }
    if aliases {
        out.push('\n');
    }
    for shim in shims {
        let _ = writeln!(out, "{};", signature(shim));
    }
    if !shims.is_empty() {
        out.push('\n');
    }
    let _ = writeln!(out, "}}  // namespace {}", options.shim_namespace);
    out
}

pub(crate) fn source(
    options: &CodegenOptions,
    shims: &[ShimFn<'_>],
    constants: &[ConstantPlan],
    classes: &[ClassPlan],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#include \"{}\"\n", options.shim_include);
    out.push_str(&constant_checks(constants));
    out.push_str(&layout_checks(classes, &options.shim_namespace));
    let _ = writeln!(out, "namespace {} {{\n", options.shim_namespace);
    for shim in shims {
        let _ = writeln!(
            out,
            "{} {{\n  {}\n}}\n",
            signature(shim),
            body(shim).join("\n  ")
        );
    }
    let _ = writeln!(out, "}}  // namespace {}", options.shim_namespace);
    out
}

/// The C++ compiler re-checks the values of integer and `bool` constants written to Rust as `const`.
/// If a generated file is compiled on another platform or with other defines and the value has
/// changed, there is a compile error instead of a silently wrong `const`.
///
/// Floating-point constants declared `static const double` are not C++ constant expressions and
/// cannot be checked here; the initializer check (`ferrobridge-parser`) protects them.
fn constant_checks(constants: &[ConstantPlan]) -> String {
    let mut out = String::new();
    for constant in constants {
        let value = match constant.value {
            ConstantValue::Bool(v) => v.to_string(),
            ConstantValue::Signed(i64::MIN) => "(-9223372036854775807LL - 1)".to_owned(),
            ConstantValue::Signed(v) => format!("{v}LL"),
            ConstantValue::Unsigned(v) => format!("{v}ULL"),
            ConstantValue::Float(_) => continue,
        };
        let mut path = constant.namespace.clone();
        path.push(constant.cpp_name.clone());
        let path = path.join("::");
        let _ = writeln!(
            out,
            "static_assert(::{path} == {value}, \"ferrobridge: `{path}` differs from the value it was \
             generated with; regenerate the bridge\");"
        );
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Is the Rust `#[repr(C)]` layout of value types the same as in C++? If the analysis is wrong, a
/// compile error instead of silent UB (ADR-0010).
///
/// For instantiations, the alias in the shim is used: `offsetof` is a macro, and the comma in a
/// spelling such as `Pair<int, float>` splits its arguments.
fn layout_checks(classes: &[ClassPlan], shim_namespace: &str) -> String {
    let mut out = String::new();
    for class in classes {
        let Some(layout) = &class.value else {
            continue;
        };
        let alias = format!("::{shim_namespace}::{}", class.type_ref.rust_name);
        let ty = if class.instance_spelling.is_some() {
            &alias
        } else {
            &class.type_ref.cpp_path
        };
        let name = class.type_ref.cpp_path.trim_start_matches("::");
        let message = |what: &str| {
            format!("\"ferrobridge: `{name}` {what}; it cannot be bound as a value type in Rust\"")
        };
        let _ = writeln!(
            out,
            "static_assert(::std::is_trivially_copyable<{ty}>::value, {});",
            message("is not trivially copyable")
        );
        let _ = writeln!(
            out,
            "static_assert(::std::is_standard_layout<{ty}>::value, {});",
            message("is not standard-layout")
        );
        let _ = writeln!(
            out,
            "static_assert(sizeof({ty}) == {} && alignof({ty}) == {}, {});",
            layout.size_bytes,
            layout.align_bytes,
            message("has a different size from when it was generated; regenerate the bridge")
        );
        for field in &layout.fields {
            let _ = writeln!(
                out,
                "static_assert(offsetof({ty}, {}) == {}, {});",
                field.cpp_name,
                field.offset_bytes,
                message(&format!(
                    "field `{}` is in a different place from when it was generated",
                    field.cpp_name
                ))
            );
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn signature(shim: &ShimFn<'_>) -> String {
    let plan = shim.plan;
    let mut params = Vec::new();
    if let Callee::Method {
        class,
        receiver_mutable,
        ..
    } = &plan.callee
    {
        let constness = if *receiver_mutable { "" } else { "const " };
        params.push(format!("{constness}{}& {SHIM_SELF}", class.cpp_path));
    }
    for param in &plan.params {
        params.push(format!("{} {}", cpp_type(&param.ty), param.rust_name));
    }
    let noexcept = if plan.fallible { "" } else { " noexcept" };
    format!(
        "{} {}({}){noexcept}",
        cpp_return(plan),
        shim.name,
        params.join(", ")
    )
}

/// The lines of a shim body.
fn body(shim: &ShimFn<'_>) -> Vec<String> {
    let mut lines = length_checks(shim);
    lines.extend(call_lines(shim));
    lines
}

/// The arguments passed to the C++ function, in C++ order. Length parameters missing from the Rust
/// signature are filled from the slice's length.
fn arguments(shim: &ShimFn<'_>) -> String {
    let plan = shim.plan;
    let mut params = plan
        .params
        .iter()
        .map(|param| match (&param.ty, param.pass) {
            // A `borrowed` pointer parameter: Rust gave a reference, C++ expects a pointer.
            (_, Pass::AddressOf) => format!("&{}", param.rust_name),
            (_, Pass::Slice) => format!("{}.data()", param.rust_name),
            (BridgeType::UniquePtr(_), Pass::Direct) => {
                format!("::std::move({})", param.rust_name)
            }
            (_, Pass::Direct) => param.rust_name.clone(),
        });
    let mut args = Vec::with_capacity(plan.cpp_arity());
    for position in 0..plan.cpp_arity() {
        let length = plan.lengths.iter().find(|l| l.position == position);
        let arg = match length {
            Some(length) => format!(
                "static_cast<{}>({}.size())",
                length.cpp_type, plan.params[length.slice].rust_name
            ),
            None => params.next().unwrap_or_default(),
        };
        args.push(arg);
    }
    args.join(", ")
}

/// Every slice's length must fit the C++ length type; if not, it is not truncated silently, an exception →
/// `Err` in Rust. The comparison is done in `unsigned long long`: both sides are non-negative and
/// no value is truncated.
fn length_checks(shim: &ShimFn<'_>) -> Vec<String> {
    let plan = shim.plan;
    let mut lines = Vec::new();
    for length in &plan.lengths {
        let slice = &plan.params[length.slice].rust_name;
        lines.push(format!(
            "if (static_cast<unsigned long long>({slice}.size()) > \
             static_cast<unsigned long long>(::std::numeric_limits<{}>::max())) {{",
            length.cpp_type
        ));
        // `ferrobridge::Error` already adds the item's name.
        lines.push(format!(
            "  throw ::std::length_error(\"`{slice}` is too long for the type of its length parameter \
             ({})\");",
            length.cpp_type
        ));
        lines.push("}".to_owned());
    }
    lines
}

/// The call and return lines.
fn call_lines(shim: &ShimFn<'_>) -> Vec<String> {
    let plan = shim.plan;
    let args = arguments(shim);
    let call = match &plan.callee {
        // The constructor of a value type returns the object by value.
        Callee::Constructor { class }
            if matches!(plan.returns, Returns::Value(BridgeType::Value(_))) =>
        {
            return vec![format!("return {}({args});", class.cpp_path)];
        }
        Callee::Constructor { class } => {
            return vec![format!(
                "return ::std::make_unique<{}>({args});",
                class.cpp_path
            )];
        }
        Callee::Static { class, cpp_name } => format!("{}::{cpp_name}({args})", class.cpp_path),
        Callee::Method { cpp_name, .. } => format!("{SHIM_SELF}.{cpp_name}({args})"),
        Callee::Free {
            namespace,
            cpp_name,
        } => {
            let mut path = namespace.clone();
            path.push(cpp_name.clone());
            format!("::{}({args})", path.join("::"))
        }
    };
    let line = match &plan.returns {
        Returns::Unit => format!("{call};"),
        Returns::Value(_) => format!("return {call};"),
        Returns::BoxedClass(class) => {
            format!("return ::std::make_unique<{}>({call});", class.cpp_path)
        }
        Returns::OwnedString => format!("return ::std::make_unique<::std::string>({call});"),
        Returns::OwnedVector(element) => format!(
            "return ::std::make_unique<::std::vector<{}>>({call});",
            cpp_type(element)
        ),
        // Based on an annotation: the pointer was allocated with `new`. A null becomes an empty `unique_ptr`.
        Returns::OwnedPointer(inner) => {
            format!("return ::std::unique_ptr<{}>({call});", cpp_type(inner))
        }
        // A null pointer is never turned into a reference: an exception → `Err` in Rust.
        Returns::BorrowedPointer { .. } => {
            return vec![
                format!("auto* fb_result = {call};"),
                "if (fb_result == nullptr) {".to_owned(),
                format!(
                    "  throw ::std::runtime_error(\"{} returned a null pointer (a `borrowed` return)\");",
                    callee_name(&plan.callee)
                ),
                "}".to_owned(),
                "return *fb_result;".to_owned(),
            ];
        }
    };
    vec![line]
}

/// The full C++ name of the called item, for error messages (`own::Widget::child`).
fn callee_name(callee: &Callee) -> String {
    let name = match callee {
        Callee::Method {
            class, cpp_name, ..
        }
        | Callee::Static { class, cpp_name } => {
            format!("{}::{cpp_name}", class.cpp_path)
        }
        Callee::Constructor { class } => class.cpp_path.clone(),
        Callee::Free {
            namespace,
            cpp_name,
        } => {
            let mut path = namespace.clone();
            path.push(cpp_name.clone());
            path.join("::")
        }
    };
    name.trim_start_matches("::").to_owned()
}
