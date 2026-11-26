//! Rust tarafındaki adlar (ADR-0005).
//!
//! - Tek tanımlı isim olduğu gibi kalır.
//! - Aynı C++ adı farklı namespace'lerde varsa namespace ön eki alır (`geo_length`).
//! - Overload'ların *hepsi* parametre tiplerinden türetilen sonek alır (`clamp_i32`).
//! - Rust anahtar kelimeleri `_` ile kaçılır (`type_`); C++ adı `cxx_name` ile korunur.
//! - Tüm bunlardan sonra hâlâ çakışan adlara sıra numarası eklenir (`_2`).

use std::collections::{HashMap, HashSet};

use crate::plan::{BridgeType, Callee, FunctionPlan};

/// Rust'ın katı ve ileride kullanılmak üzere ayrılmış anahtar kelimeleri.
const RUST_KEYWORDS: [&str; 52] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Bridge modülünde zaten anlamı olan adlar; aynı adlı C++ tipleri `Cpp` ön eki alır.
const RESERVED_TYPE_NAMES: [&str; 12] = [
    "String",
    "Str",
    "Box",
    "Vec",
    "Result",
    "Option",
    "Pin",
    "UniquePtr",
    "SharedPtr",
    "WeakPtr",
    "CxxString",
    "CxxVector",
];

pub(crate) fn is_rust_keyword(name: &str) -> bool {
    RUST_KEYWORDS.contains(&name)
}

/// Anahtar kelimeyse sonuna `_` ekler.
pub(crate) fn escape(name: &str) -> String {
    if is_rust_keyword(name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// Parametre adı: anahtar kelimelerden kaçınılmış; isimsizse konumuna göre `argN`.
pub(crate) fn param_name(name: Option<&str>, index: usize) -> String {
    match name {
        Some(name) if !name.is_empty() => escape(name),
        _ => format!("arg{index}"),
    }
}

/// Sınıf/enum adları: aynı ad birden fazla namespace'te varsa namespace'ler PascalCase ön ek
/// olur (`PhysicsVehicle`); bridge'de ayrılmış adlar `Cpp` ön eki alır.
pub(crate) fn type_names(types: &[(Vec<String>, String)]) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (_, name) in types {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    types
        .iter()
        .map(|(namespace, name)| {
            let base = if counts[name.as_str()] > 1 {
                let prefix: String = namespace.iter().map(|s| pascal(s)).collect();
                format!("{prefix}{name}")
            } else {
                name.clone()
            };
            if RESERVED_TYPE_NAMES.contains(&base.as_str()) || is_rust_keyword(&base) {
                format!("Cpp{base}")
            } else {
                base
            }
        })
        .collect()
}

fn pascal(segment: &str) -> String {
    segment
        .split('_')
        .filter(|part| part.chars().all(|c| c.is_ascii_alphanumeric()) && !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// Overload soneki için bir tipin kısa adı.
fn type_token(ty: &BridgeType) -> String {
    match ty {
        BridgeType::Bool => "bool".to_owned(),
        BridgeType::Char => "char".to_owned(),
        BridgeType::Int(int) => int.rust_name().to_owned(),
        BridgeType::F32 => "f32".to_owned(),
        BridgeType::F64 => "f64".to_owned(),
        BridgeType::Class(r) | BridgeType::Enum(r) => r.rust_name.clone(),
        BridgeType::String => "string".to_owned(),
        BridgeType::UniquePtr(inner) => format!("uptr_{}", type_token(inner)),
        BridgeType::Vector(inner) => format!("vec_{}", type_token(inner)),
        BridgeType::Ref { mutable, inner } => {
            if *mutable {
                format!("mut_{}", type_token(inner))
            } else {
                type_token(inner)
            }
        }
    }
}

fn overload_suffix(plan: &FunctionPlan) -> String {
    if plan.params.is_empty() {
        "noargs".to_owned()
    } else {
        plan.params
            .iter()
            .map(|p| type_token(&p.ty))
            .collect::<Vec<_>>()
            .join("_")
    }
}

/// Aynı taban adı taşıyanların hepsine sonek ekler, sonra anahtar kelimeleri kaçar.
fn apply_overloads(plans: &mut [&mut FunctionPlan], bases: Vec<String>) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for base in &bases {
        *counts.entry(base.as_str()).or_default() += 1;
    }
    let names: Vec<String> = bases
        .iter()
        .zip(plans.iter())
        .map(|(base, plan)| {
            let name = if counts[base.as_str()] > 1 {
                format!("{base}_{}", overload_suffix(plan))
            } else {
                base.clone()
            };
            escape(&name)
        })
        .collect();
    for (plan, name) in plans.iter_mut().zip(names) {
        plan.rust_name = name;
    }
}

/// Bridge'in düz ad alanındaki fonksiyonlar: serbest fonksiyonlar, constructor'lar ve
/// statik metotlar. Sınıf adları `type_names` ile önceden belirlenmiş olmalı.
pub(crate) fn name_free_scope(plans: &mut [&mut FunctionPlan]) {
    // Aynı C++ adı kaç farklı namespace'te var?
    let mut namespaces: HashMap<String, HashSet<Vec<String>>> = HashMap::new();
    for plan in plans.iter() {
        if let Callee::Free {
            namespace,
            cpp_name,
        } = &plan.callee
        {
            namespaces
                .entry(cpp_name.clone())
                .or_default()
                .insert(namespace.clone());
        }
    }
    let bases: Vec<String> = plans
        .iter()
        .map(|plan| match &plan.callee {
            Callee::Free {
                namespace,
                cpp_name,
            } => {
                if namespaces[cpp_name].len() > 1 && !namespace.is_empty() {
                    format!("{}_{cpp_name}", namespace.join("_"))
                } else {
                    cpp_name.clone()
                }
            }
            Callee::Constructor { class } => format!("{}_new", class.rust_name),
            Callee::Static { class, cpp_name } => format!("{}_{cpp_name}", class.rust_name),
            Callee::Method { cpp_name, .. } => cpp_name.clone(),
        })
        .collect();
    apply_overloads(plans, bases);
    dedupe(plans);
}

/// Bir sınıfın (statik olmayan) metotları; adlar yalnızca sınıf içinde benzersiz olmalı.
pub(crate) fn name_methods(plans: &mut [&mut FunctionPlan]) {
    let bases: Vec<String> = plans
        .iter()
        .map(|plan| match &plan.callee {
            Callee::Method { cpp_name, .. } => cpp_name.clone(),
            _ => plan.rust_name.clone(),
        })
        .collect();
    apply_overloads(plans, bases);
    dedupe(plans);
}

/// Hâlâ çakışan adlara (ör. `f(int)` ve `f(const int&)` aynı soneki üretir) sıra numarası.
fn dedupe(plans: &mut [&mut FunctionPlan]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for plan in plans.iter_mut() {
        let count = seen.entry(plan.rust_name.clone()).or_default();
        *count += 1;
        if *count > 1 {
            plan.rust_name = format!("{}_{count}", plan.rust_name);
        }
    }
}
