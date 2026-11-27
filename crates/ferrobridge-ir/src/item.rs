//! Top-level items found in a header.

use serde::{Deserialize, Serialize};

use crate::{QualType, Type};

/// An item read from a header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// A free function (not a class member).
    Function(Function),
    /// A class or struct definition.
    Class(Class),
    /// An enum definition.
    Enum(Enum),
    /// A constant of numeric or `bool` type whose value is known exactly at compile time.
    Constant(Constant),
    /// A construct not supported in this version. Recorded with a reason instead of being skipped silently.
    Unsupported(Unsupported),
}

/// A free function declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    /// The function's name (without namespace).
    pub name: String,
    /// The namespace path, outermost first (`["geo", "detail"]` for `geo::detail::f`).
    pub namespace: Vec<String>,
    /// The parameters, in order.
    pub params: Vec<Param>,
    /// The return type.
    pub return_type: QualType,
    /// Does the function guarantee not to throw (`noexcept`, `throw()`)?
    pub noexcept: bool,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// A class or struct definition. Only public members are taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Class {
    /// The class's name.
    pub name: String,
    /// The namespace path, outermost first.
    pub namespace: Vec<String>,
    /// Can it not be created directly because it has a pure virtual method?
    pub is_abstract: bool,
    /// Is it polymorphic: does it or one of its base classes (whatever its access) declare a
    /// virtual function? A pointer to a polymorphic class may actually point to a subclass;
    /// deleting such an object with `delete` needs a virtual destructor.
    pub is_polymorphic: bool,
    /// The destructor's access and virtuality. If not declared explicitly, the implicit destructor is recorded.
    pub destructor: Destructor,
    /// Explicitly declared public constructors (except `= delete` ones).
    pub constructors: Vec<Constructor>,
    /// Public methods (including static ones).
    pub methods: Vec<Method>,
    /// Public members not supported in this version (operators, nested types...).
    pub unsupported_members: Vec<Unsupported>,
    /// Non-static data members (whatever their access), in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    /// The memory layout; `None` if libclang could not compute it (e.g. an incomplete type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
    /// Does the user declare a copy/move constructor, an assignment operator or a destructor
    /// (except `= default` ones)? Such a class may not be trivially copyable.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_copy_or_move: bool,
    /// If this is a template instantiation (`[templates]`), which one; `name` is then the Rust name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_of: Option<TemplateInstance>,
    /// Where the definition is in the source.
    pub location: SourceLocation,
}

/// A non-static data member of a class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// The field's name.
    pub name: String,
    /// Its type; bit fields and arrays are `Type::Unsupported`.
    #[serde(rename = "type")]
    pub ty: QualType,
    /// Is it public?
    pub public: bool,
    /// The offset in bits from the start of the class; `None` if it could not be computed.
    pub offset_bits: Option<u64>,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// The memory layout of a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    /// `sizeof`
    pub size_bytes: u64,
    /// `alignof`
    pub align_bytes: u64,
}

/// An instantiation of a class template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInstance {
    /// The template's name (e.g. `Vector3`).
    pub template: String,
    /// The full C++ spelling of the instantiation, with its namespace (e.g. `Vector3<float>`, `geo::Box<int>`).
    pub spelling: String,
}

/// A class's destructor. Because `delete` runs when a `UniquePtr<T>` is dropped in Rust, whether
/// it is safe for every object whose ownership passes to Rust is decided by looking at this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destructor {
    /// Public and not deleted. An implicit (not explicitly declared) destructor goes here too.
    Public {
        /// Is it virtual (declared `virtual` itself, or one of a base class's is virtual)?
        is_virtual: bool,
    },
    /// `private` or `protected`: it cannot be deleted from outside the class.
    NotPublic,
    /// `= delete`
    Deleted,
}

/// A constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constructor {
    /// The parameters, in order.
    pub params: Vec<Param>,
    /// Is there a guarantee not to throw?
    pub noexcept: bool,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// A member function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
    /// The method's name.
    pub name: String,
    /// The parameters, in order (excluding `this`).
    pub params: Vec<Param>,
    /// The return type.
    pub return_type: QualType,
    /// Is it a `const` method (does not change the object)?
    pub is_const: bool,
    /// Is it a static method?
    pub is_static: bool,
    /// Is it a virtual method?
    pub is_virtual: bool,
    /// Is there a guarantee not to throw?
    pub noexcept: bool,
    /// For an operator method, the operator itself (`"+"`, `"+="`, `"=="`...); `name` is then
    /// like `operator+`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// An enum definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enum {
    /// The enum's name.
    pub name: String,
    /// The namespace path, outermost first.
    pub namespace: Vec<String>,
    /// Is it an `enum class`?
    pub scoped: bool,
    /// The underlying integer type.
    pub underlying: Type,
    /// The values, in source order.
    pub variants: Vec<EnumVariant>,
    /// Where the definition is in the source.
    pub location: SourceLocation,
}

/// An enum value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    /// The value's name.
    pub name: String,
    /// Its numeric value.
    pub value: i64,
}

/// A `const`/`constexpr` variable; its value was computed from the header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constant {
    /// The constant's name.
    pub name: String,
    /// The namespace path, outermost first.
    pub namespace: Vec<String>,
    /// The constant's type: [`Type::Integer`], [`Type::Float`] or [`Type::Bool`].
    #[serde(rename = "type")]
    pub ty: Type,
    /// The computed value.
    pub value: ConstantValue,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// The value of a constant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ConstantValue {
    /// The value of a constant of signed integer type.
    Signed(i64),
    /// The value of a constant of unsigned integer type.
    Unsigned(u64),
    /// A floating-point number. Always finite: constants with NaN or infinite values do not enter the IR. For
    /// `float` constants the value is an `f64` that converts to `f32` without loss.
    Float(f64),
    /// `bool`.
    Bool(bool),
}

// `f64` is not `Eq`, because NaN is not equal to itself. Because the values in the IR are always
// finite (see `ConstantValue::Float`), equality here is an equivalence relation.
impl Eq for ConstantValue {}

/// A function parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// The parameter's name; `None` if unnamed in the header.
    pub name: Option<String>,
    /// The parameter's type.
    #[serde(rename = "type")]
    pub ty: QualType,
}

/// An unsupported construct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unsupported {
    /// The kind of construct (e.g. `class`, `enum`, `function_template`).
    pub construct: String,
    /// Its name with its namespace (e.g. `geo::Vehicle`); empty if unnamed.
    pub name: String,
    /// Why it is not supported and when support is planned.
    pub reason: String,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// A location in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// The file name relative to the header that was read (contains no absolute path).
    pub file: String,
    /// The line number, starting at 1.
    pub line: u32,
    /// The column number, starting at 1.
    pub column: u32,
}
