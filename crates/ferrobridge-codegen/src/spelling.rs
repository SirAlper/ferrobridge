//! The Rust (cxx) and C++ spellings of bridge types. The two must correspond one to one:
//! cxx expects `std::int32_t` in C++ for `i32` in Rust, `const T&` for `&T`, and so on.

use ferrobridge_analysis::{BridgeType, FunctionPlan, Returns};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

use crate::CodegenError;

/// Turns a name into a Rust identifier; an error if it is a keyword or invalid.
pub(crate) fn ident(name: &str) -> Result<Ident, CodegenError> {
    syn::parse_str::<Ident>(name).map_err(|_| CodegenError::InvalidIdentifier(name.to_owned()))
}

/// The Rust type inside the cxx bridge.
pub(crate) fn rust_type(ty: &BridgeType) -> Result<TokenStream, CodegenError> {
    rust_type_with(ty, None)
}

/// Are these the opaque types for which cxx wants `Pin` (class, `CxxString`, `CxxVector`)? Primitives
/// and enums get a plain `&mut`.
fn is_opaque(ty: &BridgeType) -> bool {
    matches!(
        ty,
        BridgeType::Class(_) | BridgeType::String | BridgeType::Vector(_)
    )
}

/// The Rust type with `lifetime` (e.g. `'a`) added to the outermost reference.
pub(crate) fn rust_type_with(
    ty: &BridgeType,
    lifetime: Option<&TokenStream>,
) -> Result<TokenStream, CodegenError> {
    Ok(match ty {
        BridgeType::Bool => quote!(bool),
        BridgeType::Char => quote!(c_char),
        BridgeType::Int(int) => {
            let int = ident(int.rust_name())?;
            quote!(#int)
        }
        BridgeType::F32 => quote!(f32),
        BridgeType::F64 => quote!(f64),
        BridgeType::Class(r) | BridgeType::Enum(r) | BridgeType::Value(r) => {
            let name = ident(&r.rust_name)?;
            quote!(#name)
        }
        BridgeType::String => quote!(CxxString),
        BridgeType::UniquePtr(inner) => {
            let inner = rust_type(inner)?;
            quote!(UniquePtr<#inner>)
        }
        BridgeType::Vector(inner) => {
            let inner = rust_type(inner)?;
            quote!(CxxVector<#inner>)
        }
        BridgeType::Slice { mutable, element } => {
            let element = rust_type(element)?;
            if *mutable {
                quote!(&#lifetime mut [#element])
            } else {
                quote!(&#lifetime [#element])
            }
        }
        BridgeType::Ref {
            mutable: false,
            inner,
        } => {
            let inner_tokens = rust_type(inner)?;
            quote!(&#lifetime #inner_tokens)
        }
        BridgeType::Ref {
            mutable: true,
            inner,
        } => {
            let inner_tokens = rust_type(inner)?;
            if is_opaque(inner) {
                quote!(Pin<&#lifetime mut #inner_tokens>)
            } else {
                quote!(&#lifetime mut #inner_tokens)
            }
        }
    })
}

/// Is the return a reference (its lifetime must be tied to the receiver)?
pub(crate) fn returns_reference(plan: &FunctionPlan) -> bool {
    matches!(
        plan.returns,
        Returns::BorrowedPointer { .. } | Returns::Value(BridgeType::Ref { .. })
    )
}

/// The return type in the cxx bridge (without the `Result` wrapping); `None` for `void`.
/// If `lifetime` is given, it is added to the returned reference.
pub(crate) fn rust_return(
    plan: &FunctionPlan,
    lifetime: Option<&TokenStream>,
) -> Result<Option<TokenStream>, CodegenError> {
    Ok(match &plan.returns {
        Returns::Unit => None,
        Returns::Value(ty) => Some(rust_type_with(ty, lifetime)?),
        Returns::BoxedClass(class) => {
            let name = ident(&class.rust_name)?;
            Some(quote!(UniquePtr<#name>))
        }
        Returns::OwnedString => Some(quote!(UniquePtr<CxxString>)),
        Returns::OwnedVector(element) => {
            let element = rust_type(element)?;
            Some(quote!(UniquePtr<CxxVector<#element>>))
        }
        Returns::OwnedPointer(inner) => {
            let inner = rust_type(inner)?;
            Some(quote!(UniquePtr<#inner>))
        }
        Returns::BorrowedPointer { mutable, inner } => Some(rust_type_with(
            &BridgeType::Ref {
                mutable: *mutable,
                inner: Box::new(inner.clone()),
            },
            lifetime,
        )?),
    })
}

/// The C++ type in the shim signature.
pub(crate) fn cpp_type(ty: &BridgeType) -> String {
    match ty {
        BridgeType::Bool => "bool".to_owned(),
        BridgeType::Char => "char".to_owned(),
        BridgeType::Int(int) => int.cpp_name().to_owned(),
        BridgeType::F32 => "float".to_owned(),
        BridgeType::F64 => "double".to_owned(),
        BridgeType::Class(r) | BridgeType::Enum(r) | BridgeType::Value(r) => r.cpp_path.clone(),
        BridgeType::String => "::std::string".to_owned(),
        BridgeType::UniquePtr(inner) => format!("::std::unique_ptr<{}>", cpp_type(inner)),
        BridgeType::Vector(inner) => format!("::std::vector<{}>", cpp_type(inner)),
        BridgeType::Slice {
            mutable: false,
            element,
        } => format!("::rust::Slice<const {}>", cpp_type(element)),
        BridgeType::Slice {
            mutable: true,
            element,
        } => format!("::rust::Slice<{}>", cpp_type(element)),
        BridgeType::Ref {
            mutable: false,
            inner,
        } => format!("const {}&", cpp_type(inner)),
        BridgeType::Ref {
            mutable: true,
            inner,
        } => format!("{}&", cpp_type(inner)),
    }
}

/// The C++ return type of the shim.
pub(crate) fn cpp_return(plan: &FunctionPlan) -> String {
    match &plan.returns {
        Returns::Unit => "void".to_owned(),
        Returns::Value(ty) => cpp_type(ty),
        Returns::BoxedClass(class) => format!("::std::unique_ptr<{}>", class.cpp_path),
        Returns::OwnedString => "::std::unique_ptr<::std::string>".to_owned(),
        Returns::OwnedVector(element) => {
            format!("::std::unique_ptr<::std::vector<{}>>", cpp_type(element))
        }
        Returns::OwnedPointer(inner) => format!("::std::unique_ptr<{}>", cpp_type(inner)),
        Returns::BorrowedPointer { mutable, inner } => cpp_type(&BridgeType::Ref {
            mutable: *mutable,
            inner: Box::new(inner.clone()),
        }),
    }
}
