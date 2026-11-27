//! Rules that turn IR types into bridge types (the architecture's type table and the
//! safety rules). If a type cannot be expressed safely, it is rejected with a reason; nothing
//! is ever guessed. Information the header does not give (the ownership of pointers) comes only
//! from a `ferrobridge.toml` annotation.

use std::collections::HashMap;

use ferrobridge_ir::{ConstantValue, Destructor, QualType, Type};

use crate::config::{ParamAnnotation, ReturnAnnotation};
use crate::plan::{BridgeType, IntType, Pass, Returns, TypeRef};

const REASON_POINTER_PARAM: &str = "The owner and lifetime of a raw pointer cannot be told from the header; \
     it must be annotated as `borrowed` in `ferrobridge.toml` (`ferrobridge suggest` prints a draft).";
const REASON_POINTER_RETURN: &str = "The owner of a returned raw pointer cannot be told from the header; \
     it must be annotated in `ferrobridge.toml` (`ferrobridge suggest` prints a draft).";
const REASON_MUT_PRIMITIVE_REF: &str = "A non-const primitive reference (e.g. `double&`) \
     may be kept; it must be annotated as `borrowed` in `ferrobridge.toml`.";
const REASON_VALUE_CONTAINER: &str = "Value types are not bound inside `std::vector` or `std::unique_ptr` \
     yet.";
const REASON_CLASS_BY_VALUE: &str = "Taking a class by value needs copying/moving; not \
     supported yet (use `const T&`).";
const REASON_STRING_BY_VALUE: &str =
    "Taking `std::string` by value is not supported yet (use `const std::string&`).";
const REASON_VECTOR_BY_VALUE: &str =
    "Taking `std::vector` by value is not supported yet (use `const std::vector<T>&`).";
const REASON_RVALUE: &str = "Rvalue references (`T&&`) are not supported yet.";
const REASON_REF_RETURN_FREE: &str = "The lifetime of a reference returned by a free or static function \
     is unclear (it would have to be `'static`); to be handled in a later milestone.";
const REASON_BORROWED_RETURN_FREE: &str = "A `borrowed` return is supported only on methods \
     (the reference is tied to the object); on free/static functions the lifetime would have to be `'static`, \
     to be handled in a later milestone.";
const REASON_OWNED_REF: &str = "The ownership of a returned reference cannot be transferred; `owned` is valid only for pointer returns.";
const REASON_OWNED_CONST: &str = "A `const T*` return is not bound as `owned`: the Rust side \
     would be able to change the object.";
const REASON_OWNED_PRIMITIVE: &str = "`owned` is supported only for pointers to classes, `std::string` \
     and `std::vector` (cxx does not support `UniquePtr<primitive>`).";
const REASON_MUT_REF_FROM_CONST: &str =
    "A const method cannot return a mutable reference (Rust borrowing rules).";
const REASON_ABSTRACT_BY_VALUE: &str = "An abstract class cannot be returned by value.";
const REASON_VOID_PARAM: &str = "`void` cannot be a parameter.";
const REASON_VOID_POINTER: &str = "`void*` is an untyped pointer; it has no safe Rust counterpart.";
const REASON_C_STRING: &str = "`char*` C strings are not supported yet (use `std::string`).";
const REASON_POINTER_TO_POINTER: &str = "Pointers to pointers (`T**`) are not supported.";
const REASON_UNSUPPORTED_POINTEE: &str = "Pointers to this type are not supported yet.";
const REASON_UNSUPPORTED_REF: &str = "References to this type are not supported yet.";
const REASON_UNSUPPORTED_UNIQUE: &str = "`std::unique_ptr` is supported only for classes in this header, \
     `std::string` and `std::vector`.";
const REASON_VECTOR_BOOL: &str =
    "`std::vector<bool>` is a special bit-packed type in C++; cxx does not support it.";
const REASON_VECTOR_ELEMENT: &str = "A `std::vector` element can only be a fixed-width integer, \
     `float`/`double`, `std::string` or a class in this header.";
const REASON_FLOAT_SIZE: &str = "Only `float` and `double` are supported.";
const REASON_CONSTANT_VALUE: &str = "The constant's value does not fit the Rust type (`char` constants \
     are bound only between 0 and 127: the signedness of `c_char` depends on the platform).";
const REASON_SLICE_NOT_POINTER: &str = "`slice` can only be applied to pointer parameters.";
const REASON_SLICE_ELEMENT: &str = "`slice` can only be applied to pointers to arrays of numbers \
     (fixed-width integers, `float`, `double`).";
const REASON_SLICE_CHAR: &str = "`char` arrays are not bound as `slice` (their signedness depends on \
     the platform); the header should use `uint8_t` for byte arrays.";
const REASON_SLICE_LENGTH: &str =
    "The `slice` length parameter must be an integer passed by value.";

/// Underlying C++ integer types with the same width on every platform. `long` (32 bits on
/// Windows, 64 elsewhere) and `long long` (`std::int64_t` is `long` on some platforms) are left out.
const PORTABLE_BUILTINS: [&str; 6] = [
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned int",
];

/// The key of a class or enum: (namespace path, name).
pub(crate) type TypeKey = (Vec<String>, String);

/// A class whose name may appear in the bridge.
#[derive(Clone)]
pub(crate) struct KnownClass {
    pub(crate) type_ref: TypeRef,
    pub(crate) is_abstract: bool,
    pub(crate) is_polymorphic: bool,
    pub(crate) destructor: Destructor,
    /// Is it a value type (a `#[repr(C)]` struct in Rust)?
    pub(crate) value: bool,
}

impl KnownClass {
    /// Its type in the bridge: `Value` if it is a value type, the opaque `Class` otherwise.
    pub(crate) fn bridge_type(&self) -> BridgeType {
        if self.value {
            BridgeType::Value(self.type_ref.clone())
        } else {
            BridgeType::Class(self.type_ref.clone())
        }
    }
}

/// Classes and enums whose names may appear in the bridge: (namespace, name) → information.
#[derive(Default)]
pub(crate) struct Known {
    pub(crate) classes: HashMap<(Vec<String>, String), KnownClass>,
    pub(crate) enums: HashMap<(Vec<String>, String), TypeRef>,
}

/// The kind of rejection that an annotation could resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Need {
    /// A `borrowed` annotation on the parameter.
    Param,
    /// One of these annotations on the return.
    Return(Vec<ReturnAnnotation>),
}

/// Why a type was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rejection {
    pub(crate) reason: String,
    /// Could it be bound if an annotation were added?
    pub(crate) need: Option<Need>,
}

impl From<String> for Rejection {
    fn from(reason: String) -> Self {
        Rejection { reason, need: None }
    }
}

impl From<&str> for Rejection {
    fn from(reason: &str) -> Self {
        Rejection::from(reason.to_owned())
    }
}

/// A parameter that can be bound.
#[derive(Debug)]
pub(crate) struct ParamRule {
    pub(crate) ty: BridgeType,
    pub(crate) pass: Pass,
    /// Was the given annotation really used (if not, it is unnecessary)?
    pub(crate) applied: bool,
}

/// A return that can be bound.
#[derive(Debug)]
pub(crate) struct ReturnRule {
    pub(crate) returns: Returns,
    /// Was the given annotation really used?
    pub(crate) applied: bool,
}

/// Lowers a parameter type.
///
/// `slice` annotations are handled not here but at the function level (together with the length
/// parameter); only `borrowed` comes here.
pub(crate) fn param_type(
    qt: &QualType,
    known: &Known,
    annotation: Option<&ParamAnnotation>,
) -> Result<ParamRule, Rejection> {
    let borrowed = annotation == Some(&ParamAnnotation::Borrowed);
    let direct = |ty| ParamRule {
        ty,
        pass: Pass::Direct,
        applied: false,
    };
    match &qt.ty {
        Type::Void => Err(REASON_VOID_PARAM.into()),
        Type::Record {
            name,
            namespace,
            declared_in,
        } => {
            let class = class(namespace, name, declared_in.as_deref(), known)?;
            if class.value {
                Ok(direct(class.bridge_type()))
            } else {
                Err(REASON_CLASS_BY_VALUE.into())
            }
        }
        Type::String => Err(REASON_STRING_BY_VALUE.into()),
        Type::Vector { .. } => Err(REASON_VECTOR_BY_VALUE.into()),
        Type::UniquePtr { pointee } => Ok(direct(BridgeType::UniquePtr(Box::new(unique_inner(
            pointee, known,
        )?)))),
        Type::LValueRef { referent } if is_mut_primitive(referent) => {
            if !borrowed {
                return Err(Rejection {
                    reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                    need: Some(Need::Param),
                });
            }
            Ok(ParamRule {
                applied: true,
                ..direct(mutable_ref(non_ref(&referent.ty, known)?))
            })
        }
        Type::LValueRef { referent } => Ok(direct(ref_type(referent, known)?)),
        Type::Pointer { pointee } => {
            let inner = pointee_type(pointee, known)?;
            if !borrowed {
                return Err(Rejection {
                    reason: REASON_POINTER_PARAM.to_owned(),
                    need: Some(Need::Param),
                });
            }
            Ok(ParamRule {
                ty: BridgeType::Ref {
                    mutable: !pointee.is_const,
                    inner: Box::new(inner),
                },
                pass: Pass::AddressOf,
                applied: true,
            })
        }
        other => Ok(direct(non_ref(other, known)?)),
    }
}

/// Lowers a return type. `receiver_mutable`: for a method, whether the receiver is mutable;
/// `None` for a free/static function.
pub(crate) fn return_type(
    qt: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<ReturnRule, Rejection> {
    let plain = |returns| ReturnRule {
        returns,
        applied: false,
    };
    match &qt.ty {
        Type::Void => Ok(plain(Returns::Unit)),
        Type::Record {
            name,
            namespace,
            declared_in,
        } => {
            let class = class(namespace, name, declared_in.as_deref(), known)?;
            if class.value {
                return Ok(plain(Returns::Value(class.bridge_type())));
            }
            if class.is_abstract {
                return Err(REASON_ABSTRACT_BY_VALUE.into());
            }
            deletable(&class, false)?;
            Ok(plain(Returns::BoxedClass(class.type_ref)))
        }
        Type::String => Ok(plain(Returns::OwnedString)),
        Type::Vector { element } => {
            Ok(plain(Returns::OwnedVector(vector_element(element, known)?)))
        }
        Type::UniquePtr { pointee } => Ok(plain(Returns::Value(BridgeType::UniquePtr(Box::new(
            unique_inner(pointee, known)?,
        ))))),
        Type::LValueRef { referent } => ref_return(referent, known, receiver_mutable, annotation)
            .map(|(ty, applied)| ReturnRule {
                returns: Returns::Value(ty),
                applied,
            }),
        Type::Pointer { pointee } => pointer_return(pointee, known, receiver_mutable, annotation),
        other => Ok(plain(Returns::Value(non_ref(other, known)?))),
    }
}

/// A `T&` returned by a method. A primitive `T&` needs an annotation; the others follow the type table.
fn ref_return(
    referent: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<(BridgeType, bool), Rejection> {
    let Some(receiver_mutable) = receiver_mutable else {
        return Err(REASON_REF_RETURN_FREE.into());
    };
    if annotation == Some(ReturnAnnotation::Owned) {
        return Err(REASON_OWNED_REF.into());
    }
    if is_mut_primitive(referent) {
        if annotation != Some(ReturnAnnotation::Borrowed) {
            return Err(Rejection {
                reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                need: Some(Need::Return(vec![ReturnAnnotation::Borrowed])),
            });
        }
        if !receiver_mutable {
            return Err(REASON_MUT_REF_FROM_CONST.into());
        }
        return Ok((mutable_ref(non_ref(&referent.ty, known)?), true));
    }
    let reference = ref_type(referent, known)?;
    if matches!(reference, BridgeType::Ref { mutable: true, .. }) && !receiver_mutable {
        return Err(REASON_MUT_REF_FROM_CONST.into());
    }
    Ok((reference, false))
}

/// A returned `T*`: not bound without an annotation.
fn pointer_return(
    pointee: &QualType,
    known: &Known,
    receiver_mutable: Option<bool>,
    annotation: Option<ReturnAnnotation>,
) -> Result<ReturnRule, Rejection> {
    let inner = pointee_type(pointee, known)?;
    let ownable = matches!(
        inner,
        BridgeType::Class(_) | BridgeType::String | BridgeType::Vector(_)
    ) && !pointee.is_const;
    match annotation {
        None => {
            let mut options = Vec::new();
            if ownable {
                options.push(ReturnAnnotation::Owned);
            }
            if receiver_mutable.is_some() {
                options.push(ReturnAnnotation::Borrowed);
            }
            if options.is_empty() {
                // A free/static function and a pointer that cannot be owned: no annotation
                // helps.
                let owned = if pointee.is_const {
                    REASON_OWNED_CONST
                } else {
                    REASON_OWNED_PRIMITIVE
                };
                return Err(format!("{owned} {REASON_BORROWED_RETURN_FREE}").into());
            }
            Err(Rejection {
                reason: REASON_POINTER_RETURN.to_owned(),
                need: Some(Need::Return(options)),
            })
        }
        Some(ReturnAnnotation::Owned) => {
            if pointee.is_const {
                return Err(REASON_OWNED_CONST.into());
            }
            match &inner {
                BridgeType::Class(_) => {
                    if let Type::Record {
                        name,
                        namespace,
                        declared_in,
                    } = &pointee.ty
                    {
                        deletable(
                            &class(namespace, name, declared_in.as_deref(), known)?,
                            true,
                        )?;
                    }
                }
                BridgeType::String | BridgeType::Vector(_) => {}
                _ => return Err(REASON_OWNED_PRIMITIVE.into()),
            }
            Ok(ReturnRule {
                returns: Returns::OwnedPointer(inner),
                applied: true,
            })
        }
        Some(ReturnAnnotation::Borrowed) => {
            let Some(receiver_mutable) = receiver_mutable else {
                return Err(REASON_BORROWED_RETURN_FREE.into());
            };
            Ok(ReturnRule {
                returns: Returns::BorrowedPointer {
                    mutable: !pointee.is_const && receiver_mutable,
                    inner,
                },
                applied: true,
            })
        }
    }
}

/// The Rust type of a value-type field: a number, `bool`, `c_char` or a class marked as a value
/// type (`value_classes`). `None` if it does not fit.
pub(crate) fn value_field_type(
    ty: &Type,
    known: &Known,
    is_value: &dyn Fn(&TypeKey) -> bool,
) -> Option<BridgeType> {
    match ty {
        Type::Bool | Type::Integer { .. } | Type::Float { .. } => match non_ref(ty, known).ok()? {
            t @ (BridgeType::Bool
            | BridgeType::Char
            | BridgeType::Int(_)
            | BridgeType::F32
            | BridgeType::F64) => Some(t),
            _ => None,
        },
        Type::Record {
            name, namespace, ..
        } => {
            let key = (namespace.clone(), name.clone());
            let class = known.classes.get(&key)?;
            is_value(&key).then(|| BridgeType::Value(class.type_ref.clone()))
        }
        _ => None,
    }
}

/// The Rust type of a constant; rejected if the value does not match the type or does not fit.
pub(crate) fn constant_type(ty: &Type, value: ConstantValue) -> Result<BridgeType, String> {
    let bridge = non_ref(ty, &Known::default())?;
    let as_int = |value: ConstantValue| match value {
        ConstantValue::Signed(v) => Some(i128::from(v)),
        ConstantValue::Unsigned(v) => Some(i128::from(v)),
        _ => None,
    };
    let fits = match (&bridge, value) {
        (BridgeType::Bool, ConstantValue::Bool(_))
        | (BridgeType::F32 | BridgeType::F64, ConstantValue::Float(_)) => true,
        (BridgeType::Int(int), value) => {
            let (min, max) = int.range();
            as_int(value).is_some_and(|v| (min..=max).contains(&v))
        }
        // `c_char` is signed on one platform and unsigned on another: only values that are the same
        // on both.
        (BridgeType::Char, value) => as_int(value).is_some_and(|v| (0..=127).contains(&v)),
        _ => false,
    };
    if fits {
        Ok(bridge)
    } else {
        Err(REASON_CONSTANT_VALUE.to_owned())
    }
}

/// A parameter annotated with `slice`: the element type and whether the array is mutable.
pub(crate) fn slice_element(qt: &QualType, known: &Known) -> Result<(BridgeType, bool), String> {
    let Type::Pointer { pointee } = &qt.ty else {
        return Err(REASON_SLICE_NOT_POINTER.to_owned());
    };
    match &pointee.ty {
        Type::Integer { .. } | Type::Float { .. } => match non_ref(&pointee.ty, known)? {
            ty @ (BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64) => {
                Ok((ty, !pointee.is_const))
            }
            _ => Err(REASON_SLICE_CHAR.to_owned()),
        },
        _ => Err(REASON_SLICE_ELEMENT.to_owned()),
    }
}

/// The underlying C++ type of a `slice` length parameter (e.g. `"unsigned short"`). Because it is
/// not visible to Rust, platform-dependent types such as `size_t` are accepted too.
pub(crate) fn slice_length(qt: &QualType) -> Result<String, String> {
    match &qt.ty {
        Type::Integer { builtin, .. } => Ok(builtin.clone()),
        _ => Err(REASON_SLICE_LENGTH.to_owned()),
    }
}

/// Is it a pointer that could be an array of numbers (for a `slice` suggestion)?
pub(crate) fn could_be_slice(qt: &QualType, known: &Known) -> bool {
    slice_element(qt, known).is_ok()
}

/// Is it a non-`const` primitive or enum (like `double&`)?
fn is_mut_primitive(referent: &QualType) -> bool {
    !referent.is_const
        && matches!(
            referent.ty,
            Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. }
        )
}

fn mutable_ref(inner: BridgeType) -> BridgeType {
    BridgeType::Ref {
        mutable: true,
        inner: Box::new(inner),
    }
}

/// The types a pointer can point to (that an annotation can turn into a reference).
fn pointee_type(pointee: &QualType, known: &Known) -> Result<BridgeType, Rejection> {
    match &pointee.ty {
        Type::Record {
            name,
            namespace,
            declared_in,
        } => Ok(class(namespace, name, declared_in.as_deref(), known)?.bridge_type()),
        Type::String => Ok(BridgeType::String),
        Type::Vector { element } => Ok(BridgeType::Vector(Box::new(vector_element(
            element, known,
        )?))),
        Type::Integer {
            builtin,
            fixed_width: false,
            ..
        } if builtin == "char" => Err(REASON_C_STRING.into()),
        Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. } => {
            Ok(non_ref(&pointee.ty, known)?)
        }
        Type::Void => Err(REASON_VOID_POINTER.into()),
        Type::Pointer { .. } => Err(REASON_POINTER_TO_POINTER.into()),
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}").into()),
        _ => Err(REASON_UNSUPPORTED_POINTEE.into()),
    }
}

/// Types that can be passed by value, and the ones rejected for certain.
fn non_ref(ty: &Type, known: &Known) -> Result<BridgeType, String> {
    match ty {
        Type::Bool => Ok(BridgeType::Bool),
        Type::Integer {
            spelling,
            builtin,
            size_bytes,
            signed,
            fixed_width,
        } => {
            if builtin == "char" && !fixed_width {
                return Ok(BridgeType::Char);
            }
            if !fixed_width && !PORTABLE_BUILTINS.contains(&builtin.as_str()) {
                return Err(format!(
                    "`{spelling}` ({builtin}) may have a different width on different platforms; \
                     use a fixed-width type such as `std::int64_t`."
                ));
            }
            IntType::from_layout(*size_bytes, *signed)
                .map(BridgeType::Int)
                .ok_or_else(|| format!("no suitable integer type for `{spelling}`."))
        }
        Type::Float { size_bytes, .. } => match size_bytes {
            4 => Ok(BridgeType::F32),
            8 => Ok(BridgeType::F64),
            _ => Err(REASON_FLOAT_SIZE.to_owned()),
        },
        Type::Enum {
            name,
            namespace,
            declared_in,
        } => known
            .enums
            .get(&(namespace.clone(), name.clone()))
            .cloned()
            .map(BridgeType::Enum)
            .ok_or_else(|| not_defined(namespace, name, declared_in.as_deref())),
        Type::Pointer { .. } => Err(REASON_POINTER_PARAM.to_owned()),
        Type::RValueRef { .. } => Err(REASON_RVALUE.to_owned()),
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}")),
        Type::Void
        | Type::Record { .. }
        | Type::String
        | Type::UniquePtr { .. }
        | Type::Vector { .. }
        | Type::LValueRef { .. } => Err(REASON_UNSUPPORTED_REF.to_owned()),
    }
}

/// The type table for `T&` / `const T&` (except non-const primitives; they need an annotation).
fn ref_type(referent: &QualType, known: &Known) -> Result<BridgeType, Rejection> {
    let mutable = !referent.is_const;
    let inner = match &referent.ty {
        Type::Record {
            name,
            namespace,
            declared_in,
        } => class(namespace, name, declared_in.as_deref(), known)?.bridge_type(),
        Type::String => BridgeType::String,
        Type::Vector { element } => BridgeType::Vector(Box::new(vector_element(element, known)?)),
        Type::Bool | Type::Integer { .. } | Type::Float { .. } | Type::Enum { .. } => {
            if mutable {
                return Err(Rejection {
                    reason: REASON_MUT_PRIMITIVE_REF.to_owned(),
                    need: Some(Need::Param),
                });
            }
            non_ref(&referent.ty, known)?
        }
        Type::Pointer { .. } => return Err(REASON_POINTER_PARAM.into()),
        Type::Unsupported { spelling, reason } => {
            return Err(format!("`{spelling}`: {reason}").into());
        }
        _ => return Err(REASON_UNSUPPORTED_REF.into()),
    };
    Ok(BridgeType::Ref {
        mutable,
        inner: Box::new(inner),
    })
}

/// The element type of a `std::vector`: what cxx accepts for `CxxVector<T>`.
fn vector_element(element: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &element.ty {
        Type::Bool => Err(REASON_VECTOR_BOOL.to_owned()),
        Type::Integer { .. } | Type::Float { .. } => match non_ref(&element.ty, known)? {
            ty @ (BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64) => Ok(ty),
            _ => Err(REASON_VECTOR_ELEMENT.to_owned()),
        },
        Type::String => Ok(BridgeType::String),
        Type::Record {
            name,
            namespace,
            declared_in,
        } => {
            let class = class(namespace, name, declared_in.as_deref(), known)?;
            if class.value {
                return Err(REASON_VALUE_CONTAINER.to_owned());
            }
            // The destructor of `std::vector<T>` itself deletes the elements.
            deletable(&class, false)?;
            Ok(BridgeType::Class(class.type_ref))
        }
        Type::Unsupported { spelling, reason } => Err(format!("`{spelling}`: {reason}")),
        _ => Err(REASON_VECTOR_ELEMENT.to_owned()),
    }
}

fn unique_inner(pointee: &QualType, known: &Known) -> Result<BridgeType, String> {
    match &pointee.ty {
        Type::Record {
            name,
            namespace,
            declared_in,
        } => {
            let class = class(namespace, name, declared_in.as_deref(), known)?;
            if class.value {
                return Err(REASON_VALUE_CONTAINER.to_owned());
            }
            deletable(&class, false)?;
            Ok(BridgeType::Class(class.type_ref))
        }
        Type::String => Ok(BridgeType::String),
        Type::Vector { element } => Ok(BridgeType::Vector(Box::new(vector_element(
            element, known,
        )?))),
        _ => Err(REASON_UNSUPPORTED_UNIQUE.to_owned()),
    }
}

/// Is it safe for the Rust side to delete the object with `delete` (when a `UniquePtr` is dropped)?
///
/// `through_base_pointer`: the object comes from a pointer and its real type may be a
/// subclass. Then the destructor of a polymorphic class must be virtual; otherwise `delete` is UB.
pub(crate) fn deletable(class: &KnownClass, through_base_pointer: bool) -> Result<(), String> {
    let path = class.type_ref.cpp_path.trim_start_matches("::");
    match class.destructor {
        Destructor::NotPublic => Err(format!(
            "the destructor of `{path}` is not public; the Rust side cannot delete the object, no `UniquePtr` \
             is generated."
        )),
        Destructor::Deleted => Err(format!(
            "the destructor of `{path}` is deleted (`= delete`); no `UniquePtr` is generated."
        )),
        Destructor::Public { is_virtual: false }
            if through_base_pointer && class.is_polymorphic =>
        {
            Err(format!(
                "`{path}` is polymorphic but its destructor is not virtual: if the returned pointer points to a \
                 subclass, `delete` is undefined behavior, no `UniquePtr` is generated."
            ))
        }
        Destructor::Public { .. } => Ok(()),
    }
}

fn class(
    namespace: &[String],
    name: &str,
    declared_in: Option<&str>,
    known: &Known,
) -> Result<KnownClass, String> {
    known
        .classes
        .get(&(namespace.to_vec(), name.to_owned()))
        .cloned()
        .ok_or_else(|| not_defined(namespace, name, declared_in))
}

/// An unknown type: if it is in a header outside the bridge, which one and what to do.
fn not_defined(namespace: &[String], name: &str, declared_in: Option<&str>) -> String {
    let mut path = namespace.to_vec();
    path.push(name.to_owned());
    let path = path.join("::");
    match declared_in {
        Some(header) => format!(
            "`{path}` is declared in `{header}`, and this header is not part of the bridge; add the header \
             to the bridge too (`ferrobridge generate a.h {header} ...`)."
        ),
        None => {
            format!("`{path}` is not defined as a bindable type in the bridge's headers.")
        }
    }
}

/// The integer type for the underlying type of an enum.
pub(crate) fn enum_repr(underlying: &Type) -> Result<IntType, String> {
    match non_ref(underlying, &Known::default())? {
        BridgeType::Int(int) => Ok(int),
        BridgeType::Char => Err("`char`-based enums are not supported.".to_owned()),
        _ => Err("The enum's underlying type is not an integer.".to_owned()),
    }
}

/// A readable C++ spelling of a type (used in suggestions and warnings).
pub(crate) fn spell(qt: &QualType) -> String {
    let base = match &qt.ty {
        Type::Void => "void".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Integer { spelling, .. } | Type::Float { spelling, .. } => spelling.clone(),
        Type::Pointer { pointee } => {
            let star = format!("{}*", spell(pointee));
            return if qt.is_const {
                format!("{star} const")
            } else {
                star
            };
        }
        Type::LValueRef { referent } => return format!("{}&", spell(referent)),
        Type::RValueRef { referent } => return format!("{}&&", spell(referent)),
        Type::Record {
            name, namespace, ..
        }
        | Type::Enum {
            name, namespace, ..
        } => {
            let mut path = namespace.clone();
            path.push(name.clone());
            path.join("::")
        }
        Type::String => "std::string".to_owned(),
        Type::UniquePtr { pointee } => format!("std::unique_ptr<{}>", spell(pointee)),
        Type::Vector { element } => format!("std::vector<{}>", spell(element)),
        Type::Unsupported { spelling, .. } => spelling.clone(),
    };
    if qt.is_const {
        format!("const {base}")
    } else {
        base
    }
}
