//! Turning the libclang AST into IR.
//!
//! Only declarations in the target headers (the bridge's headers) are taken; other files they
//! `#include` and the prelude are not. For types declared in another file, the file name is
//! written to the IR (`declared_in`), and analysis tells the user which header to add.
//! No unsupported construct is skipped silently: it is recorded with a reason as an
//! `Item::Unsupported`, in `Class::unsupported_members` or as a `Type::Unsupported`.
//!
//! Deliberately not recorded (because they are not an API to bind): forward declarations
//! (`class Foo;`), typedefs/usings (resolved in the type where they are used), functions deleted
//! with `= delete`, destructors (called by `UniquePtr`), private/protected members.
//!
//! Constants (`Item::Constant`) are taken only if their values are known exactly at compile
//! time; see [`InitializerCheck`].

use std::path::Path;

use clang_sys as cx;
use ferrobridge_ir::{
    Class, Constant, ConstantValue, Constructor, Destructor, Enum, EnumVariant, Field, Function,
    Item, Layout, Method, Module, Param, QualType, SourceLocation, TemplateInstance, Type,
    Unsupported,
};

use crate::clang::{self, Cursor, Evaluated, File, TranslationUnit};

/// The IR name of an anonymous namespace.
const ANONYMOUS_NAMESPACE: &str = "(anonymous)";

const REASON_UNION: &str = "Unions are not supported yet.";
const REASON_ANONYMOUS: &str = "Unnamed classes and enums cannot be named from Rust.";
const REASON_NESTED: &str = "Nested types (a class/enum defined inside a class) are a v0.2 goal.";
const REASON_TEMPLATE: &str = "Templates are outside the MVP scope (a v0.2 goal).";
const REASON_CLASS_TEMPLATE: &str = "Class template: cannot be bound without being instantiated with a concrete type; \
     instantiate it with `[templates]` in `ferrobridge.toml` (e.g. `Vector3f = \"Vector3<float>\"`).";
const REASON_SPECIALIZATION: &str = "Explicit specialization of a class template: not bound; only \
     instantiations of the primary template requested with `[templates]` are.";
const REASON_DEPENDENT: &str = "This type depending on a template parameter is not supported yet.";
const REASON_MEMBER_TEMPLATE_PARAM: &str =
    "A member template's parameter; only the class's own parameters can be substituted.";
const REASON_BIT_FIELD: &str = "Bit fields do not have a layout with a counterpart in Rust.";
const REASON_NOT_A_CLASS_TEMPLATE: &str =
    "The `[templates]` entry is not an instantiation of a class template.";
const REASON_NON_TYPE_PARAMS: &str =
    "Only class templates with type parameters can be instantiated (`template <typename T>`).";
const REASON_VARIABLE: &str = "Global and static variables are not supported yet.";
const REASON_CONSTANT_TYPE: &str = "Only constants of numeric and `bool` types are bound.";
const REASON_CONSTANT_CALL: &str = "The constant's initializer (or that of a constant it depends on) contains a function \
     call; the value may be computed at run time, and it is not certain to be the same as the value \
     computed at compile time.";
const REASON_CONSTANT_UNKNOWN: &str = "The constant's value could not be computed (it has no initializer, or it is not a constant expression).";
const REASON_CONSTANT_NON_FINITE: &str = "The constant's value is not finite (NaN or infinite); not bound because \
     the bit pattern of a NaN cannot be kept in Rust.";
const REASON_CONSTANT_RANGE: &str = "The constant's value does not fit its type.";
const REASON_CONSTANT_PLATFORM: &str = "The constant's initializer contains `sizeof`, `alignof` or `offsetof`; \
     its value depends on the platform and would be wrong on another platform.";
const REASON_OPERATOR: &str = "This operator is not bound (assignment, `new`/`delete`, `->`, `++`/`--`, \
     `()`, `,` and unary `&`/`*`); arithmetic and comparison operators are bound.";
const REASON_INDEX_OPERATOR: &str = "`operator[]` is not bound: in C++ an index is usually used without \
     bounds checks; an out-of-bounds index from safe Rust would be undefined behavior. \
     Use the fields (`v.x`) of value types.";
const REASON_FREE_OPERATOR: &str = "Free (non-member) operator functions are not bound yet.";
const REASON_CONVERSION: &str = "Conversion operators are a v0.2 goal.";
const REASON_INHERITANCE: &str =
    "Inheritance is a v0.2 goal: the base class's methods are not bound through this class.";
const REASON_INTERNAL: &str = "The definition of the internal-linkage (`static`) function is not visible in the header; \
     it cannot be called from outside the header.";
const REASON_ANONYMOUS_FUNCTION: &str =
    "Functions in an anonymous namespace have no name that can be written from Rust.";
const REASON_VARIADIC: &str =
    "Functions taking a variable number of arguments (`...`) are not supported.";
const REASON_ENUM_RANGE: &str = "The enum value does not fit in a 64-bit signed integer.";
const REASON_STD_TYPE: &str = "This standard library type is not supported yet.";
const REASON_CUSTOM_DELETER: &str = "`std::unique_ptr` with a custom deleter is not supported.";
const REASON_CUSTOM_ALLOCATOR: &str = "`std::vector` with a custom allocator is not supported.";
const REASON_UNKNOWN_DECL: &str = "This kind of declaration is not recognized yet.";
const REASON_UNKNOWN_TYPE: &str = "This type is not supported yet.";

/// At most this many steps are followed in a typedef chain (a safety limit against cycles).
const MAX_TYPEDEF_DEPTH: usize = 16;
/// At most this deep into the base class chain; anything beyond counts as unknown.
const MAX_BASE_DEPTH: usize = 32;
/// At most this deep into a constant's initializer and the constants it depends on; anything
/// beyond counts as not certain.
const MAX_INITIALIZER_DEPTH: usize = 64;
/// The maximum number of nodes visited across all constant initializers of a header; anything
/// beyond counts as not certain (rejected conservatively).
const MAX_INITIALIZER_NODES: usize = 100_000;

/// The counterpart of a `[templates]` entry in the synthetic unit.
pub(crate) struct InstanceAlias {
    /// The Rust name (e.g. `Vector3f`).
    pub(crate) rust_name: String,
    /// The `using` alias in the synthetic unit (e.g. `__ferrobridge_inst_0`).
    pub(crate) alias: String,
}

/// Turns the declarations in the `targets` files of the translation unit, and the requested
/// template instantiations, into an IR module.
pub(crate) fn lower<'tu>(
    tu: &'tu TranslationUnit<'_>,
    targets: Vec<File<'tu>>,
    aliases: &[InstanceAlias],
) -> Module {
    let mut lowering = Lowering {
        types: Types {
            targets,
            instances: Vec::new(),
            env: None,
        },
        namespace: Vec::new(),
        items: Vec::new(),
        initializers: InitializerCheck::default(),
    };
    lowering.resolve_instances(tu.cursor(), aliases);
    lowering.visit_children(tu.cursor());
    lowering.lower_instances();
    Module {
        items: lowering.items,
        ..Module::new()
    }
}

/// A resolved template instantiation.
#[derive(Clone)]
struct Instance<'tu> {
    rust_name: String,
    namespace: Vec<String>,
    /// The template's name (e.g. `Vector3`).
    template_name: String,
    /// The template's pattern (`ClassTemplate`).
    template: Cursor<'tu>,
    /// The concrete arguments, in order.
    args: Vec<clang::Type<'tu>>,
    /// The canonical type of the instantiation (`Vector3<float>`).
    ty: clang::Type<'tu>,
    /// The declaration of the instantiation (the specialization).
    declaration: Cursor<'tu>,
}

/// The types to substitute for the parameters while lowering a template pattern.
#[derive(Clone)]
struct Env {
    /// The instantiation being lowered (its position in the `Instance` list).
    instance: usize,
}

/// The context of type lowering.
#[derive(Clone)]
pub(crate) struct Types<'tu> {
    /// The headers whose declarations are taken (the bridge's headers).
    targets: Vec<File<'tu>>,
    /// The requested template instantiations (`[templates]`).
    instances: Vec<Instance<'tu>>,
    /// If a template pattern is being lowered, the instantiation to substitute for the parameters.
    env: Option<Env>,
}

impl<'tu> Types<'tu> {
    /// The instantiation of this template requested with these arguments.
    fn instance_of(
        &self,
        template: Cursor<'_>,
        args: &[clang::Type<'_>],
    ) -> Option<&Instance<'tu>> {
        self.instances.iter().find(|inst| {
            inst.template.same_as(&template)
                && inst.args.len() == args.len()
                && inst
                    .args
                    .iter()
                    .zip(args)
                    .all(|(a, b)| a.same_as(&b.canonical()))
        })
    }

    /// Is this concrete type a requested instantiation?
    fn instance_for_type(&self, canonical: clang::Type<'_>) -> Option<&Instance<'tu>> {
        // The declarations are compared instead of the types: `const Vector3<float>` and `Vector3<float>`
        // are the same specialization.
        let declaration = canonical.declaration();
        self.instances
            .iter()
            .find(|inst| inst.declaration.same_as(&declaration))
    }

    /// Was at least one instantiation requested for this template?
    fn is_instantiated(&self, template: Cursor<'_>) -> bool {
        self.instances
            .iter()
            .any(|inst| inst.template.same_as(&template))
    }
    /// Is the declaration in one of the bridge's headers?
    fn contains(&self, cursor: Cursor<'_>) -> bool {
        self.targets.iter().any(|file| cursor.is_in_file(*file))
    }

    /// The file name of a type declared outside the bridge; `None` if inside.
    fn declared_outside(&self, declaration: Cursor<'_>) -> Option<String> {
        if self.contains(declaration) {
            return None;
        }
        let file = declaration.location().file;
        let name = Path::new(&file)
            .file_name()
            .map_or(file.clone(), |n| n.to_string_lossy().into_owned());
        (!name.is_empty()).then_some(name)
    }
}

struct Lowering<'tu> {
    types: Types<'tu>,
    namespace: Vec<String>,
    items: Vec<Item>,
    initializers: InitializerCheck<'tu>,
}

impl<'tu> Lowering<'tu> {
    /// Finds the `[templates]` aliases in the synthetic unit and resolves the instantiations. Those
    /// that cannot be resolved become `unsupported` with their reasons.
    fn resolve_instances(&mut self, root: Cursor<'tu>, aliases: &[InstanceAlias]) {
        if aliases.is_empty() {
            return;
        }
        let declarations = root.children();
        for alias in aliases {
            let Some(decl) = declarations
                .iter()
                .find(|c| c.kind() == cx::CXCursor_TypeAliasDecl && c.spelling() == alias.alias)
            else {
                continue;
            };
            match resolve_instance(*decl, &alias.rust_name) {
                Ok(instance) => self.types.instances.push(instance),
                Err(reason) => self.items.push(Item::Unsupported(unsupported(
                    *decl,
                    "template_instance",
                    alias.rust_name.clone(),
                    reason,
                ))),
            }
        }
    }

    /// Lowers the resolved instantiations as classes (from the pattern, with the parameters substituted).
    fn lower_instances(&mut self) {
        for index in 0..self.types.instances.len() {
            let instance = self.types.instances[index].clone();
            let mut types = self.types.clone();
            types.env = Some(Env { instance: index });
            let spelling = instance.ty.spelling();
            let class = build_class(
                &types,
                ClassSource {
                    members: instance.template,
                    record: instance.declaration,
                    layout: instance.ty,
                },
                instance.rust_name.clone(),
                instance.namespace.clone(),
                &spelling,
                Some(TemplateInstance {
                    template: instance.template_name.clone(),
                    spelling: spelling.clone(),
                }),
            );
            self.items.push(Item::Class(class));
        }
    }

    fn visit_children(&mut self, parent: Cursor<'tu>) {
        for child in parent.children() {
            if self.types.contains(child) {
                self.visit(child);
            }
        }
    }

    fn visit(&mut self, cursor: Cursor<'tu>) {
        match cursor.kind() {
            cx::CXCursor_Namespace => {
                let name = cursor.spelling();
                self.namespace.push(if name.is_empty() {
                    ANONYMOUS_NAMESPACE.to_owned()
                } else {
                    name
                });
                self.visit_children(cursor);
                self.namespace.pop();
            }
            _ if is_linkage_spec(cursor) => self.visit_children(cursor),
            cx::CXCursor_FunctionDecl => self.function(cursor),
            // `template <> struct is_real<float> { … }`: a class whose name is the template's, once
            // per specialization.
            cx::CXCursor_StructDecl | cx::CXCursor_ClassDecl
                if cursor.is_definition() && !cursor.specialized_template().is_null() =>
            {
                self.unsupported(
                    cursor,
                    "class_template_specialization",
                    REASON_SPECIALIZATION,
                );
            }
            cx::CXCursor_StructDecl | cx::CXCursor_ClassDecl if cursor.is_definition() => {
                self.class(cursor);
            }
            cx::CXCursor_EnumDecl if cursor.is_definition() => self.enumeration(cursor),
            cx::CXCursor_UnionDecl if cursor.is_definition() => {
                self.unsupported(cursor, "union", REASON_UNION);
            }
            cx::CXCursor_FunctionTemplate => {
                self.unsupported(cursor, "function_template", REASON_TEMPLATE);
            }
            // `lower_instances` lowers the requested instantiations; the template itself is not bound.
            // A forward declaration (`template <typename T> class Vec2;`): nothing to bind.
            cx::CXCursor_ClassTemplate if !cursor.is_definition() => {}
            cx::CXCursor_ClassTemplate if self.types.is_instantiated(cursor) => {}
            cx::CXCursor_ClassTemplate => {
                self.unsupported(cursor, "class_template", REASON_CLASS_TEMPLATE);
            }
            cx::CXCursor_ClassTemplatePartialSpecialization => {
                self.unsupported(cursor, "class_template", REASON_TEMPLATE);
            }
            cx::CXCursor_TypeAliasTemplateDecl => {
                self.unsupported(cursor, "alias_template", REASON_TEMPLATE);
            }
            cx::CXCursor_VarDecl => self.variable(cursor),
            // Not an API to bind (see the module documentation).
            cx::CXCursor_StructDecl
            | cx::CXCursor_ClassDecl
            | cx::CXCursor_EnumDecl
            | cx::CXCursor_UnionDecl
            | cx::CXCursor_TypedefDecl
            | cx::CXCursor_TypeAliasDecl
            | cx::CXCursor_UsingDirective
            | cx::CXCursor_UsingDeclaration
            | cx::CXCursor_NamespaceAlias
            | cx::CXCursor_StaticAssert => {}
            _ => {
                let construct = cursor.kind_spelling();
                self.unsupported(cursor, &construct, REASON_UNKNOWN_DECL);
            }
        }
    }

    fn function(&mut self, cursor: Cursor<'_>) {
        if cursor.is_deleted() {
            return;
        }
        if cursor.spelling().starts_with("operator") {
            return self.unsupported(cursor, "operator", REASON_FREE_OPERATOR);
        }
        // An internal-linkage (`static`) function can be bound if its definition is in the header: because
        // the C++ files generated by the shim and cxx include the header, it is defined in that translation unit.
        // Since C++11 the linkage of functions in an anonymous namespace is `Internal`; they are also
        // recognized from the namespace path.
        let linkage = cursor.linkage();
        if linkage == cx::CXLinkage_UniqueExternal
            || self.namespace.iter().any(|ns| ns == ANONYMOUS_NAMESPACE)
        {
            return self.unsupported(cursor, "function", REASON_ANONYMOUS_FUNCTION);
        }
        if linkage == cx::CXLinkage_Internal && !cursor.is_definition() {
            return self.unsupported(cursor, "function", REASON_INTERNAL);
        }
        if cursor.ty().is_variadic() {
            return self.unsupported(cursor, "variadic_function", REASON_VARIADIC);
        }
        self.items.push(Item::Function(Function {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            params: params(&self.types, cursor),
            return_type: lower_type(&self.types, cursor.result_type()),
            noexcept: is_noexcept(cursor.exception_spec()),
            location: location(cursor),
        }));
    }

    /// Variables of `const`/`constexpr` numeric or `bool` type become constants if their values are
    /// known exactly at compile time; other variables are not supported.
    fn variable(&mut self, cursor: Cursor<'tu>) {
        let lowered = lower_type(&self.types, cursor.ty());
        if !lowered.is_const {
            return self.unsupported(cursor, "variable", REASON_VARIABLE);
        }
        if !matches!(
            lowered.ty,
            Type::Integer { .. } | Type::Float { .. } | Type::Bool
        ) {
            return self.unsupported(cursor, "constant", REASON_CONSTANT_TYPE);
        }
        match self.initializers.check(cursor, 0) {
            Initializer::Exact => {}
            Initializer::Call => {
                return self.unsupported(cursor, "constant", REASON_CONSTANT_CALL);
            }
            Initializer::Platform => {
                return self.unsupported(cursor, "constant", REASON_CONSTANT_PLATFORM);
            }
        }
        let Some(evaluated) = cursor.evaluate() else {
            return self.unsupported(cursor, "constant", REASON_CONSTANT_UNKNOWN);
        };
        let value = match constant_value(&lowered.ty, evaluated) {
            Ok(value) => value,
            Err(reason) => return self.unsupported(cursor, "constant", reason),
        };
        self.items.push(Item::Constant(Constant {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            ty: lowered.ty,
            value,
            location: location(cursor),
        }));
    }

    fn class(&mut self, cursor: Cursor<'_>) {
        if cursor.is_anonymous() {
            return self.unsupported(cursor, "class", REASON_ANONYMOUS);
        }
        let name = cursor.spelling();
        let qualified = self.qualified(&name);
        let class = build_class(
            &self.types,
            ClassSource {
                members: cursor,
                record: cursor,
                layout: cursor.ty(),
            },
            name,
            self.namespace.clone(),
            &qualified,
            None,
        );
        self.items.push(Item::Class(class));
    }

    fn enumeration(&mut self, cursor: Cursor<'_>) {
        if cursor.is_anonymous() {
            return self.unsupported(cursor, "enum", REASON_ANONYMOUS);
        }
        let underlying = lower_type(&self.types, cursor.enum_integer_type()).ty;
        let unsigned = matches!(underlying, Type::Integer { signed: false, .. });
        let mut variants = Vec::new();
        for constant in cursor.children() {
            if constant.kind() != cx::CXCursor_EnumConstantDecl {
                continue;
            }
            let value = if unsigned {
                i64::try_from(constant.enum_unsigned_value()).ok()
            } else {
                Some(constant.enum_value())
            };
            let Some(value) = value else {
                return self.unsupported(cursor, "enum", REASON_ENUM_RANGE);
            };
            variants.push(EnumVariant {
                name: constant.spelling(),
                value,
            });
        }
        self.items.push(Item::Enum(Enum {
            name: cursor.spelling(),
            namespace: self.namespace.clone(),
            scoped: cursor.is_scoped_enum(),
            underlying,
            variants,
            location: location(cursor),
        }));
    }

    fn unsupported(&mut self, cursor: Cursor<'_>, construct: &str, reason: &str) {
        let name = self.qualified(&cursor.spelling());
        self.items.push(Item::Unsupported(unsupported(
            cursor, construct, name, reason,
        )));
    }

    /// The full name with the current namespace (`geo::detail::f`).
    fn qualified(&self, name: &str) -> String {
        let mut path = self.namespace.clone();
        if !name.is_empty() {
            path.push(name.to_owned());
        }
        path.join("::")
    }
}

/// The result of checking a constant's initializer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Initializer {
    /// Literals, arithmetic and conversions: the same value at compile time and at run time.
    Exact,
    /// There is a function call (or the check's limit was exceeded).
    Call,
    /// `sizeof`, `alignof` or `offsetof`: the value depends on the platform where generation is done.
    Platform,
}

/// Checks constant initializers. Verified variables are remembered: every initializer is walked
/// once (chains such as `A2 = A1 + A1` take linear, not exponential, time).
///
/// Why: `static const double X = sqrt(2.0);` is not a C++ constant expression; the compiler may
/// compute the value at run time with libm, while libclang folds the value itself at compile time.
/// Because it is not certain that the two come out the same in the last bit, writing such a value
/// into Rust as a `const` would be a guess. Values such as `sizeof(void*)` also depend on the
/// machine where generation is done; a committed file would be wrong on another platform (ADR-0004).
/// Literals, arithmetic and conversions, on the other hand, give the same result everywhere under IEEE rules.
#[derive(Default)]
struct InitializerCheck<'tu> {
    exact: Vec<Cursor<'tu>>,
    visited: usize,
}

impl<'tu> InitializerCheck<'tu> {
    fn check(&mut self, variable: Cursor<'tu>, depth: usize) -> Initializer {
        if self.exact.iter().any(|known| known.same_as(&variable)) {
            return Initializer::Exact;
        }
        if depth > MAX_INITIALIZER_DEPTH {
            return Initializer::Call;
        }
        let mut stack = variable.children();
        while let Some(node) = stack.pop() {
            self.visited += 1;
            if self.visited > MAX_INITIALIZER_NODES {
                return Initializer::Call;
            }
            match node.kind() {
                cx::CXCursor_CallExpr => return Initializer::Call,
                // `sizeof`/`alignof` (`UnaryExpr`) and `offsetof` (it has no kind of its own in libclang;
                // a reference to a field without an object, `MemberRef`, appears only there or in a member
                // pointer, and a member pointer is not a numeric type).
                cx::CXCursor_UnaryExpr | cx::CXCursor_MemberRef => return Initializer::Platform,
                cx::CXCursor_DeclRefExpr => {
                    let target = node.referenced();
                    if target.kind() == cx::CXCursor_VarDecl && !target.same_as(&variable) {
                        let result = self.check(target, depth + 1);
                        if result != Initializer::Exact {
                            return result;
                        }
                    }
                }
                _ => {}
            }
            stack.extend(node.children());
        }
        self.exact.push(variable);
        Initializer::Exact
    }
}

/// Turns the value computed by libclang into an IR value according to the constant's type.
fn constant_value(ty: &Type, evaluated: Evaluated) -> Result<ConstantValue, &'static str> {
    match (ty, evaluated) {
        (Type::Bool, Evaluated::Signed(v)) => Ok(ConstantValue::Bool(v != 0)),
        (Type::Bool, Evaluated::Unsigned(v)) => Ok(ConstantValue::Bool(v != 0)),
        (Type::Integer { signed: true, .. }, Evaluated::Signed(v)) => Ok(ConstantValue::Signed(v)),
        (Type::Integer { signed: true, .. }, Evaluated::Unsigned(v)) => i64::try_from(v)
            .map(ConstantValue::Signed)
            .map_err(|_| REASON_CONSTANT_RANGE),
        (Type::Integer { signed: false, .. }, Evaluated::Unsigned(v)) => {
            Ok(ConstantValue::Unsigned(v))
        }
        (Type::Integer { signed: false, .. }, Evaluated::Signed(v)) => u64::try_from(v)
            .map(ConstantValue::Unsigned)
            .map_err(|_| REASON_CONSTANT_RANGE),
        (Type::Float { size_bytes, .. }, Evaluated::Float(v)) => {
            if !v.is_finite() {
                return Err(REASON_CONSTANT_NON_FINITE);
            }
            // A `float` constant arrives as a `double`; if it does not convert to `f32` without loss, the value
            // is not a `float` and libclang's result cannot be trusted.
            #[allow(clippy::cast_possible_truncation)]
            let narrowed = f64::from(v as f32);
            if *size_bytes == 4 && narrowed != v {
                return Err(REASON_CONSTANT_UNKNOWN);
            }
            Ok(ConstantValue::Float(v))
        }
        _ => Err(REASON_CONSTANT_UNKNOWN),
    }
}

/// Where a class's information is read from.
struct ClassSource<'tu> {
    /// The cursor whose members are walked: the class itself or the template pattern.
    members: Cursor<'tu>,
    /// The declaration of the class (or instantiation): abstractness comes from here.
    record: Cursor<'tu>,
    /// The concrete type whose layout (size, field offsets) is read.
    layout: clang::Type<'tu>,
}

/// Turns a class or template instantiation into IR.
fn build_class<'tu>(
    types: &Types<'tu>,
    source: ClassSource<'tu>,
    name: String,
    namespace: Vec<String>,
    qualified: &str,
    instance_of: Option<TemplateInstance>,
) -> Class {
    let lifetime = record_lifetime(source.members, 0);
    let layout = match (source.layout.size_of(), source.layout.align_of()) {
        (Some(size_bytes), Some(align_bytes)) => Some(Layout {
            size_bytes,
            align_bytes,
        }),
        _ => None,
    };
    let mut class = Class {
        name,
        namespace,
        is_abstract: source.record.is_abstract_record(),
        is_polymorphic: lifetime.polymorphic,
        destructor: lifetime.destructor(),
        constructors: Vec::new(),
        methods: Vec::new(),
        unsupported_members: Vec::new(),
        fields: Vec::new(),
        layout,
        user_copy_or_move: false,
        instance_of,
        location: location(source.members),
    };
    for member in source.members.children() {
        class_member(types, &mut class, qualified, member, source.layout);
    }
    class
}

/// Resolves the instantiation from the alias `using __ferrobridge_inst_N = Tmpl<args>;`.
fn resolve_instance<'tu>(
    alias: Cursor<'tu>,
    rust_name: &str,
) -> Result<Instance<'tu>, &'static str> {
    let ty = alias.typedef_underlying().canonical();
    let declaration = ty.declaration();
    let template = definition_of(declaration.specialized_template());
    if template.is_null() || template.kind() != cx::CXCursor_ClassTemplate {
        return Err(REASON_NOT_A_CLASS_TEMPLATE);
    }
    let params: Vec<cx::CXCursorKind> = template
        .children()
        .iter()
        .map(Cursor::kind)
        .filter(|kind| {
            matches!(
                *kind,
                cx::CXCursor_TemplateTypeParameter
                    | cx::CXCursor_NonTypeTemplateParameter
                    | cx::CXCursor_TemplateTemplateParameter
            )
        })
        .collect();
    let args = ty.template_args();
    let only_types = params
        .iter()
        .all(|kind| *kind == cx::CXCursor_TemplateTypeParameter);
    if !only_types
        || args.len() != params.len()
        || args.iter().any(|a| a.kind() == cx::CXType_Invalid)
    {
        return Err(REASON_NON_TYPE_PARAMS);
    }
    let Some((namespace, template_name)) = scope_path(template) else {
        return Err(REASON_NESTED);
    };
    Ok(Instance {
        rust_name: rust_name.to_owned(),
        namespace,
        template_name,
        template,
        args: args.iter().map(clang::Type::canonical).collect(),
        ty,
        declaration,
    })
}

/// In a template pattern, turns a type depending on a parameter into the instantiation's
/// concrete type. `None` if it is not dependent (it is lowered the normal way).
fn substitute(types: &Types<'_>, env: &Env, ty: clang::Type<'_>) -> Option<QualType> {
    let canonical = ty.canonical();
    if canonical.kind() != cx::CXType_Unexposed {
        return None;
    }
    let instance = &types.instances[env.instance];
    let is_const = ty.is_const();
    let lowered = match template_param(&canonical) {
        Some((0, index)) => match instance.args.get(index) {
            Some(arg) => {
                let mut plain = types.clone();
                plain.env = None;
                lower_type(&plain, *arg).ty
            }
            None => unsupported_type(ty, REASON_DEPENDENT),
        },
        Some(_) => unsupported_type(ty, REASON_MEMBER_TEMPLATE_PARAM),
        None => dependent_record(types, instance, ty),
    };
    Some(QualType {
        is_const,
        ty: lowered,
    })
}

/// The definition of a declaration; forward declarations (`template <typename T> class Vec2;`) have no body.
fn definition_of(cursor: Cursor<'_>) -> Cursor<'_> {
    let definition = cursor.definition();
    if definition.is_null() {
        cursor
    } else {
        definition
    }
}

/// `type-parameter-<depth>-<index>`: libclang's canonical spelling of template parameters
/// (the parameter's declaration cannot be reached from the type).
fn template_param(canonical: &clang::Type<'_>) -> Option<(u32, usize)> {
    let spelling = unqualified_spelling(*canonical);
    let rest = spelling.strip_prefix("type-parameter-")?;
    let (depth, index) = rest.split_once('-')?;
    Some((depth.parse().ok()?, index.parse().ok()?))
}

/// A dependent template reference in the pattern such as `Vector3<T>`, `const Vector2<T>&`: that class
/// if, with the arguments substituted, it matches a requested instantiation; otherwise unsupported with a reason.
fn dependent_record(types: &Types<'_>, current: &Instance<'_>, ty: clang::Type<'_>) -> Type {
    let declaration = {
        let named = ty.declaration();
        if named.is_null() {
            ty.canonical().declaration()
        } else {
            named
        }
    };
    let args = ty.template_args();
    // The class itself, written by name only (`Vector3`) inside the class: libclang gives the
    // declaration as the template's pattern class; the same name and the same location as the template.
    let is_current = declaration.same_as(&current.template)
        || declaration.same_as(&current.declaration)
        || (matches!(
            declaration.kind(),
            cx::CXCursor_ClassDecl | cx::CXCursor_StructDecl
        ) && declaration.spelling() == current.template.spelling()
            && declaration.location() == current.template.location());
    if args.is_empty() && is_current {
        return Type::Record {
            name: current.rust_name.clone(),
            namespace: current.namespace.clone(),
            declared_in: None,
        };
    }
    let declaration = definition_of(declaration);
    if declaration.kind() != cx::CXCursor_ClassTemplate || args.is_empty() {
        return unsupported_type(ty, REASON_DEPENDENT);
    }
    let mut concrete = Vec::with_capacity(args.len());
    for arg in args {
        let canonical = arg.canonical();
        match template_param(&canonical) {
            Some((0, index)) => match current.args.get(index) {
                Some(found) => concrete.push(*found),
                None => return unsupported_type(ty, REASON_DEPENDENT),
            },
            Some(_) => return unsupported_type(ty, REASON_MEMBER_TEMPLATE_PARAM),
            None if canonical.kind() == cx::CXType_Unexposed => {
                return unsupported_type(ty, REASON_DEPENDENT);
            }
            None => concrete.push(canonical),
        }
    }
    match types.instance_of(declaration, &concrete) {
        Some(instance) => Type::Record {
            name: instance.rust_name.clone(),
            namespace: instance.namespace.clone(),
            declared_in: None,
        },
        None => {
            let spelled: Vec<String> = concrete.iter().map(|t| t.spelling()).collect();
            Type::Unsupported {
                spelling: unqualified_spelling(ty),
                reason: format!(
                    "`{}<{}>` is not instantiated; add it to `[templates]` in `ferrobridge.toml`.",
                    declaration.spelling(),
                    spelled.join(", ")
                ),
            }
        }
    }
}

/// Information about deleting a class (including what comes from base classes).
struct RecordLifetime {
    /// It or a base declares a virtual function.
    polymorphic: bool,
    /// The explicitly declared destructor (`None` if there is none).
    declared: Option<DeclaredDestructor>,
    /// A base's destructor is virtual: this class's is virtual too.
    base_virtual: bool,
    /// A base's destructor is private or deleted: the implicit destructor counts as deleted.
    base_blocks_implicit: bool,
}

/// The properties of an explicitly declared destructor.
#[derive(Clone, Copy)]
struct DeclaredDestructor {
    access: cx::CX_CXXAccessSpecifier,
    deleted: bool,
    is_virtual: bool,
}

impl RecordLifetime {
    /// The most restrictive assumption for a base whose definition cannot be seen: polymorphic,
    /// without a virtual destructor.
    fn unknown() -> Self {
        RecordLifetime {
            polymorphic: true,
            declared: None,
            base_virtual: false,
            base_blocks_implicit: false,
        }
    }

    fn destructor(&self) -> Destructor {
        match self.declared {
            Some(d) if d.deleted => Destructor::Deleted,
            Some(d) if d.access != cx::CX_CXXPublic => Destructor::NotPublic,
            Some(d) => Destructor::Public {
                is_virtual: d.is_virtual || self.base_virtual,
            },
            None if self.base_blocks_implicit => Destructor::Deleted,
            None => Destructor::Public {
                is_virtual: self.base_virtual,
            },
        }
    }

    /// Does this class's destructor pass virtuality on to subclasses?
    fn has_virtual_destructor(&self) -> bool {
        self.declared.is_some_and(|d| d.is_virtual) || self.base_virtual
    }

    /// Can a subclass's implicit destructor not call this base's destructor?
    fn blocks_implicit_in_derived(&self) -> bool {
        match self.declared {
            Some(d) => d.deleted || d.access == cx::CX_CXXPrivate,
            None => self.base_blocks_implicit,
        }
    }
}

/// Finds a class's destructor and polymorphism by looking at its base classes recursively.
/// The destructors of members (fields) are not taken into account: a deleted field
/// destructor deletes the implicit destructor, but that leads to a compile error, not UB.
fn record_lifetime(record: Cursor<'_>, depth: usize) -> RecordLifetime {
    if depth > MAX_BASE_DEPTH || !record.is_definition() {
        return RecordLifetime::unknown();
    }
    let mut info = RecordLifetime {
        polymorphic: false,
        declared: None,
        base_virtual: false,
        base_blocks_implicit: false,
    };
    for member in record.children() {
        match member.kind() {
            cx::CXCursor_CXXBaseSpecifier => {
                let base = record_lifetime(member.ty().canonical().declaration(), depth + 1);
                info.polymorphic |= base.polymorphic;
                info.base_virtual |= base.has_virtual_destructor();
                info.base_blocks_implicit |= base.blocks_implicit_in_derived();
            }
            cx::CXCursor_Destructor => {
                let is_virtual = member.is_virtual_method();
                info.polymorphic |= is_virtual;
                info.declared = Some(DeclaredDestructor {
                    access: member.access(),
                    deleted: member.is_deleted(),
                    is_virtual,
                });
            }
            cx::CXCursor_CXXMethod => info.polymorphic |= member.is_virtual_method(),
            _ => {}
        }
    }
    info
}

/// Adds a class member to the class. Only public members are taken into account.
fn class_member(
    types: &Types<'_>,
    class: &mut Class,
    class_name: &str,
    member: Cursor<'_>,
    layout: clang::Type<'_>,
) {
    let kind = member.kind();
    // For the value type decision (is it trivially copyable?), whatever its access.
    let user_defined = |m: &Cursor<'_>| !m.is_defaulted();
    match kind {
        cx::CXCursor_FieldDecl => {
            let name = member.spelling();
            let ty = if member.is_bit_field() {
                QualType {
                    is_const: false,
                    ty: unsupported_type(member.ty(), REASON_BIT_FIELD),
                }
            } else {
                lower_type(types, member.ty())
            };
            let offset_bits = std::ffi::CString::new(name.clone())
                .ok()
                .and_then(|c| layout.offset_of(&c));
            class.fields.push(Field {
                name,
                ty,
                public: member.access() == cx::CX_CXXPublic,
                offset_bits,
                location: location(member),
            });
            return;
        }
        cx::CXCursor_Constructor
            if member.is_copy_or_move_constructor() && user_defined(&member) =>
        {
            class.user_copy_or_move = true;
        }
        cx::CXCursor_CXXMethod if member.is_copy_or_move_assignment() && user_defined(&member) => {
            class.user_copy_or_move = true;
        }
        cx::CXCursor_Destructor if user_defined(&member) => class.user_copy_or_move = true,
        // The parameters of a template pattern are not members.
        cx::CXCursor_TemplateTypeParameter
        | cx::CXCursor_NonTypeTemplateParameter
        | cx::CXCursor_TemplateTemplateParameter => return,
        _ => {}
    }
    // What matters is the existence of inheritance, not its access (public/private base): it is reported.
    if kind == cx::CXCursor_CXXBaseSpecifier {
        let name = format!("{class_name} : {}", member.spelling());
        class
            .unsupported_members
            .push(unsupported(member, "base_class", name, REASON_INHERITANCE));
        return;
    }
    if member.access() != cx::CX_CXXPublic {
        return;
    }
    let member_name = format!("{class_name}::{}", member.spelling());
    match kind {
        cx::CXCursor_Constructor if !member.is_deleted() => {
            class.constructors.push(Constructor {
                params: params(types, member),
                noexcept: is_noexcept(member.exception_spec()),
                location: location(member),
            });
        }
        cx::CXCursor_CXXMethod if !member.is_deleted() => {
            let operator = member
                .spelling()
                .strip_prefix("operator")
                .map(|op| op.trim().to_owned());
            if let Some(op) = &operator {
                let arity = member.arguments().len();
                if op == "[]" {
                    class.unsupported_members.push(unsupported(
                        member,
                        "operator",
                        member_name,
                        REASON_INDEX_OPERATOR,
                    ));
                    return;
                }
                if !bindable_operator(op, arity) {
                    class.unsupported_members.push(unsupported(
                        member,
                        "operator",
                        member_name,
                        REASON_OPERATOR,
                    ));
                    return;
                }
            }
            class.methods.push(Method {
                name: member.spelling(),
                params: params(types, member),
                return_type: lower_type(types, member.result_type()),
                is_const: member.is_const_method(),
                is_static: member.is_static_method(),
                is_virtual: member.is_virtual_method(),
                noexcept: is_noexcept(member.exception_spec()),
                operator,
                location: location(member),
            });
        }
        cx::CXCursor_ConversionFunction => class.unsupported_members.push(unsupported(
            member,
            "conversion_operator",
            member_name,
            REASON_CONVERSION,
        )),
        cx::CXCursor_VarDecl => class.unsupported_members.push(unsupported(
            member,
            "static_variable",
            member_name,
            REASON_VARIABLE,
        )),
        cx::CXCursor_StructDecl
        | cx::CXCursor_ClassDecl
        | cx::CXCursor_UnionDecl
        | cx::CXCursor_EnumDecl => class.unsupported_members.push(unsupported(
            member,
            "nested_type",
            member_name,
            REASON_NESTED,
        )),
        cx::CXCursor_FunctionTemplate => class.unsupported_members.push(unsupported(
            member,
            "method_template",
            member_name,
            REASON_TEMPLATE,
        )),
        // Not an API to bind (see the module documentation).
        cx::CXCursor_Constructor
        | cx::CXCursor_CXXMethod
        | cx::CXCursor_Destructor
        | cx::CXCursor_CXXAccessSpecifier
        | cx::CXCursor_FriendDecl
        | cx::CXCursor_TypedefDecl
        | cx::CXCursor_TypeAliasDecl
        | cx::CXCursor_UsingDeclaration
        | cx::CXCursor_StaticAssert => {}
        _ => {
            let construct = member.kind_spelling();
            class.unsupported_members.push(unsupported(
                member,
                &construct,
                member_name,
                REASON_UNKNOWN_DECL,
            ));
        }
    }
}

fn unsupported(cursor: Cursor<'_>, construct: &str, name: String, reason: &str) -> Unsupported {
    Unsupported {
        construct: construct.to_owned(),
        name,
        reason: reason.to_owned(),
        location: location(cursor),
    }
}

fn params(types: &Types<'_>, cursor: Cursor<'_>) -> Vec<Param> {
    cursor
        .arguments()
        .into_iter()
        .map(|arg| {
            let name = arg.spelling();
            Param {
                name: (!name.is_empty()).then_some(name),
                ty: lower_type(types, arg.ty()),
            }
        })
        .collect()
}

/// Is it a member operator that can be bound? (`arity`: the number of parameters excluding `this`)
fn bindable_operator(op: &str, arity: usize) -> bool {
    match arity {
        0 => matches!(op, "-" | "+" | "!" | "~"),
        1 => matches!(
            op,
            "+" | "-"
                | "*"
                | "/"
                | "%"
                | "^"
                | "&"
                | "|"
                | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "=="
                | "!="
                | "<"
                | ">"
                | "<="
                | ">="
        ),
        _ => false,
    }
}

/// `noexcept`, `noexcept(true)` and `throw()` guarantee not to throw.
/// Because the value of `noexcept(expression)` cannot be known, it is treated conservatively: no guarantee.
fn is_noexcept(spec: i32) -> bool {
    matches!(
        spec,
        cx::CXCursor_ExceptionSpecificationKind_BasicNoexcept
            | cx::CXCursor_ExceptionSpecificationKind_DynamicNone
            | cx::CXCursor_ExceptionSpecificationKind_NoThrow
    )
}

fn location(cursor: Cursor<'_>) -> SourceLocation {
    let clang::Location { file, line, column } = cursor.location();
    let file = Path::new(&file)
        .file_name()
        .map_or(file.clone(), |name| name.to_string_lossy().into_owned());
    SourceLocation { file, line, column }
}

fn lower_type(types: &Types<'_>, ty: clang::Type<'_>) -> QualType {
    if let Some(env) = &types.env {
        if let Some(substituted) = substitute(types, env, ty) {
            return substituted;
        }
    }
    let is_const = ty.is_const();
    let canonical = ty.canonical();
    let kind = canonical.kind();
    let lowered = match kind {
        cx::CXType_Void => Type::Void,
        cx::CXType_Bool => Type::Bool,
        _ if integer_info(kind).is_some() => {
            let (builtin, signed) = integer_info(kind).unwrap_or(("", false));
            let size_bytes = canonical.size_of().unwrap_or(0);
            Type::Integer {
                spelling: unqualified_spelling(ty),
                builtin: builtin.to_owned(),
                size_bytes,
                signed,
                fixed_width: is_fixed_width(ty, size_bytes),
            }
        }
        cx::CXType_Float | cx::CXType_Double => Type::Float {
            spelling: unqualified_spelling(ty),
            size_bytes: canonical.size_of().unwrap_or(0),
        },
        cx::CXType_Pointer => Type::Pointer {
            pointee: Box::new(lower_type(types, pointee_of(ty, canonical))),
        },
        cx::CXType_LValueReference => Type::LValueRef {
            referent: Box::new(lower_type(types, pointee_of(ty, canonical))),
        },
        cx::CXType_RValueReference => Type::RValueRef {
            referent: Box::new(lower_type(types, pointee_of(ty, canonical))),
        },
        cx::CXType_Record => lower_record(types, ty, canonical),
        cx::CXType_Enum => match scope_path(canonical.declaration()) {
            Some((namespace, name)) => Type::Enum {
                name,
                namespace,
                declared_in: types.declared_outside(canonical.declaration()),
            },
            None => unsupported_type(ty, REASON_NESTED),
        },
        _ => unsupported_type(ty, REASON_UNKNOWN_TYPE),
    };
    QualType {
        is_const,
        ty: lowered,
    }
}

/// Lowers class types: `std::string` and `std::unique_ptr<T>` are recognized specially.
///
/// Recognition is done not by the name text but by the declaration's namespace chain and
/// template arguments; inline namespaces such as libc++'s `std::__1` are ignored.
fn lower_record(types: &Types<'_>, ty: clang::Type<'_>, canonical: clang::Type<'_>) -> Type {
    let declaration = canonical.declaration();
    if declaration.kind() == cx::CXCursor_UnionDecl {
        return unsupported_type(ty, REASON_UNION);
    }
    let Some((namespace, name)) = scope_path(declaration) else {
        return unsupported_type(ty, REASON_NESTED);
    };
    let args = canonical.template_args();
    if namespace.first().is_some_and(|ns| ns == "std") {
        let is_char = |t: &clang::Type<'_>| {
            matches!(t.canonical().kind(), cx::CXType_Char_S | cx::CXType_Char_U)
        };
        return match (name.as_str(), args.as_slice()) {
            ("basic_string", [first, ..]) if namespace.len() == 1 && is_char(first) => Type::String,
            ("vector", [element, allocator]) if namespace.len() == 1 => {
                if is_std_decl(allocator.canonical().declaration(), "allocator") {
                    Type::Vector {
                        element: Box::new(lower_type(types, *element)),
                    }
                } else {
                    unsupported_type(ty, REASON_CUSTOM_ALLOCATOR)
                }
            }
            ("unique_ptr", [pointee, deleter]) if namespace.len() == 1 => {
                let deleter = deleter.canonical().declaration();
                if deleter.spelling() == "default_delete" {
                    Type::UniquePtr {
                        pointee: Box::new(lower_type(types, *pointee)),
                    }
                } else {
                    unsupported_type(ty, REASON_CUSTOM_DELETER)
                }
            }
            _ => unsupported_type(ty, REASON_STD_TYPE),
        };
    }
    if !args.is_empty() {
        return match types.instance_for_type(canonical) {
            Some(instance) => Type::Record {
                name: instance.rust_name.clone(),
                namespace: instance.namespace.clone(),
                declared_in: None,
            },
            None => Type::Unsupported {
                spelling: unqualified_spelling(ty),
                reason: format!(
                    "`{}` is a template that is not instantiated; add it to `[templates]` in \
                     `ferrobridge.toml`.",
                    canonical.spelling()
                ),
            },
        };
    }
    Type::Record {
        name,
        namespace,
        declared_in: types.declared_outside(declaration),
    }
}

/// Is the declaration directly in the `std` namespace (excluding inline namespaces) and with this name?
fn is_std_decl(declaration: Cursor<'_>, expected: &str) -> bool {
    scope_path(declaration)
        .is_some_and(|(namespace, name)| namespace == ["std"] && name == expected)
}

/// An `extern "C" { ... }` / `extern "C++" { ... }` block. libclang before 17 reports it as an
/// `UnexposedDecl`; it is recognized there by its first two tokens, so other unexposed declarations
/// (and `extern template ...` instantiations) are not mistaken for it.
fn is_linkage_spec(cursor: Cursor<'_>) -> bool {
    match cursor.kind() {
        cx::CXCursor_LinkageSpec => true,
        cx::CXCursor_UnexposedDecl => matches!(
            cursor.leading_tokens(2).as_slice(),
            [keyword, language] if keyword == "extern" && language.starts_with('"')
        ),
        _ => false,
    }
}

/// A declaration's namespace path and name. Inline namespaces are skipped. `None` if it is defined
/// inside a class or function, or if it is unnamed.
fn scope_path(declaration: Cursor<'_>) -> Option<(Vec<String>, String)> {
    let name = declaration.spelling();
    if name.is_empty() || declaration.is_anonymous() {
        return None;
    }
    let mut namespace = Vec::new();
    let mut parent = declaration.semantic_parent();
    for _ in 0..MAX_TYPEDEF_DEPTH * 4 {
        match parent.kind() {
            cx::CXCursor_TranslationUnit => {
                namespace.reverse();
                return Some((namespace, name));
            }
            cx::CXCursor_Namespace => {
                if !parent.is_inline_namespace() {
                    let segment = parent.spelling();
                    namespace.push(if segment.is_empty() {
                        ANONYMOUS_NAMESPACE.to_owned()
                    } else {
                        segment
                    });
                }
            }
            _ if is_linkage_spec(parent) => {}
            _ => return None,
        }
        parent = parent.semantic_parent();
    }
    None
}

/// If it is an integer type, its underlying C++ type name and whether it is signed; `None` otherwise.
/// `char16_t`, `wchar_t` and 128-bit types are deliberately left out.
fn integer_info(kind: cx::CXTypeKind) -> Option<(&'static str, bool)> {
    match kind {
        cx::CXType_Char_S => Some(("char", true)),
        cx::CXType_Char_U => Some(("char", false)),
        cx::CXType_SChar => Some(("signed char", true)),
        cx::CXType_UChar => Some(("unsigned char", false)),
        cx::CXType_Short => Some(("short", true)),
        cx::CXType_UShort => Some(("unsigned short", false)),
        cx::CXType_Int => Some(("int", true)),
        cx::CXType_UInt => Some(("unsigned int", false)),
        cx::CXType_Long => Some(("long", true)),
        cx::CXType_ULong => Some(("unsigned long", false)),
        cx::CXType_LongLong => Some(("long long", true)),
        cx::CXType_ULongLong => Some(("unsigned long long", false)),
        _ => None,
    }
}

/// The type a pointer/reference points to. For typedef'd pointers (e.g. `typedef int* IntPtr`),
/// if it cannot be taken from the sugared type, it is taken from the canonical type.
fn pointee_of<'tu>(ty: clang::Type<'tu>, canonical: clang::Type<'tu>) -> clang::Type<'tu> {
    let pointee = ty.pointee();
    if pointee.kind() == cx::CXType_Invalid {
        canonical.pointee()
    } else {
        pointee
    }
}

fn unsupported_type(ty: clang::Type<'_>, reason: &str) -> Type {
    Type::Unsupported {
        spelling: unqualified_spelling(ty),
        reason: reason.to_owned(),
    }
}

/// The type name as written in the source; `const` is removed because it is kept in a separate field.
fn unqualified_spelling(ty: clang::Type<'_>) -> String {
    let spelling = ty.spelling();
    spelling
        .strip_prefix("const ")
        .map_or(spelling.clone(), str::to_owned)
}

/// Does the type reach a fixed-width type such as `std::int32_t` at any step of the typedef
/// chain?
///
/// It is recognized by name, because libclang gives `std::int32_t` (`using ::int32_t;` inside std)
/// as an "unexposed" type without a declaration, and the chain cannot be followed there. The real size
/// is compared too, so as not to trust a user type with the same name but a different
/// size.
fn is_fixed_width(ty: clang::Type<'_>, size_bytes: u64) -> bool {
    let mut current = ty;
    for _ in 0..MAX_TYPEDEF_DEPTH {
        if let Some(bits) = fixed_width_bits(&unqualified_spelling(current)) {
            return u64::from(bits) == size_bytes * 8;
        }
        current = match current.kind() {
            cx::CXType_Elaborated => current.named(),
            cx::CXType_Typedef => current.declaration().typedef_underlying(),
            _ => return false,
        };
    }
    false
}

/// The bit width of a name such as `int32_t`, `std::uint8_t`; `None` if it is not such a name.
fn fixed_width_bits(spelling: &str) -> Option<u32> {
    let name = spelling.rsplit("::").next()?;
    let digits = name
        .strip_prefix('u')
        .unwrap_or(name)
        .strip_prefix("int")?
        .strip_suffix("_t")?;
    match digits {
        "8" => Some(8),
        "16" => Some(16),
        "32" => Some(32),
        "64" => Some(64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_fixed_width_names() {
        assert_eq!(fixed_width_bits("int32_t"), Some(32));
        assert_eq!(fixed_width_bits("std::uint8_t"), Some(8));
        assert_eq!(fixed_width_bits("::std::int64_t"), Some(64));
        assert_eq!(fixed_width_bits("int"), None);
        assert_eq!(fixed_width_bits("int128_t"), None);
        assert_eq!(fixed_width_bits("uintptr_t"), None);
        assert_eq!(fixed_width_bits("my_id"), None);
    }
}
