//! `shim.h` / `shim.cc`: cxx'in doğrudan çağıramadığı C++ öğeleri için yardımcılar.

use std::fmt::Write as _;

use ferrobridge_analysis::{BridgeType, Callee, ConstantPlan, ConstantValue, Pass, Returns};

use crate::bridge::SHIM_SELF;
use crate::spelling::{cpp_return, cpp_type};
use crate::{CodegenOptions, ShimFn};

/// Bütün C++ exception'larını yakalayıp Rust'a `Err` olarak ileten işleyici. cxx, `Result<T>`
/// dönen her çağrıda bunu kullanır; varsayılanı yalnızca `std::exception` yakalar ve diğer
/// exception'larda program sonlanırdı (güvenlik kuralı 4). Birden fazla shim aynı C++
/// dosyasına dahil edilirse tek kez tanımlansın diye korumalı.
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
  fail("bilinmeyen C++ exception'ı (std::exception türevi değil)");
}
}  // namespace behavior
}  // namespace rust
#endif  // FERROBRIDGE_TRYCATCH_DEFINED
"#;

pub(crate) fn header(options: &CodegenOptions, shims: &[ShimFn<'_>]) -> String {
    let mut out = String::new();
    out.push_str("#pragma once\n\n");
    out.push_str(
        "#include <cstdint>\n#include <exception>\n#include <memory>\n#include <stdexcept>\n\
         #include <string>\n#include <vector>\n\n",
    );
    if shims.iter().any(|shim| !shim.plan.lengths.is_empty()) {
        // `rust::Slice` ve uzunluk kontrolü için.
        out.push_str("#include <limits>\n\n#include \"rust/cxx.h\"\n\n");
    }
    for prelude in &options.prelude {
        let _ = writeln!(out, "#include \"{prelude}\"  // prelude");
    }
    let _ = writeln!(out, "#include \"{}\"\n", options.header_include);
    out.push_str(TRYCATCH);
    let _ = writeln!(out, "\nnamespace {} {{\n", options.shim_namespace);
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
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#include \"{}\"\n", options.shim_include);
    out.push_str(&constant_checks(constants));
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

/// Rust'a `const` olarak yazılan tamsayı ve `bool` sabitlerin değerini C++ derleyicisi yeniden
/// doğrular. Üretilmiş dosya başka bir platformda ya da başka define'larla derlenirse ve değer
/// değiştiyse, sessizce yanlış bir `const` yerine derleme hatası çıkar.
///
/// Kayan noktalı sabitler `static const double` olarak C++ sabit ifadesi değildir, burada
/// doğrulanamaz; onları başlatıcı denetimi korur (`ferrobridge-parser`).
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
            "static_assert(::{path} == {value}, \"ferrobridge: `{path}` üretildiği değerden \
             farklı; bridge'i yeniden üretin\");"
        );
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

/// Shim gövdesinin satırları.
fn body(shim: &ShimFn<'_>) -> Vec<String> {
    let mut lines = length_checks(shim);
    lines.extend(call_lines(shim));
    lines
}

/// C++ fonksiyonuna geçirilen argümanlar, C++'taki sırasıyla. Rust imzasında olmayan uzunluk
/// parametreleri slice'ın boyundan doldurulur.
fn arguments(shim: &ShimFn<'_>) -> String {
    let plan = shim.plan;
    let mut params = plan
        .params
        .iter()
        .map(|param| match (&param.ty, param.pass) {
            // `borrowed` pointer parametresi: Rust referans verdi, C++ pointer bekliyor.
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

/// Her slice'ın boyu C++ uzunluk tipine sığmalı; sığmazsa sessizce kırpılmaz, exception →
/// Rust'ta `Err`. Karşılaştırma `unsigned long long`'da yapılır: iki taraf da negatif değil ve
/// hiçbir değer kesilmez.
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
        lines.push(format!(
            "  throw ::std::length_error(\"{}: `{slice}` uzunluk parametresinin tipine ({}) \
             sığmayacak kadar uzun\");",
            callee_name(&plan.callee),
            length.cpp_type
        ));
        lines.push("}".to_owned());
    }
    lines
}

/// Çağrı ve dönüş satırları.
fn call_lines(shim: &ShimFn<'_>) -> Vec<String> {
    let plan = shim.plan;
    let args = arguments(shim);
    let call = match &plan.callee {
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
        // Açıklamaya dayanır: pointer `new` ile ayrıldı. Null ise boş `unique_ptr` olur.
        Returns::OwnedPointer(inner) => {
            format!("return ::std::unique_ptr<{}>({call});", cpp_type(inner))
        }
        // Null bir pointer asla referansa çevrilmez: exception → Rust'ta `Err`.
        Returns::BorrowedPointer { .. } => {
            return vec![
                format!("auto* fb_result = {call};"),
                "if (fb_result == nullptr) {".to_owned(),
                format!(
                    "  throw ::std::runtime_error(\"{} null pointer döndürdü (`borrowed` dönüş)\");",
                    callee_name(&plan.callee)
                ),
                "}".to_owned(),
                "return *fb_result;".to_owned(),
            ];
        }
    };
    vec![line]
}

/// Hata mesajları için çağrılan öğenin tam C++ adı (`own::Widget::child`).
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
