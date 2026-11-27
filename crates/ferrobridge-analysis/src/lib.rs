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

use ferrobridge_ir::{
    Class, Constant, Enum, Function, Item, Module, Param, QualType, SourceLocation,
};

pub use config::{
    Annotations, ConfigError, FunctionAnnotation, Located, ParamAnnotation, ReturnAnnotation,
    TemplateEntry,
};
pub use plan::{
    AnnotationIssue, Assumption, BridgePlan, BridgeType, Callee, ClassPlan, ConstantPlan,
    ConstantValue, EnumPlan, FieldPlan, FunctionPlan, IntType, LengthParam, OperatorKind,
    ParamPlan, Pass, Returns, Skipped, Suggestion, SuggestionTarget, TypeRef, ValueLayout,
};
pub use suggest::render_suggestions;

use rules::{Known, KnownClass, Need};

const REASON_ABSTRACT: &str = "Sınıf soyut (saf sanal metodu var); doğrudan oluşturulamaz.";
const NOTE_NO_CONSTRUCTOR: &str = "Açıkça bildirilmiş public constructor yok; Rust'ta yalnızca \
     başka fonksiyonların döndürdüğü nesnelerle kullanılabilir.";
const REASON_ANONYMOUS_NAMESPACE: &str =
    "Anonim namespace'teki tipler header dışından kullanılamaz.";
const REASON_OPERATOR_KIND: &str = "Bu operatörün Rust karşılığı tanımlı değil.";
const REASON_FIELD: &str = "Alanlara doğrudan erişim v0.2 hedefi; erişim metotları kullanın.";
const REASON_ANONYMOUS_CONSTANT: &str =
    "Anonim namespace'teki sabitlerin Rust'tan yazılabilecek bir adı yok.";

const PROMISE_BORROWED_POINTER: &str = "pointer tek bir nesneyi gösteriyor (dizi değil), null \
     değil, yalnızca çağrı süresince kullanılıyor ve saklanmıyor";
const PROMISE_BORROWED_REF: &str = "referans yalnızca çağrı süresince kullanılıyor ve saklanmıyor";
const PROMISE_OWNED: &str = "pointer `new` ile ayrıldı ve sahipliği çağırana geçiyor; Rust'ta \
     `UniquePtr` düşünce `delete` edilir";
const PROMISE_BORROWED_RETURN: &str = "pointer nesnenin sahip olduğu bir şeyi gösteriyor ve \
     nesne yaşadıkça geçerli; silinmez (null gelirse `Err` döner)";
const PROMISE_BORROWED_REF_RETURN: &str =
    "referans nesnenin sahip olduğu bir şeyi gösteriyor ve nesne yaşadıkça geçerli";
const PROMISE_NO_EXCEPTIONS: &str = "kütüphanenin hiçbir fonksiyonu exception fırlatmıyor; \
     fonksiyonlar `Result` yerine doğrudan değer döndürür. Söz bozulursa program `std::terminate` \
     ile sonlanır (tanımsız davranış değil). ferrobridge'in kendi kontrolleri (slice uzunluğu, \
     null dönüş) `Result` olarak kalır";
const REASON_INVALID_SLICE: &str =
    "`ferrobridge.toml`'daki `slice` açıklaması bu imzaya uymuyor (hatalara bakın).";

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
        slice_errors: Vec::new(),
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
            Item::Constant(constant) => match constant_plan(constant) {
                Ok(bound) => checker.plan.constants.push(bound),
                Err(reason) => checker.plan.skipped.push(skipped(
                    &qualified(&constant.namespace, &constant.name),
                    &reason,
                    &constant.location,
                )),
            },
            Item::Unsupported(unsupported) => checker.plan.skipped.push(Skipped {
                name: unsupported.name.clone(),
                reason: unsupported.reason.clone(),
                location: unsupported.location.clone(),
            }),
        }
    }

    if let Some(line) = annotations.no_exceptions {
        checker.plan.assumptions.push(Assumption {
            name: "(kütüphane)".to_owned(),
            subject: "exception'lar".to_owned(),
            annotation: "exceptions = false".to_owned(),
            promise: PROMISE_NO_EXCEPTIONS.to_owned(),
            origin: checker.origin(line),
        });
    }
    let mut errors = checker.unmatched_annotations();
    errors.append(&mut checker.slice_errors);
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
    /// İmzaya uymayan `slice` açıklamaları.
    slice_errors: Vec<AnnotationIssue>,
}

/// Bir `slice` açıklamasının bir overload'a uygulanışı.
struct SliceBinding {
    /// Uzunluk parametresinin C++ parametre listesindeki yeri.
    len_index: usize,
    len_name: String,
    /// Uzunluğun C++ temel tipi.
    len_type: String,
    element: BridgeType,
    mutable: bool,
    /// Açıklamanın satırı.
    line: usize,
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

        // Opak sınıfların public alanlarına Rust'tan erişilemiyor. Atlanan üyeler kaynaktaki
        // sırayla listelenir.
        let value = if known_class.value {
            value_layout(class, &self.known)
        } else {
            None
        };
        let mut members: Vec<Skipped> = class
            .fields
            .iter()
            .filter(|f| f.public && value.is_none())
            .map(|field| {
                skipped(
                    &format!("{class_name}::{}", field.name),
                    REASON_FIELD,
                    &field.location,
                )
            })
            .chain(
                class
                    .unsupported_members
                    .iter()
                    .map(|u| skipped(&u.name, &u.reason, &u.location)),
            )
            .collect();
        members.sort_by_key(|m| (m.location.line, m.location.column));
        self.plan.skipped.extend(members);

        let factory_blocker = if value.is_some() {
            None
        } else if class.is_abstract {
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
            // Değer tipi alanlarından kurulabilir; not gereksiz.
            if class.constructors.is_empty() && value.is_none() {
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
                    // Constructor shim'i nesneyi `std::unique_ptr` içinde döndürür; değer tipini
                    // değerle.
                    bound.returns = if value.is_some() {
                        Returns::Value(BridgeType::Value(type_ref.clone()))
                    } else {
                        Returns::Value(BridgeType::UniquePtr(Box::new(BridgeType::Class(
                            type_ref.clone(),
                        ))))
                    };
                    constructors.push(bound);
                }
            }
        }

        for method in &class.methods {
            let name = format!("{class_name}::{}", method.name);
            let operator = match &method.operator {
                Some(op) => match OperatorKind::from_cpp(op, method.params.len()) {
                    Some(kind) if !method.is_static => Some(kind),
                    _ => {
                        self.plan.skipped.push(skipped(
                            &name,
                            REASON_OPERATOR_KIND,
                            &method.location,
                        ));
                        continue;
                    }
                },
                None => None,
            };
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
                methods.push(FunctionPlan { operator, ..bound });
            }
        }

        Some(ClassPlan {
            type_ref,
            cpp_name: class.name.clone(),
            namespace: class.namespace.clone(),
            constructors,
            methods,
            value,
            instance_spelling: class.instance_of.as_ref().map(|i| i.spelling.clone()),
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
        // `fix`: sorun bir açıklamayla çözülebilir mi? `Some(Some(_))` ise bu yer için öneri
        // yazılır; `Some(None)` ise başka bir yerin açıklaması çözer (ör. slice uzunluğu).
        let mut fail = |message: String, fix: Option<Option<Need>>, target, cpp_type| {
            first_failure.get_or_insert(message);
            match fix {
                Some(Some(_)) => suggestions.push((target, cpp_type)),
                Some(None) => {}
                None => all_annotatable = false,
            }
        };

        let shown_names: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(index, p)| {
                p.name
                    .clone()
                    .unwrap_or_else(|| naming::param_name(None, index))
            })
            .collect();
        let slices = match annotation {
            Some(annotation) => {
                let (slices, errors) = self.slice_bindings(&key, params, &shown_names, annotation);
                if !errors.is_empty() {
                    self.slice_errors.extend(errors);
                    self.plan
                        .skipped
                        .push(skipped(&display, REASON_INVALID_SLICE, location));
                    return None;
                }
                slices
            }
            None => BTreeMap::new(),
        };
        let length_positions: BTreeSet<usize> = slices.values().map(|s| s.len_index).collect();
        let has_slice_candidate = params
            .iter()
            .any(|p| rules::could_be_slice(&p.ty, &self.known));
        let slice_length_candidates: BTreeSet<usize> = if has_slice_candidate {
            (0..params.len())
                .filter(|&i| rules::slice_length(&params[i].ty).is_ok())
                .collect()
        } else {
            BTreeSet::new()
        };

        let mut bound_params = Vec::with_capacity(params.len());
        let mut lengths = Vec::new();
        for (index, param) in params.iter().enumerate() {
            if length_positions.contains(&index) {
                continue; // Slice'ın boyundan doldurulur (aşağıda `lengths`).
            }
            let rust_name = naming::param_name(param.name.as_deref(), index);
            let shown = shown_names[index].clone();
            if let Some(binding) = slices.get(&index) {
                let given = format!("{{ slice = \"{}\" }}", binding.len_name);
                assumptions.push(Assumption {
                    name: key.clone(),
                    subject: format!("parametre `{shown}`"),
                    annotation: given,
                    promise: format!(
                        "pointer `{}` elemanlık bir diziyi gösteriyor; dizi yalnızca çağrı \
                         süresince kullanılıyor ve saklanmıyor; uzunluk 0 iken pointer okunmuyor \
                         (Rust boş slice için null olmayan bir pointer verir)",
                        binding.len_name
                    ),
                    origin: origin(binding.line),
                });
                lengths.push(LengthParam {
                    position: binding.len_index,
                    slice: bound_params.len(),
                    cpp_type: binding.len_type.clone(),
                });
                bound_params.push(ParamPlan {
                    rust_name,
                    ty: BridgeType::Slice {
                        mutable: binding.mutable,
                        element: Box::new(binding.element.clone()),
                    },
                    pass: Pass::Slice,
                });
                continue;
            }
            let given = annotation.and_then(|a| a.params.get(&shown));
            match rules::param_type(&param.ty, &self.known, given.map(|l| &l.value)) {
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
                Err(rejection) => {
                    let slice_lengths = if rules::could_be_slice(&param.ty, &self.known) {
                        params
                            .iter()
                            .zip(&shown_names)
                            .filter(|(p, _)| rules::slice_length(&p.ty).is_ok())
                            .map(|(_, name)| name.clone())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    // Değerle alınan bir tamsayı (ör. `size_t`), aynı fonksiyondaki bir sayı
                    // pointer'ının `slice` uzunluğu yapılırsa Rust imzasından çıkar.
                    let fix = match rejection.need {
                        Some(need) => Some(Some(need)),
                        None if slice_length_candidates.contains(&index) => Some(None),
                        None => None,
                    };
                    fail(
                        format!("parametre `{shown}`: {}", rejection.reason),
                        fix,
                        SuggestionTarget::Param {
                            name: shown,
                            slice_lengths,
                        },
                        rules::spell(&param.ty),
                    );
                }
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
                        rejection.need.map(Some),
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
        lengths.sort_by_key(|l| l.position);
        Some(FunctionPlan {
            rust_name: String::new(),
            callee,
            params: bound_params,
            returns,
            // Uzunluk kontrolü (slice C++ uzunluk tipine sığmıyorsa) exception fırlatır.
            // `[library] exceptions = false`: kütüphane fırlatmaz, söz bozulursa `terminate`.
            fallible: !(noexcept || self.annotations.no_exceptions.is_some())
                || always_fallible
                || !lengths.is_empty(),
            operator: None,
            lengths,
            location: location.clone(),
        })
    }

    /// Bu overload'a uygulanan `slice` açıklamaları: slice parametresinin yeri → uygulanış.
    /// Açıklama imzaya uymuyorsa hatalar döner. Açıklanan parametre bu overload'da yoksa
    /// açıklama bu overload'ı ilgilendirmez (hiç yoksa `unmatched_annotations` yakalar).
    fn slice_bindings(
        &self,
        key: &str,
        params: &[Param],
        shown_names: &[String],
        annotation: &FunctionAnnotation,
    ) -> (BTreeMap<usize, SliceBinding>, Vec<AnnotationIssue>) {
        let mut bindings = BTreeMap::new();
        let mut errors = Vec::new();
        let mut used_lengths: BTreeMap<usize, String> = BTreeMap::new();
        let position = |name: &str| shown_names.iter().position(|n| n == name);
        for (name, located) in &annotation.params {
            let ParamAnnotation::Slice { len } = &located.value else {
                continue;
            };
            let Some(index) = position(name) else {
                continue;
            };
            let mut error = |message: String| {
                errors.push(AnnotationIssue {
                    origin: self.origin(located.line),
                    message: format!("`{key}` parametre `{name}`: {message}"),
                });
            };
            if len == name {
                error("`slice` uzunluğu parametrenin kendisi olamaz.".to_owned());
                continue;
            }
            let Some(len_index) = position(len) else {
                let known: Vec<String> = shown_names.iter().map(|n| format!("`{n}`")).collect();
                error(format!(
                    "`{key}`'in `{len}` adında parametresi yok (`slice` uzunluğu; parametreler: \
                     {}).",
                    known.join(", ")
                ));
                continue;
            };
            let element = rules::slice_element(&params[index].ty, &self.known);
            let len_type = rules::slice_length(&params[len_index].ty);
            let (element, mutable) = match element {
                Ok(found) => found,
                Err(reason) => {
                    error(reason);
                    continue;
                }
            };
            let len_type = match len_type {
                Ok(found) => found,
                Err(reason) => {
                    error(format!("`{len}`: {reason}"));
                    continue;
                }
            };
            if annotation.params.contains_key(len) {
                error(format!(
                    "`{len}` slice'ın uzunluk parametresi; ona ayrıca açıklama verilemez."
                ));
                continue;
            }
            if let Some(other) = used_lengths.insert(len_index, name.clone()) {
                error(format!(
                    "`{other}` ve `{name}` iki ayrı slice aynı uzunluk parametresini (`{len}`) \
                     paylaşamaz."
                ));
                continue;
            }
            bindings.insert(
                index,
                SliceBinding {
                    len_index,
                    len_name: len.clone(),
                    len_type,
                    element,
                    mutable,
                    line: located.line,
                },
            );
        }
        (bindings, errors)
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
            Item::Function(_) | Item::Constant(_) | Item::Unsupported(_) => {}
        }
    }

    let keys: Vec<(Vec<String>, String)> = entries.iter().map(|(key, _)| key.clone()).collect();
    let rust_names = naming::type_names(&keys);
    let mut known = Known::default();
    for ((key, entry), rust_name) in entries.into_iter().zip(rust_names) {
        // Template örneklemesinin C++ adı yazımıdır (`::geo::Vec3<float>`), Rust adı değil.
        let cpp = match &entry {
            Entry::Class(Class {
                instance_of: Some(instance),
                ..
            }) => format!("::{}", instance.spelling),
            _ => cpp_path(&key.0, &key.1),
        };
        let type_ref = TypeRef {
            rust_name,
            cpp_path: cpp,
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
                        value: false,
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
    mark_value_types(module, &mut known);
    known
}

/// Değer tipi olabilecek sınıfları işaretler (ADR-0010). Bir sınıf değer tipidir, eğer:
/// polimorfik değilse, taban sınıfı yoksa, kullanıcı kopyalama/taşıma/destructor bildirmemişse,
/// düzeni biliniyorsa ve bütün alanları public olup sayı, `bool`, `c_char` ya da başka bir
/// değer tipiyse (`const` alan olmadan). Alan tipleri birbirine bağlı olduğundan (`Matrix3f` içinde `Vector3f`) küme
/// değişmeyene kadar elenir. Son sözü C++ derleyicisi söyler: shim düzeni `static_assert` ile
/// doğrular.
fn mark_value_types(module: &Module, known: &mut Known) {
    let classes: Vec<&Class> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Class(class) => Some(class),
            _ => None,
        })
        .filter(|class| {
            known
                .classes
                .contains_key(&(class.namespace.clone(), class.name.clone()))
        })
        .collect();
    let mut values: BTreeSet<(Vec<String>, String)> = classes
        .iter()
        .filter(|class| value_shape(class))
        .map(|class| (class.namespace.clone(), class.name.clone()))
        .collect();
    loop {
        let rejected: Vec<(Vec<String>, String)> = classes
            .iter()
            .map(|class| (class.namespace.clone(), class.name.clone()))
            .filter(|key| values.contains(key))
            .filter(|key| {
                let class = classes
                    .iter()
                    .find(|c| c.namespace == key.0 && c.name == key.1);
                class.is_some_and(|class| {
                    class.fields.iter().any(|field| {
                        rules::value_field_type(&field.ty.ty, known, &|k| values.contains(k))
                            .is_none()
                    })
                })
            })
            .collect();
        if rejected.is_empty() {
            break;
        }
        for key in rejected {
            values.remove(&key);
        }
    }
    for key in values {
        if let Some(class) = known.classes.get_mut(&key) {
            class.value = true;
        }
    }
}

/// Alan tiplerinden bağımsız değer tipi koşulları.
fn value_shape(class: &Class) -> bool {
    !class.is_polymorphic
        && !class.is_abstract
        && !class.user_copy_or_move
        && class.destructor == ferrobridge_ir::Destructor::Public { is_virtual: false }
        && class.layout.is_some()
        && !class.fields.is_empty()
        && class
            .fields
            .iter()
            // `const` alan: Rust'tan yazılırsa C++'ta `const` nesneyi değiştirmek olurdu.
            .all(|f| f.public && !f.ty.is_const && f.offset_bits.is_some_and(|bits| bits % 8 == 0))
        && !class
            .unsupported_members
            .iter()
            .any(|m| m.construct == "base_class")
}

/// Değer tipinin Rust düzeni.
fn value_layout(class: &Class, known: &Known) -> Option<ValueLayout> {
    let layout = class.layout?;
    let mut fields = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        let ty = rules::value_field_type(&field.ty.ty, known, &|key| {
            known.classes.get(key).is_some_and(|c| c.value)
        })?;
        fields.push(FieldPlan {
            rust_name: naming::escape(&field.name),
            cpp_name: field.name.clone(),
            ty,
            offset_bytes: field.offset_bits? / 8,
        });
    }
    Some(ValueLayout {
        size_bytes: layout.size_bytes,
        align_bytes: layout.align_bytes,
        fields,
    })
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
    name_constants(plan);
}

fn constant_plan(constant: &Constant) -> Result<ConstantPlan, String> {
    if in_anonymous_namespace(&constant.namespace) {
        return Err(REASON_ANONYMOUS_CONSTANT.to_owned());
    }
    Ok(ConstantPlan {
        rust_name: String::new(),
        cpp_name: constant.name.clone(),
        namespace: constant.namespace.clone(),
        ty: rules::constant_type(&constant.ty, constant.value)?,
        value: constant.value,
        location: constant.location.clone(),
    })
}

/// Sabit adları: aynı ad birden fazla namespace'te varsa namespace ön eki (`units_FEET`);
/// anahtar kelimeler `_` ile kaçılır. Bir güvenli fonksiyonla (Rust'ta aynı ad alanında)
/// çakışan sabit bağlanmaz.
fn name_constants(plan: &mut BridgePlan) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for constant in &plan.constants {
        *counts.entry(constant.cpp_name.clone()).or_default() += 1;
    }
    let functions: BTreeSet<String> = plan.functions.iter().map(|f| f.rust_name.clone()).collect();
    let mut used = BTreeSet::new();
    let mut kept = Vec::with_capacity(plan.constants.len());
    for mut constant in std::mem::take(&mut plan.constants) {
        let base = if counts[&constant.cpp_name] > 1 && !constant.namespace.is_empty() {
            format!("{}_{}", constant.namespace.join("_"), constant.cpp_name)
        } else {
            constant.cpp_name.clone()
        };
        let mut name = naming::escape(&base);
        let mut counter = 2;
        while used.contains(&name) {
            name = format!("{}_{counter}", naming::escape(&base));
            counter += 1;
        }
        if functions.contains(&name) {
            plan.skipped.push(skipped(
                &qualified(&constant.namespace, &constant.cpp_name),
                &format!(
                    "Rust'ta sabitler ve fonksiyonlar aynı ad alanında; `{name}` adlı fonksiyonla \
                     çakışıyor."
                ),
                &constant.location,
            ));
            continue;
        }
        used.insert(name.clone());
        constant.rust_name = name;
        kept.push(constant);
    }
    plan.constants = kept;
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
