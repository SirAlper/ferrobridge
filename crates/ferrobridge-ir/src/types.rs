//! IR counterparts of C++ types.

use serde::{Deserialize, Serialize};

/// A type together with its `const` qualifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualType {
    /// Is the type `const`-qualified?
    #[serde(rename = "const")]
    pub is_const: bool,
    /// The unqualified type.
    #[serde(rename = "type")]
    pub ty: Type,
}

/// A C++ type.
///
/// Unsupported types are not dropped; they are kept as [`Type::Unsupported`] with a reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    /// `void`
    Void,
    /// `bool`
    Bool,
    /// Character and integer types (`char`, `int`, `std::int32_t`, `size_t`...).
    Integer {
        /// The type name as written in the source (the typedef name is kept).
        spelling: String,
        /// The underlying C++ type after typedefs are resolved (e.g. `"unsigned long"` for
        /// `size_t`). Portability across platforms is judged by this.
        builtin: String,
        /// Size in bytes on the target platform.
        size_bytes: u64,
        /// Is it signed?
        signed: bool,
        /// Is it a type such as `std::int32_t` with the same width on every platform?
        fixed_width: bool,
    },
    /// Floating-point types (`float`, `double`).
    Float {
        /// The type name as written in the source.
        spelling: String,
        /// Size in bytes on the target platform.
        size_bytes: u64,
    },
    /// A raw pointer (`T*`). No ownership information here; it is decided in the analysis layer.
    Pointer {
        /// The pointed-to type.
        pointee: Box<QualType>,
    },
    /// An lvalue reference (`T&`).
    #[serde(rename = "lvalue_ref")]
    LValueRef {
        /// The referenced type.
        referent: Box<QualType>,
    },
    /// An rvalue reference (`T&&`).
    #[serde(rename = "rvalue_ref")]
    RValueRef {
        /// The referenced type.
        referent: Box<QualType>,
    },
    /// A class or struct (e.g. `geo::Vehicle`).
    Record {
        /// The class's name.
        name: String,
        /// The namespace path, outermost first.
        namespace: Vec<String>,
        /// If declared outside the headers that were read, the name of the file declaring it (e.g.
        /// `rotations.h`); `None` if it is in one of the bridge's headers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        declared_in: Option<String>,
    },
    /// An enum (e.g. `geo::Mode`).
    Enum {
        /// The enum's name.
        name: String,
        /// The namespace path, outermost first.
        namespace: Vec<String>,
        /// If declared outside the headers that were read, the name of the file declaring it (e.g.
        /// `rotations.h`); `None` if it is in one of the bridge's headers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        declared_in: Option<String>,
    },
    /// `std::string`
    String,
    /// `std::unique_ptr<T>` (with its default deleter).
    UniquePtr {
        /// The owned type.
        pointee: Box<QualType>,
    },
    /// `std::vector<T>` (with its default allocator).
    Vector {
        /// The element type.
        element: Box<QualType>,
    },
    /// A type not supported in this version.
    Unsupported {
        /// The type as written in the source.
        spelling: String,
        /// Why it is not supported.
        reason: String,
    },
}
