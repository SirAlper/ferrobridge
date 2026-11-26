//! IR üzerinde bağlanabilirlik analizi.
//!
//! [`check`] her öğe, parametre ve dönüş tipi için cxx'te güvenle ifade edilip
//! edilemeyeceğine karar verir ve kod üretiminin tükettiği bir [`BridgePlan`] üretir.
//! Kurallar Obsidian `03 Mimari` notundaki tip tablosu ve güvenlik kurallarıdır. Bir
//! fonksiyonun tek bir parametresi bile desteklenmiyorsa fonksiyon **tümüyle** atlanır ve
//! nedeni bildirilir; yarım bağlama yapılmaz.
//!
//! Header'ın söylemediği sahiplik bilgisi [`Annotations`]'tan (`ferrobridge.toml`) gelir.
//! Kullanılan her açıklama planda bir [`Assumption`] olarak görünür; açıklama eksikliği
//! yüzünden bağlanamayan her yer bir [`Suggestion`] olur.

mod config;
mod naming;
mod plan;
mod rules;
mod suggest;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ferrobridge_ir::{Class, Enum, Function, Item, Module, Param, QualType, SourceLocation};

pub use config::{
    Annotations, ConfigError, FunctionAnnotation, Located, ParamAnnotation, ReturnAnnotation,
};
pub use plan::{
    AnnotationIssue, Assumption, BridgePlan, BridgeType, Callee, ClassPlan, EnumPlan, FunctionPlan,
    IntType, ParamPlan, Pass, Returns, Skipped, Suggestion, SuggestionTarget, TypeRef,
};
pub use suggest::render_suggestions;

use rules::{Known, KnownClass, Need};

const REASON_ABSTRACT: &str = "Sınıf soyut (saf sanal metodu var); doğrudan oluşturulamaz.";
const NOTE_NO_CONSTRUCTOR: &str = "Açıkça bildirilmiş public constructor yok; Rust'ta yalnızca \
     başka fonksiyonların döndürdüğü nesnelerle kullanılabilir.";
const REASON_ANONYMOUS_NAMESPACE: &str =
    "Anonim namespace'teki tipler header dışından kullanılamaz.";

const PROMISE_BORROWED_POINTER: &str = "pointer tek bir nesneyi gösteriyor (dizi değil), null \
     değil, yalnızca çağrı süresince kullanılıyor ve saklanmıyor";
const PROMISE_BORROWED_REF: &str = "referans yalnızca çağrı süresince kullanılıyor ve saklanmıyor";
const PROMISE_OWNED: &str = "pointer `new` ile ayrıldı ve sahipliği çağırana geçiyor; Rust'ta \
     `UniquePtr` düşünce `delete` edilir";
const PROMISE_BORROWED_RETURN: &str = "pointer nesnenin sahip olduğu bir şeyi gösteriyor ve \
     nesne yaşadıkça geçerli; silinmez (null gelirse `Err` döner)";
const PROMISE_BORROWED_REF_RETURN: &str =
    "referans nesnenin sahip olduğu bir şeyi gösteriyor ve nesne yaşadıkça geçerli";

/// IR modülünün bağlama planını çıkarır.
///
/// `annotations` header'daki hiçbir öğeye uymuyorsa (yazım hatası, olmayan parametre...)
/// plan üretilmez; bütün uyumsuzluklar birlikte döner.
pub fn check(
    module: &Module,
    annotations: &Annotations,
) -> Result<BridgePlan, Vec<AnnotationIssue>> {
    let mut checker = Checker {
        known: Known::default(),
        annotations,
        plan: BridgePlan::default(),
        seen: BTreeMap::new(),
        returns_seen: BTreeSet::new(),
        applied_overloads: BTreeMap::new(),
    };
    checker.known = known_types(module, &mut checker.plan);

    for item in &module.items {
        match item {
            Item::Function(function) => {
                if let Some(bound) = checker.free_function(function) {
                    checker.plan.functions.push(bound);
                }
            }
            Item::Class(class) => {
                if let Some(bound) = checker.class_plan(class) {
                    checker.plan.classes.push(bound);
                }
            }
            Item::Enum(_) => {} // `known_types` içinde işlendi.
            Item::Unsupported(unsupported) => checker.plan.skipped.push(Skipped {
                name: unsupported.name.clone(),
                reason: unsupported.reason.clone(),
                location: unsupported.location.clone(),
            }),
        }
    }

    let errors = checker.unmatched_annotations();
    if !errors.is_empty() {
        return Err(errors);
    }
    checker.warn_shared_overloads();
    let mut plan = checker.plan;
    assign_names(&mut plan);
    Ok(plan)
}

/// [`check`] boyunca taşınan durum.
struct Checker<'a> {
    known: Known,
    annotations: &'a Annotations,
    plan: BridgePlan,
    /// Header'da karşılığı bulunan açıklama anahtarları → herhangi bir overload'daki
    /// parametre adları.
    seen: BTreeMap<String, BTreeSet<String>>,
    /// Dönüşü olan (constructor olmayan) öğelerin anahtarları.
    returns_seen: BTreeSet<String>,
    /// Açıklaması gerçekten uygulanan overload'ların imzaları, anahtara göre.
    applied_overloads: BTreeMap<String, Vec<String>>,
}

/// Bağlanmaya çalışılan bir fonksiyon, metot ya da constructor.
struct Candidate<'c> {
    /// `ferrobridge.toml` anahtarı (tam C++ adı).
    key: String,
    /// Atlanma mesajlarındaki ad.
    display: String,
    callee: Callee,
    params: &'c [Param],
    /// Constructor'lar için `None`.
    return_type: Option<&'c QualType>,
    receiver_mutable: Option<bool>,
    noexcept: bool,
    location: &'c SourceLocation,
}

impl Checker<'_> {
    fn free_function(&mut self, function: &Function) -> Option<FunctionPlan> {
        let name = qualified(&function.namespace, &function.name);
        self.bind(Candidate {
            key: name.clone(),
            display: name,
            callee: Callee::Free {
                namespace: function.namespace.clone(),
                cpp_name: function.name.clone(),
            },
            params: &function.params,
            return_type: Some(&function.return_type),
            receiver_mutable: None,
            noexcept: function.noexcept,
            location: &function.location,
        })
    }

    fn class_plan(&mut self, class: &Class) -> Option<ClassPlan> {
        let known_class = self
            .known
            .classes
            .get(&(class.namespace.clone(), class.name.clone()))?
            .clone();
        let type_ref = known_class.type_ref.clone();
        let class_name = qualified(&class.namespace, &class.name);
        let constructor_key = format!("{class_name}::{}", class.name);
        let mut constructors = Vec::new();
        let mut methods = Vec::new();

        for unsupported in &class.unsupported_members {
            self.plan.skipped.push(skipped(
                &unsupported.name,
                &unsupported.reason,
                &unsupported.location,
            ));
        }

        let factory_blocker = if class.is_abstract {
            Some(REASON_ABSTRACT.to_owned())
        } else {
            rules::deletable(&known_class, false).err()
        };
        if let Some(reason) = factory_blocker {
            for constructor in &class.constructors {
                self.mark_seen(&constructor_key, &constructor.params, false);
                self.plan.skipped.push(skipped(
                    &format!("{class_name} (constructor)"),
                    &reason,
                    &constructor.location,
                ));
            }
        } else {
            if class.constructors.is_empty() {
                self.plan
                    .notes
                    .push(skipped(&class_name, NOTE_NO_CONSTRUCTOR, &class.location));
            }
            for (index, constructor) in class.constructors.iter().enumerate() {
                if let Some(mut bound) = self.bind(Candidate {
                    key: constructor_key.clone(),
                    display: format!("{class_name} (constructor #{})", index + 1),
                    callee: Callee::Constructor {
                        class: type_ref.clone(),
                    },
                    params: &constructor.params,
                    return_type: None,
                    receiver_mutable: None,
                    noexcept: constructor.noexcept,
                    location: &constructor.location,
                }) {
                    // Constructor shim'i nesneyi `std::unique_ptr` içinde döndürür.
                    bound.returns = Returns::Value(BridgeType::UniquePtr(Box::new(
                        BridgeType::Class(type_ref.clone()),
                    )));
                    constructors.push(bound);
                }
            }
        }

        for method in &class.methods {
            let name = format!("{class_name}::{}", method.name);
            let (callee, receiver_mutable) = if method.is_static {
                (
                    Callee::Static {
                        class: type_ref.clone(),
                        cpp_name: method.name.clone(),
                    },
                    None,
                )
            } else {
                (
                    Callee::Method {
                        class: type_ref.clone(),
                        cpp_name: method.name.clone(),
                        receiver_mutable: !method.is_const,
                    },
                    Some(!method.is_const),
                )
            };
            if let Some(bound) = self.bind(Candidate {
                key: name.clone(),
                display: name,
                callee,
                params: &method.params,
                return_type: Some(&method.return_type),
                receiver_mutable,
                noexcept: method.noexcept,
                location: &method.location,
            }) {
                methods.push(bound);
            }
        }

        Some(ClassPlan {
            type_ref,
            cpp_name: class.name.clone(),
            namespace: class.namespace.clone(),
            constructors,
            methods,
        })
    }

    /// Bir fonksiyonu bağlamayı dener. Tek bir tip bile desteklenmiyorsa atlar ve ilk nedeni
    /// yazar. Bütün sorunlar açıklamayla çözülebiliyorsa her biri için bir öneri kaydeder.
    fn bind(&mut self, candidate: Candidate<'_>) -> Option<FunctionPlan> {
        let Candidate {
            key,
            display,
            callee,
            params,
            return_type,
            receiver_mutable,
            noexcept,
            location,
        } = candidate;
        self.mark_seen(&key, params, return_type.is_some());
        let annotation = self.annotations.functions.get(&key);
        let origin = |line: usize| self.origin(line);

        let mut first_failure: Option<String> = None;
        let mut suggestions = Vec::new();
        let mut all_annotatable = true;
        let mut assumptions = Vec::new();
        let mut warnings = Vec::new();
        let mut fail = |message: String, rejection_need: Option<Need>, target, cpp_type| {
            first_failure.get_or_insert(message);
            match rejection_need {
                Some(_) => suggestions.push((target, cpp_type)),
                None => all_annotatable = false,
            }
        };

        let mut bound_params = Vec::with_capacity(params.len());
        for (index, param) in params.iter().enumerate() {
            let rust_name = naming::param_name(param.name.as_deref(), index);
            let shown = param.name.clone().unwrap_or_else(|| rust_name.clone());
            let given = annotation.and_then(|a| a.params.get(&shown));
            match rules::param_type(&param.ty, &self.known, given.map(|l| l.value)) {
                Ok(rule) => {
                    if let Some(given) = given {
                        let subject = format!("parametre `{shown}`");
                        if rule.applied {
                            let promise =
                                if matches!(param.ty.ty, ferrobridge_ir::Type::Pointer { .. }) {
                                    PROMISE_BORROWED_POINTER
                                } else {
                                    PROMISE_BORROWED_REF
                                };
                            assumptions.push(Assumption {
                                name: key.clone(),
                                subject,
                                annotation: given.value.to_string(),
                                promise: promise.to_owned(),
                                origin: origin(given.line),
                            });
                        } else {
                            warnings.push(unneeded(&origin(given.line), &key, &subject, &param.ty));
                        }
                    }
                    bound_params.push(ParamPlan {
                        rust_name,
                        ty: rule.ty,
                        pass: rule.pass,
                    });
                }
                Err(rejection) => fail(
                    format!("parametre `{shown}`: {}", rejection.reason),
                    rejection.need,
                    SuggestionTarget::Param { name: shown },
                    rules::spell(&param.ty),
                ),
            }
        }

        let mut returns = Returns::Unit;
        let mut always_fallible = false;
        if let Some(return_type) = return_type {
            let given = annotation.and_then(|a| a.returns.as_ref());
            match rules::return_type(
                return_type,
                &self.known,
                receiver_mutable,
                given.map(|l| l.value),
            ) {
                Ok(rule) => {
                    if let Some(given) = given {
                        if rule.applied {
                            let promise = match (&rule.returns, given.value) {
                                (_, ReturnAnnotation::Owned) => PROMISE_OWNED,
                                (Returns::BorrowedPointer { .. }, _) => PROMISE_BORROWED_RETURN,
                                _ => PROMISE_BORROWED_REF_RETURN,
                            };
                            assumptions.push(Assumption {
                                name: key.clone(),
                                subject: "dönüş".to_owned(),
                                annotation: given.value.to_string(),
                                promise: promise.to_owned(),
                                origin: origin(given.line),
                            });
                        } else {
                            warnings.push(unneeded(
                                &origin(given.line),
                                &key,
                                "dönüş",
                                return_type,
                            ));
                        }
                    }
                    always_fallible = matches!(rule.returns, Returns::BorrowedPointer { .. });
                    returns = rule.returns;
                }
                Err(rejection) => {
                    let target = match &rejection.need {
                        Some(Need::Return(options)) => SuggestionTarget::Return {
                            options: options.clone(),
                        },
                        _ => SuggestionTarget::Return {
                            options: Vec::new(),
                        },
                    };
                    fail(
                        format!("dönüş tipi: {}", rejection.reason),
                        rejection.need,
                        target,
                        rules::spell(return_type),
                    );
                }
            }
        }

        self.plan.annotation_warnings.extend(warnings);
        if let Some(reason) = first_failure {
            self.plan.skipped.push(skipped(&display, &reason, location));
            if all_annotatable {
                let signature = signature(&key, params);
                self.plan
                    .suggestions
                    .extend(
                        suggestions
                            .into_iter()
                            .map(|(target, cpp_type)| Suggestion {
                                key: key.clone(),
                                signature: signature.clone(),
                                target,
                                cpp_type,
                                location: location.clone(),
                            }),
                    );
            }
            return None;
        }
        if !assumptions.is_empty() {
            self.applied_overloads
                .entry(key.clone())
                .or_default()
                .push(signature(&key, params));
        }
        self.plan.assumptions.extend(assumptions);
        Some(FunctionPlan {
            rust_name: String::new(),
            callee,
            params: bound_params,
            returns,
            fallible: !noexcept || always_fallible,
            location: location.clone(),
        })
    }

    /// Bu anahtarın header'da bir karşılığı olduğunu ve parametre adlarını kaydeder.
    fn mark_seen(&mut self, key: &str, params: &[Param], has_return: bool) {
        let names = self.seen.entry(key.to_owned()).or_default();
        for (index, param) in params.iter().enumerate() {
            names.insert(
                param
                    .name
                    .clone()
                    .unwrap_or_else(|| naming::param_name(None, index)),
            );
        }
        if has_return {
            self.returns_seen.insert(key.to_owned());
        }
    }

    /// Bir açıklama birden fazla overload'a uygulandıysa uyarır: hepsi için doğru olmayabilir
    /// (ör. biri pointer'ı saklıyor olabilir). Overload'ları ayrı açıklamak v0.2 hedefi.
    fn warn_shared_overloads(&mut self) {
        for (key, signatures) in &self.applied_overloads {
            if signatures.len() < 2 {
                continue;
            }
            let line = self.annotations.functions.get(key).map_or(1, |f| f.line);
            self.plan.annotation_warnings.push(AnnotationIssue {
                origin: self.origin(line),
                message: format!(
                    "`{key}` açıklaması {} overload'a birden uygulandı: {}. Sözün hepsi için \
                     doğru olduğundan emin ol; overload'ları ayrı açıklamak henüz mümkün değil.",
                    signatures.len(),
                    signatures.join("; ")
                ),
            });
        }
    }

    /// Header'da karşılığı olmayan açıklamalar.
    fn unmatched_annotations(&self) -> Vec<AnnotationIssue> {
        let mut errors = Vec::new();
        for (key, function) in &self.annotations.functions {
            let Some(params) = self.seen.get(key) else {
                errors.push(AnnotationIssue {
                    origin: self.origin(function.line),
                    message: format!(
                        "`{key}` header'da bulunamadı. Tam C++ adı yazılmalı: `ns::f`, \
                         `ns::Sınıf::metot`, constructor için `ns::Sınıf::Sınıf`."
                    ),
                });
                continue;
            };
            if let Some(returns) = &function.returns {
                if !self.returns_seen.contains(key) {
                    errors.push(AnnotationIssue {
                        origin: self.origin(returns.line),
                        message: format!("`{key}` bir constructor; dönüşü açıklanamaz."),
                    });
                }
            }
            for (param, annotation) in &function.params {
                if !params.contains(param) {
                    let known: Vec<String> = params.iter().map(|p| format!("`{p}`")).collect();
                    errors.push(AnnotationIssue {
                        origin: self.origin(annotation.line),
                        message: format!(
                            "`{key}`'in `{param}` adında parametresi yok (parametreler: {}).",
                            if known.is_empty() {
                                "yok".to_owned()
                            } else {
                                known.join(", ")
                            }
                        ),
                    });
                }
            }
        }
        errors
    }

    /// `ferrobridge.toml:3` biçiminde yer; yalnızca dosya adı (üretilen dosyalara yerel yol
    /// sızmasın).
    fn origin(&self, line: usize) -> String {
        let name = Path::new(&self.annotations.source_name)
            .file_name()
            .map_or_else(
                || self.annotations.source_name.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
        format!("{name}:{line}")
    }
}

fn unneeded(origin: &str, key: &str, subject: &str, ty: &QualType) -> AnnotationIssue {
    AnnotationIssue {
        origin: origin.to_owned(),
        message: format!(
            "`{key}` {subject} (`{}`) açıklama gerektirmiyor; açıklama yok sayıldı.",
            rules::spell(ty)
        ),
    }
}

/// Okunabilir imza: `geo::Widget::attach(geo::Widget* target)`.
fn signature(key: &str, params: &[Param]) -> String {
    let params: Vec<String> = params
        .iter()
        .map(|p| match &p.name {
            Some(name) => format!("{} {name}", rules::spell(&p.ty)),
            None => rules::spell(&p.ty),
        })
        .collect();
    format!("{key}({})", params.join(", "))
}

/// Bridge'de adı geçebilecek sınıf ve enum'ları toplar; enum planlarını da oluşturur.
fn known_types(module: &Module, plan: &mut BridgePlan) -> Known {
    /// Bridge'e girecek bir tip, kaynak sırasıyla.
    enum Entry<'m> {
        Class(&'m Class),
        Enum { decl: &'m Enum, repr: IntType },
    }

    let mut entries: Vec<((Vec<String>, String), Entry<'_>)> = Vec::new();
    for item in &module.items {
        match item {
            Item::Class(class) if in_anonymous_namespace(&class.namespace) => {
                plan.skipped.push(skipped(
                    &qualified(&class.namespace, &class.name),
                    REASON_ANONYMOUS_NAMESPACE,
                    &class.location,
                ));
            }
            Item::Class(class) => entries.push((
                (class.namespace.clone(), class.name.clone()),
                Entry::Class(class),
            )),
            Item::Enum(decl) => match enum_bindable(decl) {
                Ok(repr) => entries.push((
                    (decl.namespace.clone(), decl.name.clone()),
                    Entry::Enum { decl, repr },
                )),
                Err(reason) => plan.skipped.push(skipped(
                    &qualified(&decl.namespace, &decl.name),
                    &reason,
                    &decl.location,
                )),
            },
            Item::Function(_) | Item::Unsupported(_) => {}
        }
    }

    let keys: Vec<(Vec<String>, String)> = entries.iter().map(|(key, _)| key.clone()).collect();
    let rust_names = naming::type_names(&keys);
    let mut known = Known::default();
    for ((key, entry), rust_name) in entries.into_iter().zip(rust_names) {
        let type_ref = TypeRef {
            rust_name,
            cpp_path: cpp_path(&key.0, &key.1),
        };
        match entry {
            Entry::Class(class) => {
                known.classes.insert(
                    key,
                    KnownClass {
                        type_ref,
                        is_abstract: class.is_abstract,
                        is_polymorphic: class.is_polymorphic,
                        destructor: class.destructor,
                    },
                );
            }
            Entry::Enum { decl, repr } => {
                plan.enums.push(EnumPlan {
                    type_ref: type_ref.clone(),
                    cpp_name: decl.name.clone(),
                    namespace: decl.namespace.clone(),
                    repr,
                    variants: decl.variants.clone(),
                });
                known.enums.insert(key, type_ref);
            }
        }
    }
    known
}

fn enum_bindable(enumeration: &Enum) -> Result<IntType, String> {
    if in_anonymous_namespace(&enumeration.namespace) {
        return Err(REASON_ANONYMOUS_NAMESPACE.to_owned());
    }
    if let Some(variant) = enumeration
        .variants
        .iter()
        .find(|v| naming::is_rust_keyword(&v.name))
    {
        return Err(format!(
            "Enum değeri `{}` bir Rust anahtar kelimesi; bağlanamıyor.",
            variant.name
        ));
    }
    rules::enum_repr(&enumeration.underlying)
}

/// Rust adlarını atar (ADR-0005).
fn assign_names(plan: &mut BridgePlan) {
    let BridgePlan {
        functions, classes, ..
    } = plan;
    let mut free_scope: Vec<&mut FunctionPlan> = functions.iter_mut().collect();
    for class in classes.iter_mut() {
        let ClassPlan {
            constructors,
            methods,
            ..
        } = class;
        free_scope.extend(constructors.iter_mut());
        let (statics, instance): (Vec<&mut FunctionPlan>, Vec<&mut FunctionPlan>) = methods
            .iter_mut()
            .partition(|m| matches!(m.callee, Callee::Static { .. }));
        free_scope.extend(statics);
        let mut instance = instance;
        naming::name_methods(&mut instance);
    }
    naming::name_free_scope(&mut free_scope);
}

fn skipped(name: &str, reason: &str, location: &SourceLocation) -> Skipped {
    Skipped {
        name: name.to_owned(),
        reason: reason.to_owned(),
        location: location.clone(),
    }
}

fn qualified(namespace: &[String], name: &str) -> String {
    let mut path = namespace.to_vec();
    path.push(name.to_owned());
    path.join("::")
}

fn cpp_path(namespace: &[String], name: &str) -> String {
    format!("::{}", qualified(namespace, name))
}

fn in_anonymous_namespace(namespace: &[String]) -> bool {
    namespace.iter().any(|segment| segment.starts_with('('))
}

#[cfg(test)]
mod tests;
