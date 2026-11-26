//! IR üzerinde bağlanabilirlik analizi.
//!
//! [`check`] her öğe, parametre ve dönüş tipi için cxx'te güvenle ifade edilip
//! edilemeyeceğine karar verir ve kod üretiminin tükettiği bir [`BridgePlan`] üretir.
//! Kurallar Obsidian `03 Mimari` notundaki tip tablosu ve güvenlik kurallarıdır. Bir
//! fonksiyonun tek bir parametresi bile desteklenmiyorsa fonksiyon **tümüyle** atlanır ve
//! nedeni bildirilir; yarım bağlama yapılmaz.

mod naming;
mod plan;
mod rules;

use ferrobridge_ir::{Class, Enum, Function, Item, Module, Param, QualType, SourceLocation};

pub use plan::{
    BridgePlan, BridgeType, Callee, ClassPlan, EnumPlan, FunctionPlan, IntType, ParamPlan, Returns,
    Skipped, TypeRef,
};

use rules::Known;

const REASON_ABSTRACT: &str = "Sınıf soyut (saf sanal metodu var); doğrudan oluşturulamaz.";
const NOTE_NO_CONSTRUCTOR: &str = "Açıkça bildirilmiş public constructor yok; Rust'ta yalnızca \
     başka fonksiyonların döndürdüğü nesnelerle kullanılabilir.";
const REASON_ANONYMOUS_NAMESPACE: &str =
    "Anonim namespace'teki tipler header dışından kullanılamaz.";

/// IR modülünün bağlama planını çıkarır.
pub fn check(module: &Module) -> BridgePlan {
    let mut plan = BridgePlan::default();
    let known = known_types(module, &mut plan);

    for item in &module.items {
        match item {
            Item::Function(function) => {
                if let Some(bound) = free_function(function, &known, &mut plan.skipped) {
                    plan.functions.push(bound);
                }
            }
            Item::Class(class) => {
                if let Some(bound) = class_plan(class, &known, &mut plan) {
                    plan.classes.push(bound);
                }
            }
            Item::Enum(_) => {} // `known_types` içinde işlendi.
            Item::Unsupported(unsupported) => plan.skipped.push(Skipped {
                name: unsupported.name.clone(),
                reason: unsupported.reason.clone(),
                location: unsupported.location.clone(),
            }),
        }
    }

    assign_names(&mut plan);
    plan
}

/// Bridge'de adı geçebilecek sınıf ve enum'ları toplar; enum planlarını da oluşturur.
fn known_types(module: &Module, plan: &mut BridgePlan) -> Known {
    /// Bridge'e girecek bir tip, kaynak sırasıyla.
    enum Entry<'m> {
        Class { is_abstract: bool },
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
                Entry::Class {
                    is_abstract: class.is_abstract,
                },
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
            Entry::Class { is_abstract } => {
                known.classes.insert(key, (type_ref, is_abstract));
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

fn free_function(
    function: &Function,
    known: &Known,
    skipped_out: &mut Vec<Skipped>,
) -> Option<FunctionPlan> {
    let name = qualified(&function.namespace, &function.name);
    let callee = Callee::Free {
        namespace: function.namespace.clone(),
        cpp_name: function.name.clone(),
    };
    bind(
        &name,
        callee,
        &function.params,
        &function.return_type,
        None,
        function.noexcept,
        &function.location,
        known,
        skipped_out,
    )
}

fn class_plan(class: &Class, known: &Known, plan: &mut BridgePlan) -> Option<ClassPlan> {
    let (type_ref, _) = known
        .classes
        .get(&(class.namespace.clone(), class.name.clone()))?
        .clone();
    let class_name = qualified(&class.namespace, &class.name);
    let mut constructors = Vec::new();
    let mut methods = Vec::new();

    for unsupported in &class.unsupported_members {
        plan.skipped.push(skipped(
            &unsupported.name,
            &unsupported.reason,
            &unsupported.location,
        ));
    }

    if class.is_abstract {
        for constructor in &class.constructors {
            plan.skipped.push(skipped(
                &format!("{class_name} (constructor)"),
                REASON_ABSTRACT,
                &constructor.location,
            ));
        }
    } else {
        if class.constructors.is_empty() {
            plan.notes
                .push(skipped(&class_name, NOTE_NO_CONSTRUCTOR, &class.location));
        }
        for (index, constructor) in class.constructors.iter().enumerate() {
            let callee = Callee::Constructor {
                class: type_ref.clone(),
            };
            let returns = QualType {
                is_const: false,
                ty: ferrobridge_ir::Type::Void,
            };
            if let Some(mut bound) = bind(
                &format!("{class_name} (constructor #{})", index + 1),
                callee,
                &constructor.params,
                &returns,
                None,
                constructor.noexcept,
                &constructor.location,
                known,
                &mut plan.skipped,
            ) {
                // Constructor shim'i nesneyi `std::unique_ptr` içinde döndürür.
                bound.returns = Returns::Value(BridgeType::UniquePtr(Box::new(BridgeType::Class(
                    type_ref.clone(),
                ))));
                constructors.push(bound);
            }
        }
    }

    for method in &class.methods {
        let name = format!("{class_name}::{}", method.name);
        let (callee, receiver) = if method.is_static {
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
        if let Some(bound) = bind(
            &name,
            callee,
            &method.params,
            &method.return_type,
            receiver,
            method.noexcept,
            &method.location,
            known,
            &mut plan.skipped,
        ) {
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

/// Bir fonksiyonu bağlamayı dener; tek bir tip bile desteklenmiyorsa atlar ve nedenini yazar.
#[allow(clippy::too_many_arguments)]
fn bind(
    name: &str,
    callee: Callee,
    params: &[Param],
    return_type: &QualType,
    receiver_mutable: Option<bool>,
    noexcept: bool,
    location: &SourceLocation,
    known: &Known,
    skipped_out: &mut Vec<Skipped>,
) -> Option<FunctionPlan> {
    let mut bound_params = Vec::with_capacity(params.len());
    for (index, param) in params.iter().enumerate() {
        let rust_name = naming::param_name(param.name.as_deref(), index);
        match rules::param_type(&param.ty, known) {
            Ok(ty) => bound_params.push(ParamPlan { rust_name, ty }),
            Err(reason) => {
                let shown = param.name.clone().unwrap_or(rust_name);
                skipped_out.push(skipped(
                    name,
                    &format!("parametre `{shown}`: {reason}"),
                    location,
                ));
                return None;
            }
        }
    }
    let returns = match rules::return_type(return_type, known, receiver_mutable) {
        Ok(returns) => returns,
        Err(reason) => {
            skipped_out.push(skipped(name, &format!("dönüş tipi: {reason}"), location));
            return None;
        }
    };
    Some(FunctionPlan {
        rust_name: String::new(),
        callee,
        params: bound_params,
        returns,
        fallible: !noexcept,
        location: location.clone(),
    })
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
