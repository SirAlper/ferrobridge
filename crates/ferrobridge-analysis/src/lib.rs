//! Bindability analysis over the IR.
//!
//! [`check`] decides, for every item, parameter and return type, whether it can be expressed
//! safely in cxx, and produces a [`BridgePlan`] consumed by code generation.
//! The rules are the architecture's type table and the safety rules. If even a single
//! parameter of a function is unsupported, the function is skipped **entirely** and the
//! reason is reported; there are no half bindings.
//!
//! Ownership information the header does not give comes from [`Annotations`] (`ferrobridge.toml`).
//! Every annotation used shows up in the plan as an [`Assumption`]; every place that could not
//! be bound because an annotation is missing becomes a [`Suggestion`].

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

const REASON_ABSTRACT: &str =
    "The class is abstract (it has a pure virtual method); it cannot be created directly.";
const NOTE_NO_CONSTRUCTOR: &str = "No explicitly declared public constructor; in Rust it can only be used \
     with objects returned by other functions.";
const REASON_ANONYMOUS_NAMESPACE: &str =
    "Types in an anonymous namespace cannot be used from outside the header.";
const REASON_OPERATOR_KIND: &str = "This operator has no defined Rust counterpart.";
const REASON_FIELD: &str = "Direct access to fields is a v0.2 goal; use accessor methods.";
const REASON_ANONYMOUS_CONSTANT: &str =
    "Constants in an anonymous namespace have no name that can be written from Rust.";

const PROMISE_BORROWED_POINTER: &str = "the pointer points to a single object (not an array), is not \
     null, and is used only during the call and not kept";
const PROMISE_BORROWED_REF: &str = "the reference is used only during the call and not kept";
const PROMISE_OWNED: &str = "the pointer was allocated with `new` and its ownership passes to the caller; in Rust \
     it is `delete`d when the `UniquePtr` is dropped";
const PROMISE_BORROWED_RETURN: &str = "the pointer points to something the object owns and is \
     valid while the object lives; it is not deleted (a null returns `Err`)";
const PROMISE_BORROWED_REF_RETURN: &str =
    "the reference points to something the object owns and is valid while the object lives";
const PROMISE_NO_EXCEPTIONS: &str = "no function of the library throws exceptions; \
     functions return values directly instead of `Result`. If the promise is broken, the program ends with \
     `std::terminate` (not undefined behavior). ferrobridge's own checks (slice length, \
     null return) stay `Result`";
const REASON_INVALID_SLICE: &str =
    "The `slice` annotation in `ferrobridge.toml` does not match this signature (see the errors).";

/// Works out the binding plan of an IR module.
///
/// If `annotations` do not match any item in the header (a typo, a nonexistent parameter...),
/// no plan is produced; all mismatches are returned together.
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
            Item::Enum(_) => {} // Handled in `known_types`.
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
            name: "(library)".to_owned(),
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

/// The state carried through [`check`].
struct Checker<'a> {
    known: Known,
    annotations: &'a Annotations,
    plan: BridgePlan,
    /// Annotation keys with a counterpart in the header → the parameter names in any
    /// overload.
    seen: BTreeMap<String, BTreeSet<String>>,
    /// Keys of items that have a return (not constructors).
    returns_seen: BTreeSet<String>,
    /// Signatures of the overloads an annotation was really applied to, by key.
    applied_overloads: BTreeMap<String, Vec<String>>,
    /// `slice` annotations that do not match the signature.
    slice_errors: Vec<AnnotationIssue>,
}

/// How a `slice` annotation applies to an overload.
struct SliceBinding {
    /// The position of the length parameter in the C++ parameter list.
    len_index: usize,
    len_name: String,
    /// The underlying C++ type of the length.
    len_type: String,
    element: BridgeType,
    mutable: bool,
    /// The annotation's line.
    line: usize,
}

/// A function, method or constructor being bound.
struct Candidate<'c> {
    /// The `ferrobridge.toml` key (the full C++ name).
    key: String,
    /// The name in skip messages.
    display: String,
    callee: Callee,
    params: &'c [Param],
    /// `None` for constructors.
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

        // The public fields of opaque classes cannot be reached from Rust. Skipped members are listed
        // in source order.
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
            // A value type can be constructed from its fields; the note is unnecessary.
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
                    // The constructor shim returns the object inside a `std::unique_ptr`; a value type
                    // by value.
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

    /// Tries to bind a function. If even one type is unsupported, it skips it and writes the first
    /// reason. If all problems can be solved with annotations, it records a suggestion for each.
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
        // `fix`: can the problem be solved with an annotation? With `Some(Some(_))` a suggestion is
        // written for this place; with `Some(None)` another place's annotation solves it (e.g. a slice length).
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
                continue; // Filled from the slice's length (`lengths` below).
            }
            let rust_name = naming::param_name(param.name.as_deref(), index);
            let shown = shown_names[index].clone();
            if let Some(binding) = slices.get(&index) {
                let given = format!("{{ slice = \"{}\" }}", binding.len_name);
                assumptions.push(Assumption {
                    name: key.clone(),
                    subject: format!("parameter `{shown}`"),
                    annotation: given,
                    promise: format!(
                        "the pointer points to an array of `{}` elements; the array is used only during the call \
                         and not kept; when the length is 0 the pointer is not read \
                         (Rust gives a non-null pointer for an empty slice)",
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
                        let subject = format!("parameter `{shown}`");
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
                    // An integer taken by value (e.g. `size_t`) leaves the Rust signature if it is made the
                    // `slice` length of a numeric pointer in the same function.
                    let fix = match rejection.need {
                        Some(need) => Some(Some(need)),
                        None if slice_length_candidates.contains(&index) => Some(None),
                        None => None,
                    };
                    fail(
                        format!("parameter `{shown}`: {}", rejection.reason),
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
                                subject: "return".to_owned(),
                                annotation: given.value.to_string(),
                                promise: promise.to_owned(),
                                origin: origin(given.line),
                            });
                        } else {
                            warnings.push(unneeded(
                                &origin(given.line),
                                &key,
                                "return",
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
                        format!("return type: {}", rejection.reason),
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
            // The length check (when the slice does not fit the C++ length type) throws an exception.
            // `[library] exceptions = false`: the library does not throw; if the promise is broken, `terminate`.
            fallible: !(noexcept || self.annotations.no_exceptions.is_some())
                || always_fallible
                || !lengths.is_empty(),
            operator: None,
            lengths,
            location: location.clone(),
        })
    }

    /// The `slice` annotations applied to this overload: position of the slice parameter → application.
    /// Returns errors if an annotation does not match the signature. If the annotated parameter is not
    /// in this overload, the annotation does not concern it (`unmatched_annotations` catches it if it is nowhere).
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
                    message: format!("`{key}` parameter `{name}`: {message}"),
                });
            };
            if len == name {
                error("the `slice` length cannot be the parameter itself.".to_owned());
                continue;
            }
            let Some(len_index) = position(len) else {
                let known: Vec<String> = shown_names.iter().map(|n| format!("`{n}`")).collect();
                error(format!(
                    "`{key}` has no parameter named `{len}` (the `slice` length; parameters: \
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
                    "`{len}` is the slice's length parameter; it cannot be annotated separately."
                ));
                continue;
            }
            if let Some(other) = used_lengths.insert(len_index, name.clone()) {
                error(format!(
                    "two separate slices `{other}` and `{name}` cannot share the same length \
                     parameter (`{len}`)."
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

    /// Records that this key has a counterpart in the header, and its parameter names.
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

    /// Warns if an annotation was applied to more than one overload: it may not be right for all of
    /// them (e.g. one of them may keep the pointer). Annotating overloads separately is a v0.2 goal.
    fn warn_shared_overloads(&mut self) {
        for (key, signatures) in &self.applied_overloads {
            if signatures.len() < 2 {
                continue;
            }
            let line = self.annotations.functions.get(key).map_or(1, |f| f.line);
            self.plan.annotation_warnings.push(AnnotationIssue {
                origin: self.origin(line),
                message: format!(
                    "the `{key}` annotation was applied to {} overloads at once: {}. Make sure the promise is \
                     right for all of them; annotating overloads separately is not possible yet.",
                    signatures.len(),
                    signatures.join("; ")
                ),
            });
        }
    }

    /// Annotations with no counterpart in the header.
    fn unmatched_annotations(&self) -> Vec<AnnotationIssue> {
        let mut errors = Vec::new();
        for (key, function) in &self.annotations.functions {
            let Some(params) = self.seen.get(key) else {
                errors.push(AnnotationIssue {
                    origin: self.origin(function.line),
                    message: format!(
                        "`{key}` was not found in the header. Write the full C++ name: `ns::f`, \
                         `ns::Class::method`, `ns::Class::Class` for a constructor."
                    ),
                });
                continue;
            };
            if let Some(returns) = &function.returns {
                if !self.returns_seen.contains(key) {
                    errors.push(AnnotationIssue {
                        origin: self.origin(returns.line),
                        message: format!(
                            "`{key}` is a constructor; its return cannot be annotated."
                        ),
                    });
                }
            }
            for (param, annotation) in &function.params {
                if !params.contains(param) {
                    let known: Vec<String> = params.iter().map(|p| format!("`{p}`")).collect();
                    errors.push(AnnotationIssue {
                        origin: self.origin(annotation.line),
                        message: format!(
                            "`{key}` has no parameter named `{param}` (parameters: {}).",
                            if known.is_empty() {
                                "none".to_owned()
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

    /// The location in the form `ferrobridge.toml:3`; the file name only (so no local path leaks
    /// into generated files).
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
            "`{key}` {subject} (`{}`) does not need an annotation; the annotation was ignored.",
            rules::spell(ty)
        ),
    }
}

/// A readable signature: `geo::Widget::attach(geo::Widget* target)`.
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

/// Collects the classes and enums whose names may appear in the bridge; also builds the enum plans.
fn known_types(module: &Module, plan: &mut BridgePlan) -> Known {
    /// A type entering the bridge, in source order.
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
        // The C++ name of a template instantiation is its spelling (`::geo::Vec3<float>`), not the Rust name.
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

/// Marks the classes that can be value types (ADR-0010). A class is a value type if it is not
/// polymorphic, has no base class, the user declares no copy/move/destructor, its layout is known,
/// and all its fields are public and are numbers, `bool`, `c_char` or another value type (with no
/// `const` field). Because field types depend on each other (`Vector3f` inside `Matrix3f`), the set
/// is reduced until it stops changing. The C++ compiler has the final word: the shim checks the
/// layout with `static_assert`.
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

/// Value type conditions that do not depend on field types.
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
            // A `const` field: writing it from Rust would change a `const` object in C++.
            .all(|f| f.public && !f.ty.is_const && f.offset_bits.is_some_and(|bits| bits % 8 == 0))
        && !class
            .unsupported_members
            .iter()
            .any(|m| m.construct == "base_class")
}

/// The Rust layout of a value type.
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
            "The enum value `{}` is a Rust keyword; it cannot be bound.",
            variant.name
        ));
    }
    rules::enum_repr(&enumeration.underlying)
}

/// Assigns Rust names (ADR-0005).
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

/// Constant names: if the same name exists in more than one namespace, a namespace prefix (`units_FEET`);
/// keywords are escaped with `_`. A constant that clashes with a safe function (in the same
/// namespace in Rust) is not bound.
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
                    "In Rust, constants and functions share a namespace; it clashes with the function named \
                     `{name}`."
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
