//! Bağlama planından kod üretimi (ADR-0002, ADR-0004).
//!
//! Üç dosya üretilir:
//! - `bridge.rs`: `#[cxx::bridge]` modülü (cxx'in derleyeceği Rust tarafı).
//! - `shim.h` / `shim.cc`: cxx'in doğrudan çağıramadığı şeyler için C++ yardımcıları
//!   (constructor fabrikaları, statik metotlar, değerle dönen sınıf, `std::string` ve
//!   `std::vector`'ler, açıklamalı pointer'lar) ve
//!   bütün C++ exception'larını yakalayan `rust::behavior::trycatch` (güvenlik kuralı 4).
//!
//! Üretilen dosyalar kullanıcı tarafından görülür, incelenir ve commit'lenir (ADR-0004).

mod bridge;
mod safe;
mod shim;
mod spelling;

use std::collections::HashSet;

use ferrobridge_analysis::{BridgePlan, Callee, FunctionPlan, TypeRef};

/// Kod üretimi ayarları.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenOptions {
    /// Shim'in orijinal header'ı `#include` ederken kullanacağı yol (ör. `"mylib/vehicle.h"`).
    pub header_include: String,
    /// Bridge'in ve `shim.cc`'nin `shim.h`'yi `#include` ederken kullanacağı yol.
    pub shim_include: String,
    /// Shim fonksiyonlarının C++ namespace'i (ör. `"ferrobridge::vehicle"`). Aynı programdaki
    /// farklı header'ların shim'leri çakışmasın diye header'a özgü olmalı.
    pub shim_namespace: String,
}

/// Üretilen dosyaların içerikleri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFiles {
    /// `bridge.rs`
    pub bridge_rs: String,
    /// `shim.h`
    pub shim_h: String,
    /// `shim.cc`
    pub shim_cc: String,
}

/// Kod üretimi hataları.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// Ayar olarak verilen namespace geçerli bir C++ namespace yolu değil.
    #[error("geçersiz shim namespace'i: `{0}` (ör. `ferrobridge::vehicle`)")]
    InvalidNamespace(String),
    /// Bir ad Rust tanımlayıcısı olarak kullanılamıyor.
    #[error("Rust tanımlayıcısı olarak kullanılamayan ad: `{0}`")]
    InvalidIdentifier(String),
    /// Üretilen Rust kodu ayrıştırılamadı; bu ferrobridge'de bir hatadır.
    #[error("üretilen bridge kodu geçersiz (ferrobridge hatası, lütfen bildirin): {0}")]
    Internal(String),
}

/// Planı kullanarak üç dosyayı üretir.
///
/// # Errors
///
/// Ayarlar geçersizse ya da bir ad Rust'ta kullanılamıyorsa hata döner.
pub fn generate(
    plan: &BridgePlan,
    options: &CodegenOptions,
) -> Result<GeneratedFiles, CodegenError> {
    if !is_cpp_namespace(&options.shim_namespace) {
        return Err(CodegenError::InvalidNamespace(
            options.shim_namespace.clone(),
        ));
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
            shim::header(options, &shims)
        ),
        shim_cc: format!(
            "{}\n{}",
            comment(&header, "//"),
            shim::source(options, &shims)
        ),
    })
}

/// C++ shim üzerinden çağrılan bir fonksiyon. Rust'taki ve C++'taki adı aynıdır.
pub(crate) struct ShimFn<'p> {
    pub(crate) name: String,
    pub(crate) plan: &'p FunctionPlan,
}

/// Shim gerektiren bütün fonksiyonlar, bridge'deki sırayla.
///
/// Serbest kapsamdakilerin (serbest fonksiyon, constructor, statik metot) adları analizde
/// zaten benzersiz yapıldı. Metot shim'leri `<Sınıf>_<metot>` adını alır ve gerekirse
/// sıra numarasıyla benzersizleştirilir.
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

/// Üretilen her dosyanın başındaki açıklama: nereden üretildiği ve neyin bağlanmadığı.
fn header_comment(plan: &BridgePlan, options: &CodegenOptions) -> Vec<String> {
    let mut lines = vec![
        "@generated by ferrobridge — elle düzenlemeyin; header değişince yeniden üretin."
            .to_owned(),
        format!("Kaynak header: {}", options.header_include),
    ];
    if !plan.skipped.is_empty() {
        lines.push(String::new());
        lines.push(format!("Bağlanmayan öğeler ({}):", plan.skipped.len()));
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
            "Açıklamaya dayanan varsayımlar ({}). Bunlar `ferrobridge.toml`'daki sözlerdir; biri",
            plan.assumptions.len()
        ));
        lines.push("yanlışsa üretilen kod tanımsız davranışa yol açabilir:".to_owned());
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
    fn unique_appends_a_counter() {
        let mut used = HashSet::from(["a".to_owned(), "a_2".to_owned()]);
        assert_eq!(unique(&mut used, "a".to_owned()), "a_3");
        assert_eq!(unique(&mut used, "b".to_owned()), "b");
    }
}
