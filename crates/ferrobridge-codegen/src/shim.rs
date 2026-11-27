//! `shim.h` / `shim.cc`: cxx'in doğrudan çağıramadığı C++ öğeleri için yardımcılar.

use std::fmt::Write as _;

use ferrobridge_analysis::{
    BridgeType, Callee, ClassPlan, ConstantPlan, ConstantValue, Pass, Returns,
};

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
        // Değer tiplerinin düzen doğrulaması için (`shim.cc`).
        out.push_str("#include <cstddef>\n#include <type_traits>\n\n");
    }
    if shims.iter().any(|shim| !shim.plan.lengths.is_empty()) {
        // `rust::Slice` ve uzunluk kontrolü için.
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
    // Template örneklemelerinin cxx'in kullanabileceği bir adı olsun.
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

/// Değer tiplerinin Rust'taki `#[repr(C)]` düzeni C++'takiyle aynı mı? Analiz yanılırsa sessiz
/// UB yerine derleme hatası (ADR-0010).
///
/// Örneklemelerde shim'deki takma ad kullanılır: `offsetof` bir makro ve `Pair<int, float>`
/// gibi bir yazımdaki virgül argümanları böler.
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
            format!("\"ferrobridge: `{name}` {what}; Rust'ta değer tipi olarak bağlanamaz\"")
        };
        let _ = writeln!(
            out,
            "static_assert(::std::is_trivially_copyable<{ty}>::value, {});",
            message("bit bit kopyalanabilir değil")
        );
        let _ = writeln!(
            out,
            "static_assert(::std::is_standard_layout<{ty}>::value, {});",
            message("standart düzende değil")
        );
        let _ = writeln!(
            out,
            "static_assert(sizeof({ty}) == {} && alignof({ty}) == {}, {});",
            layout.size_bytes,
            layout.align_bytes,
            message("üretildiğinden farklı boyutta; bridge'i yeniden üretin")
        );
        for field in &layout.fields {
            let _ = writeln!(
                out,
                "static_assert(offsetof({ty}, {}) == {}, {});",
                field.cpp_name,
                field.offset_bytes,
                message(&format!(
                    "`{}` alanı üretildiğinden farklı yerde",
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
        // Öğenin adını `ferrobridge::Error` zaten ekliyor.
        lines.push(format!(
            "  throw ::std::length_error(\"`{slice}` uzunluk parametresinin tipine ({}) \
             sığmayacak kadar uzun\");",
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
        // Değer tipinin constructor'ı nesneyi değerle döndürür.
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
