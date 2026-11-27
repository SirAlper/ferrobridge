//! Güvenli katman: cxx'in `ffi` modülünün üstünde idiomatik Rust API'si (ADR-0007).
//!
//! - Gerçek Rust `enum`'ları (`TryFrom<ffi::E>` / `From<E> for ffi::E`).
//! - Güvenli metotlar cxx'in opak tipine inherent `impl` olarak yazılır. Böylece sahip olunan,
//!   ödünç (`&ffi::T`) ve `Pin<&mut ffi::T>` nesnelerde aynı API çalışır.
//! - Bridge'de `UniquePtr<T>` olarak geçen her sınıf için sahip olan bir sarmalayıcı: null
//!   olmayan bir `UniquePtr`, `Deref`, değiştiren metotlar için `&mut self` iletici.
//! - Bütün hatalar `ferrobridge::Error`; `&str` / `String`, `&[T]` / `Vec<T>` dönüşümleri.
//!
//! Üretilen koddaki bütün yollar tam niteliklidir (`::std::string::String`): C++'tan gelen bir
//! ad (`String`, `Result`, `Pin`) üretilen kodu bozamaz.

use std::collections::BTreeSet;

use ferrobridge_analysis::{
    BridgePlan, BridgeType, Callee, ClassPlan, ConstantPlan, ConstantValue, EnumPlan, FunctionPlan,
    OperatorKind, Returns, TypeRef,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::bridge::raw_method_name;
use crate::spelling::ident;
use crate::{CodegenError, ShimFn};

/// Sarmalayıcının kendi metotları; C++'tan gelen bir adla çakışırsa C++ adına `_` eklenir.
const WRAPPER_HELPERS: [&str; 3] = ["from_unique_ptr", "into_inner", "pin_mut"];

/// Güvenli katmanın bütün öğeleri.
pub(crate) fn generate(
    plan: &BridgePlan,
    shims: &[ShimFn<'_>],
    shim_namespace: &str,
) -> Result<TokenStream, CodegenError> {
    let wrapped = wrapped_classes(plan);
    let ctx = Ctx {
        shims,
        wrapped: &wrapped,
    };
    let mut items = Vec::new();
    for constant in &plan.constants {
        items.push(safe_constant(constant)?);
    }
    for enumeration in &plan.enums {
        items.push(safe_enum(enumeration)?);
    }
    for class in &plan.classes {
        if class.value.is_some() {
            items.push(value_struct(class, shim_namespace)?);
        }
        items.push(class_items(&ctx, class)?);
        if class.value.is_some() {
            items.push(operator_impls(&ctx, class)?);
        }
    }
    for function in &plan.functions {
        let item = cpp_item(&function.callee);
        let name = ident(&function.rust_name)?;
        let Body {
            inputs,
            output,
            statements,
            doc,
        } = Body::new(&ctx, function, &item, None)?;
        let allows = item_allows();
        items.push(quote! {
            #doc
            #allows
            pub fn #name(#(#inputs),*) #output {
                #(#statements)*
            }
        });
    }
    Ok(quote!(#(#items)*))
}

/// Üretim boyunca gereken bilgiler.
struct Ctx<'a, 'p> {
    shims: &'a [ShimFn<'p>],
    /// Sahip olan sarmalayıcısı üretilen sınıfların Rust adları.
    wrapped: &'a BTreeSet<String>,
}

impl Ctx<'_, '_> {
    fn shim_name(&self, plan: &FunctionPlan) -> Option<&str> {
        self.shims
            .iter()
            .find(|shim| std::ptr::eq(shim.plan, plan))
            .map(|shim| shim.name.as_str())
    }

    fn is_wrapped(&self, class: &TypeRef) -> bool {
        self.wrapped.contains(&class.rust_name)
    }
}

/// Bridge'de `UniquePtr<T>` olarak geçen sınıflar. cxx `UniquePtr<T>` için gereken C++
/// kodunu yalnızca bridge'de geçen tipler için üretir; analiz bu sınıfların silinebildiğini
/// (public destructor) zaten doğruladı.
fn wrapped_classes(plan: &BridgePlan) -> BTreeSet<String> {
    let mut wrapped = BTreeSet::new();
    let all = plan
        .functions
        .iter()
        .chain(plan.classes.iter().flat_map(|c| &c.constructors))
        .chain(plan.classes.iter().flat_map(|c| &c.methods));
    for function in all {
        for param in &function.params {
            if let BridgeType::UniquePtr(inner) = &param.ty {
                if let BridgeType::Class(class) = inner.as_ref() {
                    wrapped.insert(class.rust_name.clone());
                }
            }
        }
        match &function.returns {
            Returns::BoxedClass(class) | Returns::OwnedPointer(BridgeType::Class(class)) => {
                wrapped.insert(class.rust_name.clone());
            }
            Returns::Value(BridgeType::UniquePtr(inner)) => {
                if let BridgeType::Class(class) = inner.as_ref() {
                    wrapped.insert(class.rust_name.clone());
                }
            }
            _ => {}
        }
    }
    wrapped
}

/// Hata mesajlarındaki tam C++ adı (`own::Widget::adopt`; constructor için `own::Widget::Widget`).
pub(crate) fn cpp_item(callee: &Callee) -> String {
    let name = match callee {
        Callee::Method {
            class, cpp_name, ..
        }
        | Callee::Static { class, cpp_name } => format!("{}::{cpp_name}", class.cpp_path),
        Callee::Constructor { class } => {
            let short = class.cpp_path.rsplit("::").next().unwrap_or_default();
            format!("{}::{short}", class.cpp_path)
        }
        Callee::Free {
            namespace,
            cpp_name,
        } => {
            let mut path = namespace.clone();
            path.push(cpp_name.clone());
            path.join("::")
        }
    };
    name.trim_start_matches("::").to_owned()
}

fn doc_line(text: &str) -> TokenStream {
    quote!(#[doc = #text])
}

/// C++ adları olduğu gibi korunur (`getValue`, `vec3_t`); kullanıcının crate'inde lint
/// uyarısına dönüşmesinler. `should_implement_trait`: C++ metodu `next`/`add` gibi bir trait
/// metoduyla aynı adı taşıyabilir. `too_many_arguments`: C++ imzası olduğu gibi gelir.
///
/// `#[rustfmt::skip]`: dosya commit'lenir (ADR-0004); `cargo fmt` onu prettyplease'ten farklı
/// biçimlendirip üreticinin çıktısıyla uyumsuz hale getirmesin.
fn item_allows() -> TokenStream {
    quote!(#[rustfmt::skip] #[allow(
        non_snake_case,
        non_camel_case_types,
        clippy::should_implement_trait,
        clippy::too_many_arguments
    )])
}

// ---- Sabitler ---------------------------------------------------------------------------------

/// `pub const WGS84_A: f64 = 6378137.0;`. Değer header'dan üretim anında hesaplandı
/// (`ferrobridge-parser`); C++ tarafı çağrılmaz.
fn safe_constant(constant: &ConstantPlan) -> Result<TokenStream, CodegenError> {
    let name = ident(&constant.rust_name)?;
    let ty = raw_type(&constant.ty)?;
    let mut path = constant.namespace.clone();
    path.push(constant.cpp_name.clone());
    let doc = doc_line(&format!(" C++: `{}`", path.join("::")));
    let value = constant_literal(constant)?;
    Ok(quote! {
        #doc
        #[rustfmt::skip]
        #[allow(non_upper_case_globals, clippy::approx_constant)]
        pub const #name: #ty = #value;
    })
}

fn constant_literal(constant: &ConstantPlan) -> Result<TokenStream, CodegenError> {
    let negative = |magnitude: Literal, negative: bool| {
        if negative {
            quote!(-#magnitude)
        } else {
            quote!(#magnitude)
        }
    };
    Ok(match (constant.value, &constant.ty) {
        (ConstantValue::Bool(true), _) => quote!(true),
        (ConstantValue::Bool(false), _) => quote!(false),
        (ConstantValue::Signed(v), _) => negative(Literal::u64_unsuffixed(v.unsigned_abs()), v < 0),
        (ConstantValue::Unsigned(v), _) => negative(Literal::u64_unsuffixed(v), false),
        // `f32` sabit en kısa `f32` yazımıyla (`9.80665`): değer `f32`'ye kayıpsız döner.
        (ConstantValue::Float(v), BridgeType::F32) => {
            #[allow(clippy::cast_possible_truncation)]
            let narrow = v as f32;
            negative(Literal::f32_unsuffixed(narrow.abs()), v.is_sign_negative())
        }
        (ConstantValue::Float(v), _) if v.is_finite() => {
            negative(Literal::f64_unsuffixed(v.abs()), v.is_sign_negative())
        }
        (ConstantValue::Float(_), _) => {
            return Err(CodegenError::Internal(format!(
                "sonlu olmayan sabit: {}",
                constant.cpp_name
            )));
        }
    })
}

// ---- Enum'lar ---------------------------------------------------------------------------------

fn safe_enum(enumeration: &EnumPlan) -> Result<TokenStream, CodegenError> {
    let name = ident(&enumeration.type_ref.rust_name)?;
    let cpp = enumeration
        .type_ref
        .cpp_path
        .trim_start_matches("::")
        .to_owned();
    let doc = doc_line(&format!(" C++: `{cpp}`"));
    let mut variants = Vec::new();
    let mut to_raw = Vec::new();
    let mut from_raw = Vec::new();
    let mut seen_values = BTreeSet::new();
    let mut duplicate_values = false;
    for variant in &enumeration.variants {
        let v = ident(&variant.name)?;
        let variant_doc = doc_line(&format!(
            " C++: `{cpp}::{}` (= {})",
            variant.name, variant.value
        ));
        variants.push(quote!(#variant_doc #v));
        to_raw.push(quote!(#name::#v => ffi::#name::#v));
        from_raw.push(quote!(ffi::#name::#v => ::core::result::Result::Ok(#name::#v)));
        duplicate_values |= !seen_values.insert(variant.value);
    }
    // Aynı değeri paylaşan C++ değerleri (`A = 1, B = 1`): C++'tan gelen değer ilkine çevrilir.
    let allow_duplicates = duplicate_values.then(|| quote!(#[allow(unreachable_patterns)]));
    let allows = item_allows();
    Ok(quote! {
        #doc
        #allows
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum #name {
            #(#variants),*
        }

        #[rustfmt::skip]
        impl ::core::convert::From<#name> for ffi::#name {
            fn from(value: #name) -> Self {
                match value {
                    #(#to_raw),*
                }
            }
        }

        #[rustfmt::skip]
        impl ::core::convert::TryFrom<ffi::#name> for #name {
            type Error = ::ferrobridge::Error;

            #allow_duplicates
            fn try_from(value: ffi::#name) -> ::ferrobridge::Result<Self> {
                match value {
                    #(#from_raw,)*
                    other => ::core::result::Result::Err(
                        ::ferrobridge::__private::unknown_enum(#cpp, other.repr),
                    ),
                }
            }
        }
    })
}

// ---- Değer tipleri ----------------------------------------------------------------------------

/// Değer tipi sınıf: `#[repr(C)]` struct, cxx için `ExternType` (`Trivial`) ve Rust tarafında
/// düzen kontrolleri (C++ tarafındakiler `shim.cc`'de). `unsafe impl`'in `// SAFETY:` yorumu
/// biçimlendirmeden sonra eklenir (`bridge::with_safety_comments`).
fn value_struct(class: &ClassPlan, shim_namespace: &str) -> Result<TokenStream, CodegenError> {
    let Some(layout) = &class.value else {
        return Ok(quote!());
    };
    let name = ident(&class.type_ref.rust_name)?;
    let cpp = class.type_ref.cpp_path.trim_start_matches("::").to_owned();
    // cxx'in gördüğü C++ adı: örneklemeler shim namespace'indeki takma adla.
    let type_id = if class.instance_spelling.is_some() {
        format!("{shim_namespace}::{}", class.type_ref.rust_name)
    } else {
        let mut path = class.namespace.clone();
        path.push(class.cpp_name.clone());
        path.join("::")
    };
    let doc = doc_line(&format!(
        " C++: `{cpp}`. Değer tipi: alanlar doğrudan okunup yazılır, değerle kopyalanır."
    ));
    let mut fields = Vec::with_capacity(layout.fields.len());
    let mut offsets = Vec::with_capacity(layout.fields.len());
    for field in &layout.fields {
        let field_name = ident(&field.rust_name)?;
        let ty = raw_type(&field.ty)?;
        let field_doc = doc_line(&format!(" C++: `{}`", field.cpp_name));
        fields.push(quote!(#field_doc pub #field_name: #ty));
        let offset = Literal::u64_unsuffixed(field.offset_bytes);
        offsets.push(quote!(assert!(::core::mem::offset_of!(#name, #field_name) == #offset);));
    }
    let size = Literal::u64_unsuffixed(layout.size_bytes);
    let align = Literal::u64_unsuffixed(layout.align_bytes);
    let allows = item_allows();
    Ok(quote! {
        #doc
        #allows
        #[repr(C)]
        #[derive(Debug, Clone, Copy)]
        pub struct #name {
            #(#fields),*
        }

        #[rustfmt::skip]
        unsafe impl ::cxx::ExternType for #name {
            type Id = ::cxx::type_id!(#type_id);
            type Kind = ::cxx::kind::Trivial;
        }

        #[rustfmt::skip]
        const _: () = {
            assert!(::core::mem::size_of::<#name>() == #size);
            assert!(::core::mem::align_of::<#name>() == #align);
            #(#offsets)*
        };
    })
}

/// Değer tipinin operatör metotları için `std::ops` impl'leri. Yalnızca hata dönmeyen
/// (`Result`'suz) operatörler: trait metotları `Result` döndüremez. Adlı metotlar (`op_add`)
/// her durumda vardır; impl onları çağırır. Karşılaştırmalar (`<`...) adlı metot kalır:
/// `PartialOrd`'un anlamı C++'takiyle örtüşmeyebilir.
fn operator_impls(ctx: &Ctx<'_, '_>, class: &ClassPlan) -> Result<TokenStream, CodegenError> {
    let this = ident(&class.type_ref.rust_name)?;
    let mut seen = BTreeSet::new();
    let mut impls = Vec::new();
    for method in &class.methods {
        let Some(kind) = method.operator else {
            continue;
        };
        if method.fallible {
            continue;
        }
        let Callee::Method {
            receiver_mutable, ..
        } = &method.callee
        else {
            continue;
        };
        let item = cpp_item(&method.callee);
        let output = safe_return(ctx, &method.returns, &item)?;
        if output.fallible {
            continue;
        }
        let call = ident(&method.rust_name)?;
        // Sağ taraf: (Rust tipi, adlı metoda geçen ifade).
        let rhs = match method.params.as_slice() {
            [] => None,
            [param] => match &param.ty {
                BridgeType::Value(r) => {
                    let ty = ident(&r.rust_name)?;
                    Some((quote!(#ty), quote!(rhs)))
                }
                BridgeType::Ref {
                    mutable: false,
                    inner,
                } => match inner.as_ref() {
                    BridgeType::Value(r) => {
                        let ty = ident(&r.rust_name)?;
                        Some((quote!(#ty), quote!(&rhs)))
                    }
                    _ => continue,
                },
                ty
                @ (BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64 | BridgeType::Bool) => {
                    Some((raw_type(ty)?, quote!(rhs)))
                }
                _ => continue,
            },
            _ => continue,
        };
        let binary = |trait_name: &str, fn_name: &str| -> Option<(String, String)> {
            Some((trait_name.to_owned(), fn_name.to_owned()))
        };
        let (trait_name, fn_name) = match kind {
            OperatorKind::Add => binary("Add", "add"),
            OperatorKind::Sub => binary("Sub", "sub"),
            OperatorKind::Mul => binary("Mul", "mul"),
            OperatorKind::Div => binary("Div", "div"),
            OperatorKind::Rem => binary("Rem", "rem"),
            OperatorKind::BitXor => binary("BitXor", "bitxor"),
            OperatorKind::BitAnd => binary("BitAnd", "bitand"),
            OperatorKind::BitOr => binary("BitOr", "bitor"),
            OperatorKind::Neg => binary("Neg", "neg"),
            OperatorKind::Not => binary("Not", "not"),
            OperatorKind::AddAssign => binary("AddAssign", "add_assign"),
            OperatorKind::SubAssign => binary("SubAssign", "sub_assign"),
            OperatorKind::MulAssign => binary("MulAssign", "mul_assign"),
            OperatorKind::DivAssign => binary("DivAssign", "div_assign"),
            OperatorKind::RemAssign => binary("RemAssign", "rem_assign"),
            OperatorKind::Eq => binary("PartialEq", "eq"),
            _ => None,
        }
        .unwrap_or_default();
        if trait_name.is_empty() {
            continue;
        }
        let rhs_key = rhs
            .as_ref()
            .map(|(ty, _)| ty.to_string())
            .unwrap_or_default();
        if !seen.insert((trait_name.clone(), rhs_key)) {
            continue;
        }
        let trait_ident = ident(&trait_name)?;
        let fn_ident = ident(&fn_name)?;
        let assign = trait_name.ends_with("Assign");
        let tokens = match (kind, &rhs, &output.ty) {
            // `a == b`: sağ taraf `Self` olmalı.
            (OperatorKind::Eq, Some((ty, arg)), Some(out)) if this == ty.to_string() => {
                let arg = if arg.to_string() == "rhs" {
                    quote!(*other)
                } else {
                    quote!(other)
                };
                if out.to_string() != "bool" || *receiver_mutable {
                    continue;
                }
                quote! {
                    #[rustfmt::skip]
                    impl ::core::cmp::PartialEq for #this {
                        fn eq(&self, other: &Self) -> bool {
                            self.#call(#arg)
                        }
                    }
                }
            }
            (OperatorKind::Eq, _, _) => continue,
            (_, Some((ty, arg)), _) if assign && *receiver_mutable => quote! {
                #[rustfmt::skip]
                impl ::core::ops::#trait_ident<#ty> for #this {
                    fn #fn_ident(&mut self, rhs: #ty) {
                        let _ = self.#call(#arg);
                    }
                }
            },
            (_, Some((ty, arg)), Some(out)) if !assign && !*receiver_mutable => quote! {
                #[rustfmt::skip]
                impl ::core::ops::#trait_ident<#ty> for #this {
                    type Output = #out;
                    fn #fn_ident(self, rhs: #ty) -> #out {
                        self.#call(#arg)
                    }
                }
            },
            (OperatorKind::Neg | OperatorKind::Not, None, Some(out)) if !*receiver_mutable => {
                quote! {
                    #[rustfmt::skip]
                    impl ::core::ops::#trait_ident for #this {
                        type Output = #out;
                        fn #fn_ident(self) -> #out {
                            self.#call()
                        }
                    }
                }
            }
            _ => continue,
        };
        impls.push(tokens);
    }
    Ok(quote!(#(#impls)*))
}

// ---- Sınıflar ---------------------------------------------------------------------------------

fn class_items(ctx: &Ctx<'_, '_>, class: &ClassPlan) -> Result<TokenStream, CodegenError> {
    let raw = ident(&class.type_ref.rust_name)?;
    let cpp = class.type_ref.cpp_path.trim_start_matches("::").to_owned();
    let wrapped = ctx.is_wrapped(&class.type_ref);

    let mut methods = Vec::new();
    let mut forwarders = Vec::new();
    let mut associated = Vec::new();
    for method in &class.methods {
        let item = cpp_item(&method.callee);
        match &method.callee {
            Callee::Method {
                receiver_mutable, ..
            } => {
                let name = ident(&method.rust_name)?;
                let receiver = if *receiver_mutable && class.value.is_some() {
                    quote!(&mut self)
                } else if *receiver_mutable {
                    quote!(self: ::core::pin::Pin<&mut Self>)
                } else {
                    quote!(&self)
                };
                let Body {
                    inputs,
                    output,
                    statements,
                    doc,
                } = Body::new(ctx, method, &item, Some(receiver.clone()))?;
                if *receiver_mutable && wrapped {
                    let forward = ident(&wrapper_name(&method.rust_name))?;
                    let args = method
                        .params
                        .iter()
                        .map(|p| ident(&p.rust_name))
                        .collect::<Result<Vec<_>, _>>()?;
                    forwarders.push(quote! {
                        #doc
                        pub fn #forward(&mut self, #(#inputs),*) #output {
                            self.inner.pin_mut().#name(#(#args),*)
                        }
                    });
                }
                methods.push(quote! {
                    #doc
                    pub fn #name(#receiver, #(#inputs),*) #output {
                        #(#statements)*
                    }
                });
            }
            Callee::Static { .. } => {
                associated.push(associated_fn(ctx, class, method, &item, wrapped)?);
            }
            Callee::Constructor { .. } | Callee::Free { .. } => {}
        }
    }
    for constructor in &class.constructors {
        let item = cpp_item(&constructor.callee);
        associated.push(associated_fn(ctx, class, constructor, &item, true)?);
    }

    let mut out = Vec::new();
    if !methods.is_empty() || (!wrapped && !associated.is_empty()) {
        let doc = doc_line(&format!(
            " Güvenli metotlar: C++ `{cpp}`. Sahip olunan, `&` ve `Pin<&mut>` nesnelerde çalışır."
        ));
        let statics = if wrapped {
            Vec::new()
        } else {
            associated.clone()
        };
        let allows = item_allows();
        out.push(quote! {
            #doc
            #allows
            impl ffi::#raw {
                #(#methods)*
                #(#statics)*
            }
        });
    }
    if wrapped {
        out.push(wrapper(&raw, &cpp, &associated, &forwarders));
    }
    Ok(quote!(#(#out)*))
}

/// Sarmalayıcıdaki metot adı: yardımcılarla çakışırsa sonuna `_` eklenir.
fn wrapper_name(name: &str) -> String {
    if WRAPPER_HELPERS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// Constructor ya da statik metot: `Vehicle_new_i32` → `Vehicle::new_i32`.
fn associated_fn(
    ctx: &Ctx<'_, '_>,
    class: &ClassPlan,
    function: &FunctionPlan,
    item: &str,
    on_wrapper: bool,
) -> Result<TokenStream, CodegenError> {
    let prefix = format!("{}_", class.type_ref.rust_name);
    let short = function
        .rust_name
        .strip_prefix(&prefix)
        .unwrap_or(&function.rust_name);
    let short = if on_wrapper {
        wrapper_name(short)
    } else {
        short.to_owned()
    };
    let name = ident(&short)?;
    let Body {
        inputs,
        output,
        statements,
        doc,
    } = Body::new(ctx, function, item, None)?;
    Ok(quote! {
        #doc
        pub fn #name(#(#inputs),*) #output {
            #(#statements)*
        }
    })
}

fn wrapper(
    raw: &proc_macro2::Ident,
    cpp: &str,
    associated: &[TokenStream],
    forwarders: &[TokenStream],
) -> TokenStream {
    let doc = doc_line(&format!(
        " Sahip olunan C++ nesnesi: `{cpp}`. Düşünce C++ destructor'ı çalışır. İçindeki \
         `UniquePtr` asla null değildir."
    ));
    let name = raw;
    let allows = item_allows();
    quote! {
        #doc
        #allows
        pub struct #name {
            inner: ::cxx::UniquePtr<ffi::#raw>,
        }

        #allows
        impl #name {
            /// Ham `UniquePtr`'ı sarmalar; null ise `None`.
            pub fn from_unique_ptr(inner: ::cxx::UniquePtr<ffi::#raw>) -> ::core::option::Option<Self> {
                if inner.is_null() {
                    ::core::option::Option::None
                } else {
                    ::core::option::Option::Some(Self { inner })
                }
            }

            /// İçindeki `UniquePtr`'ı verir (ham `ffi` katmanıyla çalışmak için).
            pub fn into_inner(self) -> ::cxx::UniquePtr<ffi::#raw> {
                self.inner
            }

            /// `Pin<&mut ffi::T>` bekleyen fonksiyonlara geçirmek için.
            pub fn pin_mut(&mut self) -> ::core::pin::Pin<&mut ffi::#raw> {
                self.inner.pin_mut()
            }

            #(#associated)*
            #(#forwarders)*
        }

        #[rustfmt::skip]
        impl ::core::ops::Deref for #name {
            type Target = ffi::#raw;

            fn deref(&self) -> &ffi::#raw {
                // `from_unique_ptr` null'ı reddettiği için burada panik olmaz.
                &self.inner
            }
        }
    }
}

// ---- Fonksiyon gövdeleri ----------------------------------------------------------------------

/// Bir güvenli fonksiyonun imzası (alıcı hariç) ve gövdesi.
struct Body {
    inputs: Vec<TokenStream>,
    output: TokenStream,
    statements: Vec<TokenStream>,
    doc: TokenStream,
}

impl Body {
    fn new(
        ctx: &Ctx<'_, '_>,
        plan: &FunctionPlan,
        item: &str,
        receiver: Option<TokenStream>,
    ) -> Result<Body, CodegenError> {
        let mut inputs = Vec::new();
        let mut statements = Vec::new();
        let mut args = Vec::new();
        let mut notes = Vec::new();
        let mut copies = Vec::new();
        for param in &plan.params {
            let name = ident(&param.rust_name)?;
            let input = safe_param(ctx, &param.ty)?;
            match input {
                SafeParam::Same(ty) => {
                    inputs.push(quote!(#name: #ty));
                    args.push(quote!(#name));
                }
                SafeParam::Str => {
                    inputs.push(quote!(#name: &str));
                    statements.push(quote!(::cxx::let_cxx_string!(#name = #name);));
                    args.push(quote!(&#name));
                }
                SafeParam::Slice(element) => {
                    inputs.push(quote!(#name: &[#element]));
                    statements
                        .push(quote!(let #name = ::ferrobridge::__private::cxx_vector(#name);));
                    args.push(quote!(&#name));
                    copies.push(format!(
                        " `{}` her çağrıda yeni bir C++ vector'üne kopyalanır.",
                        param.rust_name
                    ));
                }
                SafeParam::Enum(safe) => {
                    inputs.push(quote!(#name: #safe));
                    args.push(quote!(ffi::#safe::from(#name)));
                }
                SafeParam::Wrapper(wrapper) => {
                    inputs.push(quote!(#name: #wrapper));
                    args.push(quote!(#name.into_inner()));
                }
            }
        }

        let call = raw_call(ctx, plan, receiver.is_some(), &args)?;
        let conversion = match &plan.callee {
            // Değer tipinin constructor'ı değer döndürür.
            Callee::Constructor { .. }
                if matches!(plan.returns, Returns::Value(BridgeType::Value(_))) =>
            {
                safe_return(ctx, &plan.returns, item)?
            }
            // Constructor shim'i `make_unique` döndürür: null olamaz, sarmalayıcı doğrudan döner.
            Callee::Constructor { class } => {
                safe_return(ctx, &Returns::BoxedClass(class.clone()), item)?
            }
            _ => safe_return(ctx, &plan.returns, item)?,
        };
        let fallible = plan.fallible || conversion.fallible;
        if conversion.fallible && !conversion.note.is_empty() {
            notes.push(conversion.note.to_owned());
        }
        let mapped = if plan.fallible {
            quote!(#call.map_err(::ferrobridge::__private::cpp(#item)))
        } else {
            call
        };
        let ty = &conversion.ty;
        let output = match (&conversion.ty, fallible) {
            (None, false) => quote!(),
            (None, true) => quote!(-> ::ferrobridge::Result<()>),
            (Some(_), false) => quote!(-> #ty),
            (Some(_), true) => quote!(-> ::ferrobridge::Result<#ty>),
        };
        statements.push(
            match (&conversion.convert, plan.fallible, conversion.fallible) {
                // Dönüşüm yok: sonuç olduğu gibi döner.
                (None, _, _) => mapped,
                (Some(convert), true, true) => quote! {
                    let value = #mapped?;
                    #convert
                },
                (Some(convert), true, false) => quote! {
                    let value = #mapped?;
                    ::core::result::Result::Ok(#convert)
                },
                (Some(convert), false, _) => quote! {
                    let value = #mapped;
                    #convert
                },
            },
        );

        let mut doc_text = vec![format!(" C++: `{item}`")];
        if !copies.is_empty() {
            doc_text.push(String::new());
            doc_text.extend(copies);
        }
        if fallible {
            doc_text.push(String::new());
            doc_text.push(" # Errors".to_owned());
            doc_text.push(String::new());
            if plan.fallible {
                doc_text
                    .push(" C++ bir exception fırlatırsa `ferrobridge::Error::Cpp`.".to_owned());
            }
            for length in &plan.lengths {
                doc_text.push(format!(
                    " `{}` C++'taki uzunluk parametresinin tipine (`{}`) sığmayacak kadar \
                     uzunsa da `ferrobridge::Error::Cpp`; uzunluk sessizce kırpılmaz.",
                    plan.params[length.slice].rust_name, length.cpp_type
                ));
            }
        }
        doc_text.extend(notes);
        let doc = doc_text.iter().map(|line| doc_line(line));
        Ok(Body {
            inputs,
            output,
            statements,
            doc: quote!(#(#doc)*),
        })
    }
}

/// Ham katmandaki çağrı ifadesi.
fn raw_call(
    ctx: &Ctx<'_, '_>,
    plan: &FunctionPlan,
    has_receiver: bool,
    args: &[TokenStream],
) -> Result<TokenStream, CodegenError> {
    if let Some(shim) = ctx.shim_name(plan) {
        let shim = ident(shim)?;
        return Ok(if has_receiver {
            quote!(ffi::#shim(self, #(#args),*))
        } else {
            quote!(ffi::#shim(#(#args),*))
        });
    }
    match &plan.callee {
        Callee::Method { .. } => {
            let raw = ident(&raw_method_name(&plan.rust_name))?;
            Ok(quote!(self.#raw(#(#args),*)))
        }
        _ => {
            let name = ident(&plan.rust_name)?;
            Ok(quote!(ffi::#name(#(#args),*)))
        }
    }
}

/// Bir parametrenin güvenli katmandaki biçimi.
enum SafeParam {
    /// Ham tiple aynı (tam nitelikli yazımı).
    Same(TokenStream),
    /// `&CxxString` → `&str`.
    Str,
    /// `&CxxVector<T>` (T sayı) → `&[T]`.
    Slice(TokenStream),
    /// Paylaşılan enum → gerçek Rust enum'u.
    Enum(proc_macro2::Ident),
    /// `UniquePtr<T>` (sahiplik C++'a geçer) → sarmalayıcı, değerle.
    Wrapper(proc_macro2::Ident),
}

fn safe_param(ctx: &Ctx<'_, '_>, ty: &BridgeType) -> Result<SafeParam, CodegenError> {
    Ok(match ty {
        BridgeType::Ref {
            mutable: false,
            inner,
        } => match inner.as_ref() {
            BridgeType::String => SafeParam::Str,
            BridgeType::Vector(element) if is_number(element) => {
                SafeParam::Slice(raw_type(element)?)
            }
            _ => SafeParam::Same(raw_type(ty)?),
        },
        BridgeType::Enum(e) => SafeParam::Enum(ident(&e.rust_name)?),
        BridgeType::UniquePtr(inner) => match inner.as_ref() {
            BridgeType::Class(class) if ctx.is_wrapped(class) => {
                SafeParam::Wrapper(ident(&class.rust_name)?)
            }
            _ => SafeParam::Same(raw_type(ty)?),
        },
        _ => SafeParam::Same(raw_type(ty)?),
    })
}

fn is_number(ty: &BridgeType) -> bool {
    matches!(ty, BridgeType::Int(_) | BridgeType::F32 | BridgeType::F64)
}

/// Dönüşün güvenli katmandaki biçimi.
struct SafeReturn {
    /// Güvenli tip (`Result` sarmalaması hariç); `()` için `None`.
    ty: Option<TokenStream>,
    /// Ham sonucu (`value`) çeviren ifade; `None` ise olduğu gibi döner.
    convert: Option<TokenStream>,
    /// Dönüşüm başarısız olabilir mi (UTF-8, tanımsız enum, null)?
    fallible: bool,
    /// Dönüşüm başarısız olabiliyorsa dokümana eklenecek açıklama.
    note: &'static str,
}

fn safe_return(
    ctx: &Ctx<'_, '_>,
    returns: &Returns,
    item: &str,
) -> Result<SafeReturn, CodegenError> {
    let same = |ty: TokenStream| SafeReturn {
        ty: Some(ty),
        convert: None,
        fallible: false,
        note: "",
    };
    let utf8 = " C++ geçerli UTF-8 olmayan bir string döndürürse `ferrobridge::Error::Utf8`.";
    Ok(match returns {
        Returns::Unit => SafeReturn {
            ty: None,
            convert: None,
            fallible: false,
            note: "",
        },
        Returns::Value(BridgeType::Enum(e)) => {
            let safe = ident(&e.rust_name)?;
            SafeReturn {
                ty: Some(quote!(#safe)),
                convert: Some(quote!(#safe::try_from(value))),
                fallible: true,
                note: " C++ enum'da tanımlı olmayan bir değer döndürürse \
                       `ferrobridge::Error::UnknownEnumValue`.",
            }
        }
        Returns::Value(BridgeType::Ref {
            mutable: false,
            inner,
        }) if matches!(inner.as_ref(), BridgeType::String) => SafeReturn {
            ty: Some(quote!(&str)),
            convert: Some(quote!(::ferrobridge::__private::str_ref(#item, value))),
            fallible: true,
            note: utf8,
        },
        Returns::Value(BridgeType::UniquePtr(inner)) => match inner.as_ref() {
            BridgeType::Class(class) if ctx.is_wrapped(class) => {
                let wrapper = ident(&class.rust_name)?;
                SafeReturn {
                    ty: Some(quote!(::core::option::Option<#wrapper>)),
                    convert: Some(quote!(#wrapper::from_unique_ptr(value))),
                    fallible: false,
                    note: "",
                }
            }
            _ => same(raw_type(&BridgeType::UniquePtr(inner.clone()))?),
        },
        Returns::Value(ty) => same(raw_type(ty)?),
        Returns::BoxedClass(class) => {
            let wrapper = ident(&class.rust_name)?;
            SafeReturn {
                ty: Some(quote!(#wrapper)),
                convert: Some(quote! {
                    #wrapper::from_unique_ptr(value)
                        .ok_or(::ferrobridge::Error::NullPointer { item: #item })
                }),
                fallible: true,
                note: "",
            }
        }
        Returns::OwnedString => SafeReturn {
            ty: Some(quote!(::std::string::String)),
            convert: Some(quote!(::ferrobridge::__private::string(#item, value))),
            fallible: true,
            note: utf8,
        },
        Returns::OwnedVector(element) if is_number(element) => {
            let element = raw_type(element)?;
            SafeReturn {
                ty: Some(quote!(::std::vec::Vec<#element>)),
                convert: Some(quote!(::ferrobridge::__private::vec(#item, value))),
                fallible: true,
                note: "",
            }
        }
        Returns::OwnedVector(BridgeType::String) => SafeReturn {
            ty: Some(quote!(::std::vec::Vec<::std::string::String>)),
            convert: Some(quote!(::ferrobridge::__private::strings(#item, value))),
            fallible: true,
            note: utf8,
        },
        Returns::OwnedVector(element) => {
            let element = raw_type(element)?;
            same(quote!(::cxx::UniquePtr<::cxx::CxxVector<#element>>))
        }
        Returns::OwnedPointer(BridgeType::Class(class)) if ctx.is_wrapped(class) => {
            let wrapper = ident(&class.rust_name)?;
            SafeReturn {
                ty: Some(quote!(::core::option::Option<#wrapper>)),
                convert: Some(quote!(#wrapper::from_unique_ptr(value))),
                fallible: false,
                note: "",
            }
        }
        Returns::OwnedPointer(BridgeType::String) => SafeReturn {
            ty: Some(quote!(::core::option::Option<::std::string::String>)),
            convert: Some(quote! {
                if value.is_null() {
                    ::core::result::Result::Ok(::core::option::Option::None)
                } else {
                    ::ferrobridge::__private::string(#item, value).map(::core::option::Option::Some)
                }
            }),
            fallible: true,
            note: utf8,
        },
        Returns::OwnedPointer(inner) => {
            same(raw_type(&BridgeType::UniquePtr(Box::new(inner.clone())))?)
        }
        Returns::BorrowedPointer { mutable, inner } => {
            let reference = BridgeType::Ref {
                mutable: *mutable,
                inner: Box::new(inner.clone()),
            };
            if !mutable && matches!(inner, BridgeType::String) {
                SafeReturn {
                    ty: Some(quote!(&str)),
                    convert: Some(quote!(::ferrobridge::__private::str_ref(#item, value))),
                    fallible: true,
                    note: utf8,
                }
            } else {
                same(raw_type(&reference)?)
            }
        }
    })
}

/// Bir bridge tipinin güvenli katmandan (bridge modülünün dışından) tam nitelikli yazımı.
fn raw_type(ty: &BridgeType) -> Result<TokenStream, CodegenError> {
    Ok(match ty {
        BridgeType::Bool => quote!(bool),
        BridgeType::Char => quote!(::core::ffi::c_char),
        BridgeType::Int(int) => {
            let int = ident(int.rust_name())?;
            quote!(#int)
        }
        BridgeType::F32 => quote!(f32),
        BridgeType::F64 => quote!(f64),
        BridgeType::Class(r) | BridgeType::Enum(r) => {
            let name = ident(&r.rust_name)?;
            quote!(ffi::#name)
        }
        // Değer tipi güvenli katmanın kendi struct'ı.
        BridgeType::Value(r) => {
            let name = ident(&r.rust_name)?;
            quote!(#name)
        }
        BridgeType::String => quote!(::cxx::CxxString),
        BridgeType::UniquePtr(inner) => {
            let inner = raw_type(inner)?;
            quote!(::cxx::UniquePtr<#inner>)
        }
        BridgeType::Vector(inner) => {
            let inner = raw_type(inner)?;
            quote!(::cxx::CxxVector<#inner>)
        }
        BridgeType::Slice { mutable, element } => {
            let element = raw_type(element)?;
            if *mutable {
                quote!(&mut [#element])
            } else {
                quote!(&[#element])
            }
        }
        BridgeType::Ref {
            mutable: false,
            inner,
        } => {
            let inner = raw_type(inner)?;
            quote!(&#inner)
        }
        BridgeType::Ref {
            mutable: true,
            inner,
        } => {
            let opaque = matches!(
                inner.as_ref(),
                BridgeType::Class(_) | BridgeType::String | BridgeType::Vector(_)
            );
            let inner = raw_type(inner)?;
            if opaque {
                quote!(::core::pin::Pin<&mut #inner>)
            } else {
                quote!(&mut #inner)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use ferrobridge_analysis::IntType;

    use super::*;

    fn type_ref(name: &str) -> TypeRef {
        TypeRef {
            rust_name: name.to_owned(),
            cpp_path: format!("::geo::{name}"),
        }
    }

    fn class(name: &str) -> BridgeType {
        BridgeType::Class(type_ref(name))
    }

    fn reference(mutable: bool, inner: BridgeType) -> BridgeType {
        BridgeType::Ref {
            mutable,
            inner: Box::new(inner),
        }
    }

    fn with_ctx<T>(f: impl FnOnce(&Ctx<'_, '_>) -> T) -> T {
        let wrapped = BTreeSet::from(["Widget".to_owned()]);
        f(&Ctx {
            shims: &[],
            wrapped: &wrapped,
        })
    }

    fn flat(tokens: &TokenStream) -> String {
        tokens.to_string().replace(' ', "")
    }

    fn param(ty: &BridgeType) -> String {
        with_ctx(|ctx| match safe_param(ctx, ty).unwrap() {
            SafeParam::Same(t) => format!("aynı {}", flat(&t)),
            SafeParam::Str => "&str".to_owned(),
            SafeParam::Slice(t) => format!("&[{}]", flat(&t)),
            SafeParam::Enum(e) => format!("enum {e}"),
            SafeParam::Wrapper(w) => format!("sarmalayıcı {w}"),
        })
    }

    fn ret(returns: &Returns) -> (String, bool) {
        with_ctx(|ctx| {
            let r = safe_return(ctx, returns, "geo::f").unwrap();
            (r.ty.as_ref().map(flat).unwrap_or_default(), r.fallible)
        })
    }

    #[test]
    fn params_follow_the_safe_type_table() {
        let i32_ty = BridgeType::Int(IntType::I32);
        let cases = [
            (i32_ty.clone(), "aynı i32"),
            (reference(false, BridgeType::String), "&str"),
            (
                reference(false, BridgeType::Vector(Box::new(BridgeType::F64))),
                "&[f64]",
            ),
            (
                reference(false, BridgeType::Vector(Box::new(BridgeType::String))),
                "aynı &::cxx::CxxVector<::cxx::CxxString>",
            ),
            (
                reference(true, BridgeType::String),
                "aynı ::core::pin::Pin<&mut::cxx::CxxString>",
            ),
            (reference(false, class("Point")), "aynı &ffi::Point"),
            (
                reference(true, class("Widget")),
                "aynı ::core::pin::Pin<&mutffi::Widget>",
            ),
            (reference(true, BridgeType::F64), "aynı &mutf64"),
            (BridgeType::Enum(type_ref("Mode")), "enum Mode"),
            (
                BridgeType::UniquePtr(Box::new(class("Widget"))),
                "sarmalayıcı Widget",
            ),
            (
                BridgeType::UniquePtr(Box::new(class("Point"))),
                "aynı ::cxx::UniquePtr<ffi::Point>",
            ),
            (BridgeType::Char, "aynı ::core::ffi::c_char"),
        ];
        for (ty, expected) in cases {
            assert_eq!(param(&ty), expected, "{ty:?}");
        }
    }

    #[test]
    fn returns_follow_the_safe_type_table() {
        let cases = [
            (Returns::Unit, ("", false)),
            (Returns::OwnedString, ("::std::string::String", true)),
            (
                Returns::Value(reference(false, BridgeType::String)),
                ("&str", true),
            ),
            (
                Returns::Value(BridgeType::Enum(type_ref("Mode"))),
                ("Mode", true),
            ),
            (Returns::BoxedClass(type_ref("Widget")), ("Widget", true)),
            (
                Returns::OwnedPointer(class("Widget")),
                ("::core::option::Option<Widget>", false),
            ),
            (
                Returns::Value(BridgeType::UniquePtr(Box::new(class("Widget")))),
                ("::core::option::Option<Widget>", false),
            ),
            (
                Returns::OwnedPointer(BridgeType::String),
                ("::core::option::Option<::std::string::String>", true),
            ),
            (
                Returns::OwnedVector(BridgeType::Int(IntType::U8)),
                ("::std::vec::Vec<u8>", true),
            ),
            (
                Returns::OwnedVector(BridgeType::String),
                ("::std::vec::Vec<::std::string::String>", true),
            ),
            (
                Returns::OwnedVector(class("Widget")),
                ("::cxx::UniquePtr<::cxx::CxxVector<ffi::Widget>>", false),
            ),
            (
                Returns::BorrowedPointer {
                    mutable: true,
                    inner: class("Widget"),
                },
                ("::core::pin::Pin<&mutffi::Widget>", false),
            ),
            (
                Returns::BorrowedPointer {
                    mutable: false,
                    inner: BridgeType::String,
                },
                ("&str", true),
            ),
        ];
        for (returns, (ty, fallible)) in cases {
            assert_eq!(ret(&returns), (ty.to_owned(), fallible), "{returns:?}");
        }
    }

    #[test]
    fn error_items_are_full_cpp_names() {
        let widget = type_ref("Widget");
        assert_eq!(
            cpp_item(&Callee::Constructor {
                class: widget.clone()
            }),
            "geo::Widget::Widget"
        );
        assert_eq!(
            cpp_item(&Callee::Method {
                class: widget.clone(),
                cpp_name: "adopt".to_owned(),
                receiver_mutable: true
            }),
            "geo::Widget::adopt"
        );
        assert_eq!(
            cpp_item(&Callee::Free {
                namespace: vec!["io".to_owned()],
                cpp_name: "next".to_owned()
            }),
            "io::next"
        );
    }

    #[test]
    fn wrapper_helper_names_are_never_shadowed() {
        assert_eq!(wrapper_name("pin_mut"), "pin_mut_");
        assert_eq!(wrapper_name("into_inner"), "into_inner_");
        assert_eq!(wrapper_name("drive"), "drive");
    }
}
