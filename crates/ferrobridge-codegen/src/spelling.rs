//! Bridge tiplerinin Rust (cxx) ve C++ yazımları. İkisi birbirine birebir karşılık gelmeli:
//! cxx, Rust'taki `i32`'yi C++'ta `std::int32_t` bekler, `&T`'yi `const T&`, ve böyle devam.

use ferrobridge_analysis::{BridgeType, FunctionPlan, Returns};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

use crate::CodegenError;

/// Bir adı Rust tanımlayıcısına çevirir; anahtar kelime veya geçersizse hata.
pub(crate) fn ident(name: &str) -> Result<Ident, CodegenError> {
    syn::parse_str::<Ident>(name).map_err(|_| CodegenError::InvalidIdentifier(name.to_owned()))
}

/// cxx bridge içindeki Rust tipi.
pub(crate) fn rust_type(ty: &BridgeType) -> Result<TokenStream, CodegenError> {
    Ok(match ty {
        BridgeType::Bool => quote!(bool),
        BridgeType::Char => quote!(c_char),
        BridgeType::Int(int) => {
            let int = ident(int.rust_name())?;
            quote!(#int)
        }
        BridgeType::F32 => quote!(f32),
        BridgeType::F64 => quote!(f64),
        BridgeType::Class(r) | BridgeType::Enum(r) => {
            let name = ident(&r.rust_name)?;
            quote!(#name)
        }
        BridgeType::String => quote!(CxxString),
        BridgeType::UniquePtr(inner) => {
            let inner = rust_type(inner)?;
            quote!(UniquePtr<#inner>)
        }
        BridgeType::Ref {
            mutable: false,
            inner,
        } => {
            let inner = rust_type(inner)?;
            quote!(&#inner)
        }
        BridgeType::Ref {
            mutable: true,
            inner,
        } => {
            let inner = rust_type(inner)?;
            quote!(Pin<&mut #inner>)
        }
    })
}

/// cxx bridge'deki dönüş tipi (`Result` sarmalaması hariç); `void` için `None`.
pub(crate) fn rust_return(plan: &FunctionPlan) -> Result<Option<TokenStream>, CodegenError> {
    Ok(match &plan.returns {
        Returns::Unit => None,
        Returns::Value(ty) => Some(rust_type(ty)?),
        Returns::BoxedClass(class) => {
            let name = ident(&class.rust_name)?;
            Some(quote!(UniquePtr<#name>))
        }
        Returns::OwnedString => Some(quote!(UniquePtr<CxxString>)),
    })
}

/// Shim imzasındaki C++ tipi.
pub(crate) fn cpp_type(ty: &BridgeType) -> String {
    match ty {
        BridgeType::Bool => "bool".to_owned(),
        BridgeType::Char => "char".to_owned(),
        BridgeType::Int(int) => int.cpp_name().to_owned(),
        BridgeType::F32 => "float".to_owned(),
        BridgeType::F64 => "double".to_owned(),
        BridgeType::Class(r) | BridgeType::Enum(r) => r.cpp_path.clone(),
        BridgeType::String => "::std::string".to_owned(),
        BridgeType::UniquePtr(inner) => format!("::std::unique_ptr<{}>", cpp_type(inner)),
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

/// Shim'in C++ dönüş tipi.
pub(crate) fn cpp_return(plan: &FunctionPlan) -> String {
    match &plan.returns {
        Returns::Unit => "void".to_owned(),
        Returns::Value(ty) => cpp_type(ty),
        Returns::BoxedClass(class) => format!("::std::unique_ptr<{}>", class.cpp_path),
        Returns::OwnedString => "::std::unique_ptr<::std::string>".to_owned(),
    }
}
