//! The output of analysis: the binding plan consumed by code generation.

use std::fmt;

pub use ferrobridge_ir::ConstantValue;
use ferrobridge_ir::{EnumVariant, SourceLocation};

use crate::config::ReturnAnnotation;

/// What is bound for a header, and how.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BridgePlan {
    /// The bound enums.
    pub enums: Vec<EnumPlan>,
    /// The bound classes (as opaque types) and their members.
    pub classes: Vec<ClassPlan>,
    /// The bound free functions.
    pub functions: Vec<FunctionPlan>,
    /// Constants generated as Rust `const`.
    pub constants: Vec<ConstantPlan>,
    /// Items that could not be bound, and why.
    pub skipped: Vec<Skipped>,
    /// Things that are bound but the user should know about.
    pub notes: Vec<Skipped>,
    /// The promises in `ferrobridge.toml` the generated code relies on (points of trust).
    pub assumptions: Vec<Assumption>,
    /// Places that could be bound if an annotation were added (`ferrobridge suggest` prints them).
    pub suggestions: Vec<Suggestion>,
    /// Unnecessary annotations: not errors, but the file may hold a wrong belief.
    pub annotation_warnings: Vec<AnnotationIssue>,
}

/// An annotation the generated code relies on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    /// The item's full C++ name (e.g. `geo::make_widget`).
    pub name: String,
    /// What it applies to (e.g. "return", "parameter `out`").
    pub subject: String,
    /// The annotation's value (`owned`, `borrowed`).
    pub annotation: String,
    /// The promise the user makes with this annotation.
    pub promise: String,
    /// Where the annotation is (`ferrobridge.toml:3`; the file name only).
    pub origin: String,
}

/// A place that could not be bound because it needs an annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The key in `ferrobridge.toml` (the full C++ name).
    pub key: String,
    /// A readable signature (e.g. `geo::Widget::attach(geo::Widget* target)`).
    pub signature: String,
    /// The place that needs an annotation.
    pub target: SuggestionTarget,
    /// The C++ type to annotate (e.g. `geo::Widget*`).
    pub cpp_type: String,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// The target of a suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionTarget {
    /// The return value; with the annotations that can be chosen.
    Return {
        /// The annotations that make sense in this context.
        options: Vec<ReturnAnnotation>,
    },
    /// A parameter (its name; `argN` if unnamed). `borrowed` is always an option.
    Param {
        /// The parameter's name.
        name: String,
        /// If the pointer can be an array of numbers, the candidate length parameters for `{ slice = "..." }`
        /// (integer parameters of the same function); empty otherwise.
        slice_lengths: Vec<String>,
    },
}

/// A mismatch between the annotation file and the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationIssue {
    /// Where the annotation is (`ferrobridge.toml:3`).
    pub origin: String,
    /// The problem and its fix.
    pub message: String,
}

impl fmt::Display for AnnotationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

/// An item that could not be bound, or a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The item's full C++ name (e.g. `geo::Vehicle::total_km`).
    pub name: String,
    /// The reason and (if any) when support is planned.
    pub reason: String,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// A class or enum whose name appears in the bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    /// The Rust name inside the bridge (with a namespace prefix on clashes: `PhysicsVehicle`).
    pub rust_name: String,
    /// The fully qualified C++ path (e.g. `::geo::Vehicle`).
    pub cpp_path: String,
}

/// A bound class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassPlan {
    /// The type's names.
    pub type_ref: TypeRef,
    /// Its name in C++.
    pub cpp_name: String,
    /// The C++ namespace path.
    pub namespace: Vec<String>,
    /// Constructors bound as factory functions.
    pub constructors: Vec<FunctionPlan>,
    /// The bound methods (including static ones).
    pub methods: Vec<FunctionPlan>,
    /// If it is a value type, the layout of the `#[repr(C)]` struct in Rust; `None` for an opaque class.
    pub value: Option<ValueLayout>,
    /// If it is a template instantiation, its C++ spelling (e.g. `geo::Vec3<float>`); the shim defines
    /// an alias (`using`) for it with the Rust name.
    pub instance_spelling: Option<String>,
}

/// The Rust layout of a value-type class (ADR-0010). The C++ side checks it with
/// `static_assert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueLayout {
    /// `sizeof`
    pub size_bytes: u64,
    /// `alignof`
    pub align_bytes: u64,
    /// The fields, in C++ order.
    pub fields: Vec<FieldPlan>,
}

/// A field of a value type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPlan {
    /// Its name in Rust (keywords avoided).
    pub rust_name: String,
    /// Its name in C++.
    pub cpp_name: String,
    /// Its type: a number, `bool`, `c_char` or another value type.
    pub ty: BridgeType,
    /// The offset in bytes.
    pub offset_bytes: u64,
}

/// A bound enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumPlan {
    /// The type's names.
    pub type_ref: TypeRef,
    /// Its name in C++.
    pub cpp_name: String,
    /// The C++ namespace path.
    pub namespace: Vec<String>,
    /// The underlying integer type.
    pub repr: IntType,
    /// The values.
    pub variants: Vec<EnumVariant>,
}

/// A C++ constant generated as a Rust `const`. The value is written at generation time; the C++
/// side is not called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantPlan {
    /// Its name in Rust.
    pub rust_name: String,
    /// Its name in C++.
    pub cpp_name: String,
    /// The C++ namespace path.
    pub namespace: Vec<String>,
    /// Tipi: [`BridgeType::Bool`], [`BridgeType::Char`], [`BridgeType::Int`],
    /// [`BridgeType::F32`] ya da [`BridgeType::F64`].
    pub ty: BridgeType,
    /// Its value; analysis verified that it fits the type.
    pub value: ConstantValue,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

/// Integer types that are the same in Rust and C++ on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntType {
    /// `i8` / `std::int8_t`
    I8,
    /// `u8` / `std::uint8_t`
    U8,
    /// `i16` / `std::int16_t`
    I16,
    /// `u16` / `std::uint16_t`
    U16,
    /// `i32` / `std::int32_t`
    I32,
    /// `u32` / `std::uint32_t`
    U32,
    /// `i64` / `std::int64_t`
    I64,
    /// `u64` / `std::uint64_t`
    U64,
}

impl IntType {
    /// The type from the size and signedness; `None` for an unsupported size.
    pub fn from_layout(size_bytes: u64, signed: bool) -> Option<Self> {
        Some(match (size_bytes, signed) {
            (1, true) => Self::I8,
            (1, false) => Self::U8,
            (2, true) => Self::I16,
            (2, false) => Self::U16,
            (4, true) => Self::I32,
            (4, false) => Self::U32,
            (8, true) => Self::I64,
            (8, false) => Self::U64,
            _ => return None,
        })
    }

    /// Its name in Rust (`"i32"`).
    pub fn rust_name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
        }
    }

    /// The smallest and largest values it can take.
    pub fn range(self) -> (i128, i128) {
        match self {
            Self::I8 => (i8::MIN.into(), i8::MAX.into()),
            Self::U8 => (0, u8::MAX.into()),
            Self::I16 => (i16::MIN.into(), i16::MAX.into()),
            Self::U16 => (0, u16::MAX.into()),
            Self::I32 => (i32::MIN.into(), i32::MAX.into()),
            Self::U32 => (0, u32::MAX.into()),
            Self::I64 => (i64::MIN.into(), i64::MAX.into()),
            Self::U64 => (0, u64::MAX.into()),
        }
    }

    /// The C++ name cxx uses (`"::std::int32_t"`).
    pub fn cpp_name(self) -> &'static str {
        match self {
            Self::I8 => "::std::int8_t",
            Self::U8 => "::std::uint8_t",
            Self::I16 => "::std::int16_t",
            Self::U16 => "::std::uint16_t",
            Self::I32 => "::std::int32_t",
            Self::U32 => "::std::uint32_t",
            Self::I64 => "::std::int64_t",
            Self::U64 => "::std::uint64_t",
        }
    }
}

/// A type on the bridge boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeType {
    /// `bool`
    Bool,
    /// C++ `char` / Rust `c_char`
    Char,
    /// A fixed-width integer.
    Int(IntType),
    /// `f32` / `float`
    F32,
    /// `f64` / `double`
    F64,
    /// An opaque class (used only behind a reference or a `UniquePtr`).
    Class(TypeRef),
    /// A value-type class: a `#[repr(C)]` struct in Rust, passed by value (ADR-0010).
    Value(TypeRef),
    /// A shared enum.
    Enum(TypeRef),
    /// `CxxString` / `std::string` (only behind a reference or a `UniquePtr`).
    String,
    /// `UniquePtr<T>` / `std::unique_ptr<T>`
    UniquePtr(Box<BridgeType>),
    /// `CxxVector<T>` / `std::vector<T>` (only behind a reference or a `UniquePtr`).
    Vector(Box<BridgeType>),
    /// `&[T]` / `&mut [T]` (`rust::Slice<const T>` / `rust::Slice<T>`); `T` is a numeric type.
    /// Only on pointer parameters annotated with `slice`.
    Slice {
        /// Is it `&mut [T]`?
        mutable: bool,
        /// The element type.
        element: Box<BridgeType>,
    },
    /// `&T` (`mutable: false`). If `mutable: true`, `Pin<&mut T>` for opaque types (class, `std::string`,
    /// `std::vector`), `&mut T` for primitives and enums.
    Ref {
        /// Is it a mutable reference?
        mutable: bool,
        /// The referenced type.
        inner: Box<BridgeType>,
    },
}

/// The return of a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Returns {
    /// `void`
    Unit,
    /// Returned as is.
    Value(BridgeType),
    /// A class returned by value in C++; the shim wraps it in a `std::unique_ptr`, Rust gets a `UniquePtr<T>`.
    BoxedClass(TypeRef),
    /// A `std::string` returned by value in C++; the shim wraps it, Rust gets a `UniquePtr<CxxString>`.
    OwnedString,
    /// A `std::vector<T>` returned by value in C++ (the element type); the shim wraps it, Rust gets a
    /// `UniquePtr<CxxVector<T>>`.
    OwnedVector(BridgeType),
    /// A `T*` annotated `"owned"` (the pointed-to type): the shim wraps it in a `std::unique_ptr`, Rust
    /// gets a `UniquePtr<T>`. A null becomes an empty `UniquePtr`.
    OwnedPointer(BridgeType),
    /// A `T*` annotated `"borrowed"` (methods only): the shim checks for null and turns it into a
    /// reference; a null becomes an exception → `Err`. The reference's lifetime is tied to the receiver.
    BorrowedPointer {
        /// Is it a mutable reference?
        mutable: bool,
        /// The pointed-to type.
        inner: BridgeType,
    },
}

/// The C++ item that is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callee {
    /// A free function.
    Free {
        /// The C++ namespace path.
        namespace: Vec<String>,
        /// The C++ name.
        cpp_name: String,
    },
    /// A member function.
    Method {
        /// The class that owns it.
        class: TypeRef,
        /// The C++ name.
        cpp_name: String,
        /// `false`: `self: &T` (const method); `true`: `self: Pin<&mut T>`.
        receiver_mutable: bool,
    },
    /// A static member function (called through a shim).
    Static {
        /// The class that owns it.
        class: TypeRef,
        /// The C++ name.
        cpp_name: String,
    },
    /// A constructor (called through the factory function in the shim).
    Constructor {
        /// The class that is created.
        class: TypeRef,
    },
}

/// A bound function, method or constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionPlan {
    /// The Rust name inside the bridge.
    pub rust_name: String,
    /// The C++ item that is called.
    pub callee: Callee,
    /// The parameters (excluding `this`).
    pub params: Vec<ParamPlan>,
    /// The return.
    pub returns: Returns,
    /// `true` if it is not `noexcept`: it returns `Result<T>` on the Rust side.
    pub fallible: bool,
    /// If it is an operator method, which one (code generation adds a `std::ops` impl for value types).
    pub operator: Option<OperatorKind>,
    /// Length parameters removed from the Rust signature (`slice` annotations); the shim fills them
    /// from the slice's length.
    pub lengths: Vec<LengthParam>,
    /// Where the declaration is in the source.
    pub location: SourceLocation,
}

impl FunctionPlan {
    /// Does it need a C++ shim because it cannot be called directly with cxx?
    pub fn needs_shim(&self) -> bool {
        // cxx cannot bind operator names; the shim calls `fb_self.operator+(v)`.
        self.operator.is_some()
            || matches!(
                self.callee,
                Callee::Static { .. } | Callee::Constructor { .. }
            )
            || matches!(
                self.returns,
                Returns::BoxedClass(_)
                    | Returns::OwnedString
                    | Returns::OwnedVector(_)
                    | Returns::OwnedPointer(_)
                    | Returns::BorrowedPointer { .. }
            )
            || self
                .params
                .iter()
                .any(|p| matches!(p.pass, Pass::AddressOf | Pass::Slice))
    }

    /// The number of parameters of the C++ function (including lengths invisible to Rust).
    pub fn cpp_arity(&self) -> usize {
        self.params.len() + self.lengths.len()
    }
}

/// A member operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorKind {
    /// Binary `+`
    Add,
    /// Binary `-`
    Sub,
    /// Binary `*`
    Mul,
    /// Binary `/`
    Div,
    /// `%`
    Rem,
    /// Unary `-`
    Neg,
    /// Unary `+`
    Pos,
    /// `!`
    Not,
    /// `~`
    BitNot,
    /// Binary `^`
    BitXor,
    /// Binary `&`
    BitAnd,
    /// Binary `|`
    BitOr,
    /// `+=`
    AddAssign,
    /// `-=`
    SubAssign,
    /// `*=`
    MulAssign,
    /// `/=`
    DivAssign,
    /// `%=`
    RemAssign,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
}

impl OperatorKind {
    /// From the C++ operator and the number of parameters (excluding `this`).
    pub fn from_cpp(op: &str, arity: usize) -> Option<Self> {
        Some(match (op, arity) {
            ("+", 1) => Self::Add,
            ("-", 1) => Self::Sub,
            ("*", 1) => Self::Mul,
            ("/", 1) => Self::Div,
            ("%", 1) => Self::Rem,
            ("-", 0) => Self::Neg,
            ("+", 0) => Self::Pos,
            ("!", 0) => Self::Not,
            ("~", 0) => Self::BitNot,
            ("^", 1) => Self::BitXor,
            ("&", 1) => Self::BitAnd,
            ("|", 1) => Self::BitOr,
            ("+=", 1) => Self::AddAssign,
            ("-=", 1) => Self::SubAssign,
            ("*=", 1) => Self::MulAssign,
            ("/=", 1) => Self::DivAssign,
            ("%=", 1) => Self::RemAssign,
            ("==", 1) => Self::Eq,
            ("!=", 1) => Self::Ne,
            ("<", 1) => Self::Lt,
            (">", 1) => Self::Gt,
            ("<=", 1) => Self::Le,
            (">=", 1) => Self::Ge,
            _ => return None,
        })
    }

    /// The name of the named method in Rust (e.g. `op_add`); the `std::ops` impl calls it.
    pub fn method_name(self) -> &'static str {
        match self {
            Self::Add => "op_add",
            Self::Sub => "op_sub",
            Self::Mul => "op_mul",
            Self::Div => "op_div",
            Self::Rem => "op_rem",
            Self::Neg => "op_neg",
            Self::Pos => "op_pos",
            Self::Not => "op_not",
            Self::BitNot => "op_bit_not",
            Self::BitXor => "op_bit_xor",
            Self::BitAnd => "op_bit_and",
            Self::BitOr => "op_bit_or",
            Self::AddAssign => "op_add_assign",
            Self::SubAssign => "op_sub_assign",
            Self::MulAssign => "op_mul_assign",
            Self::DivAssign => "op_div_assign",
            Self::RemAssign => "op_rem_assign",
            Self::Eq => "op_eq",
            Self::Ne => "op_ne",
            Self::Lt => "op_lt",
            Self::Gt => "op_gt",
            Self::Le => "op_le",
            Self::Ge => "op_ge",
        }
    }
}

/// A C++ parameter missing from the Rust signature, filled from the slice's length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LengthParam {
    /// Its position in the C++ parameter list (starting at 0).
    pub position: usize,
    /// The position in [`FunctionPlan::params`] of the slice whose length is used.
    pub slice: usize,
    /// The underlying C++ integer type (`"unsigned short"`); the shim checks whether the length fits
    /// this type.
    pub cpp_type: String,
}

/// A parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamPlan {
    /// Its name in Rust (keywords avoided, `argN` if unnamed).
    pub rust_name: String,
    /// Its type.
    pub ty: BridgeType,
    /// How it is passed to the C++ function.
    pub pass: Pass,
}

/// How a parameter is passed to the C++ function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// As is (cxx can call it directly).
    Direct,
    /// C++ expects a pointer, Rust gives a reference: the shim takes the address and passes it (`&p`).
    AddressOf,
    /// C++ expects a pointer + a length, Rust gives a slice: the shim passes `s.data()` and (see
    /// [`LengthParam`]) `s.size()`.
    Slice,
}
