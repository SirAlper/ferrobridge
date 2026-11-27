//! Names on the Rust side (ADR-0005).
//!
//! - A name defined once stays as it is.
//! - If the same C++ name exists in different namespaces, it gets a namespace prefix (`geo_length`).
//! - *All* overloads get a suffix derived from the parameter types (`clamp_i32`).
//! - Rust keywords are escaped with `_` (`type_`); the C++ name is kept with `cxx_name`.
//! - Names that still clash after all this get a sequence number (`_2`).

use std::collections::{HashMap, HashSet};

use crate::plan::{BridgeType, Callee, FunctionPlan};

/// Rust's strict and reserved keywords.
const RUST_KEYWORDS: [&str; 52] = [
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Names that already mean something in the bridge module; C++ types with these names get a `Cpp` prefix.
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

/// Adds `_` at the end if it is a keyword.
pub(crate) fn escape(name: &str) -> String {
    if is_rust_keyword(name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// A parameter name: keywords avoided; `argN` by position if unnamed.
pub(crate) fn param_name(name: Option<&str>, index: usize) -> String {
    match name {
        Some(name) if !name.is_empty() => escape(name),
        _ => format!("arg{index}"),
    }
}

/// Class/enum names: if the same name exists in more than one namespace, the namespaces become a
/// PascalCase prefix (`PhysicsVehicle`); names reserved in the bridge get a `Cpp` prefix.
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

/// The short name of a type for an overload suffix.
fn type_token(ty: &BridgeType) -> String {
    match ty {
        BridgeType::Bool => "bool".to_owned(),
        BridgeType::Char => "char".to_owned(),
        BridgeType::Int(int) => int.rust_name().to_owned(),
        BridgeType::F32 => "f32".to_owned(),
        BridgeType::F64 => "f64".to_owned(),
        BridgeType::Class(r) | BridgeType::Enum(r) | BridgeType::Value(r) => r.rust_name.clone(),
        BridgeType::String => "string".to_owned(),
        BridgeType::UniquePtr(inner) => format!("uptr_{}", type_token(inner)),
        BridgeType::Vector(inner) => format!("vec_{}", type_token(inner)),
        BridgeType::Slice { mutable, element } => {
            let token = type_token(element);
            if *mutable {
                format!("mut_slice_{token}")
            } else {
                format!("slice_{token}")
            }
        }
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

/// Adds a suffix to all those carrying the same base name, then escapes keywords.
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

/// Functions in the bridge's flat namespace: free functions, constructors and
/// static methods. Class names must have been decided beforehand with `type_names`.
pub(crate) fn name_free_scope(plans: &mut [&mut FunctionPlan]) {
    // In how many different namespaces does the same C++ name exist?
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

/// The (non-static) methods of a class; names only need to be unique within the class.
pub(crate) fn name_methods(plans: &mut [&mut FunctionPlan]) {
    let bases: Vec<String> = plans
        .iter()
        .map(|plan| match (&plan.callee, plan.operator) {
            (_, Some(op)) => op.method_name().to_owned(),
            (Callee::Method { cpp_name, .. }, None) => cpp_name.clone(),
            _ => plan.rust_name.clone(),
        })
        .collect();
    apply_overloads(plans, bases);
    dedupe(plans);
}

/// A sequence number for names that still clash (e.g. `f(int)` and `f(const int&)` produce the same suffix).
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
