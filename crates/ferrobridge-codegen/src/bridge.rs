//! `bridge.rs`: `#[cxx::bridge]` modülünü üretir.

use ferrobridge_analysis::{
    BridgePlan, Callee, ClassPlan, EnumPlan, FunctionPlan, ParamPlan, TypeRef,
};
use std::collections::BTreeSet;

use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::spelling::{ident, returns_reference, rust_return, rust_type};
use crate::{CodegenError, CodegenOptions, ShimFn};

/// Shim fonksiyonlarında alıcının (C++'taki `this`) parametre adı. C++ anahtar kelimesi
/// olmamalı: cxx parametre adlarını üretilen C++ koduna olduğu gibi kopyalar.
pub(crate) const SHIM_SELF: &str = "fb_self";

pub(crate) fn generate(
    plan: &BridgePlan,
    options: &CodegenOptions,
    shims: &[ShimFn<'_>],
) -> Result<String, CodegenError> {
    let enums = plan
        .enums
        .iter()
        .map(shared_enum)
        .collect::<Result<Vec<_>, _>>()?;

    let mut blocks = NamespaceBlocks::default();
    for enumeration in &plan.enums {
        blocks
            .get(&enumeration.namespace)
            .push(extern_type(&enumeration.type_ref, &enumeration.cpp_name)?);
    }
    let shim_namespace_path: Vec<String> = options
        .shim_namespace
        .split("::")
        .map(str::to_owned)
        .collect();
    for class in &plan.classes {
        add_class(&mut blocks, class, &shim_namespace_path)?;
    }
    for function in plan.functions.iter().filter(|f| !f.needs_shim()) {
        let Callee::Free {
            namespace,
            cpp_name,
        } = &function.callee
        else {
            continue;
        };
        let decl = function_decl(&function.rust_name, Some(cpp_name), None, function)?;
        blocks.get(namespace).push(decl);
    }

    let shim_namespace = &options.shim_namespace;
    let shim_include = &options.shim_include;
    let value_classes: BTreeSet<String> = plan
        .classes
        .iter()
        .filter(|c| c.value.is_some())
        .map(|c| c.type_ref.rust_name.clone())
        .collect();
    let shim_decls = shims
        .iter()
        .map(|shim| {
            function_decl(
                &shim.name,
                None,
                shim_receiver(shim.plan, &value_classes),
                shim.plan,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let namespace_blocks = blocks.into_tokens();
    let safe_layer = crate::safe::generate(plan, shims, &options.shim_namespace)?;

    // `#[rustfmt::skip]`: dosya commit'lenir (ADR-0004); `cargo fmt` onu prettyplease'ten farklı
    // biçimlendirip üreticinin çıktısıyla uyumsuz hale getirmesin.
    let module = quote! {
        #[cxx::bridge]
        #[rustfmt::skip]
        #[allow(non_snake_case, non_camel_case_types, missing_docs)]
        pub mod ffi {
            #(#enums)*

            #[namespace = #shim_namespace]
            unsafe extern "C++" {
                include!(#shim_include);
                #(#shim_decls)*
            }

            #(#namespace_blocks)*
        }

        #safe_layer
    };
    let file: syn::File =
        syn::parse2(module).map_err(|error| CodegenError::Internal(error.to_string()))?;
    Ok(with_safety_comments(&with_type_aliases(
        &prettyplease::unparse(&file),
    )))
}

/// `prettyplease` yorumları atar; değer tiplerinin `unsafe impl ExternType`'ının gerekçesini
/// (5. kural) biçimlendirmeden sonra, impl'in özniteliklerinin üstüne ekler.
fn with_safety_comments(code: &str) -> String {
    const SAFETY: &str = "// SAFETY: Rust struct'ı `#[repr(C)]` ve alanları C++ sınıfıyla aynı \
sırada ve tipte; `shim.cc` C++ tipinin bit bit kopyalanabildiğini (`is_trivially_copyable`), \
standart düzende olduğunu ve boyutunu, hizalamasını, her alanın ofsetini `static_assert` ile, \
Rust tarafı da `size_of`/`offset_of!` ile doğruluyor. Bu yüzden cxx'in `Trivial` türü (değerle \
taşıma, `&mut` ile değiştirme) güvenli.";
    let lines: Vec<&str> = code.lines().collect();
    let mut out = String::with_capacity(code.len());
    for (index, line) in lines.iter().enumerate() {
        let starts_impl = lines[index..]
            .iter()
            .take_while(|l| l.trim_start().starts_with("#["))
            .count();
        let target = lines.get(index + starts_impl);
        let opens_attributes = index == 0 || !lines[index - 1].trim_start().starts_with("#[");
        if opens_attributes
            && target.is_some_and(|l| l.trim_start().starts_with("unsafe impl ::cxx::ExternType"))
        {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push_str(indent);
            out.push_str(SAFETY);
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn add_class(
    blocks: &mut NamespaceBlocks,
    class: &ClassPlan,
    shim_namespace: &[String],
) -> Result<(), CodegenError> {
    // Template örneklemesinin C++'ta adı yok: shim namespace'inde Rust adıyla bir takma adı var.
    let (namespace, cpp_name) = if class.instance_spelling.is_some() {
        (shim_namespace.to_vec(), class.type_ref.rust_name.as_str())
    } else {
        (class.namespace.clone(), class.cpp_name.as_str())
    };
    let block = blocks.get(&namespace);
    block.push(if class.value.is_some() {
        value_type(&class.type_ref, cpp_name)?
    } else {
        extern_type(&class.type_ref, cpp_name)?
    });
    for method in class.methods.iter().filter(|m| !m.needs_shim()) {
        let Callee::Method {
            class: owner,
            cpp_name,
            receiver_mutable,
        } = &method.callee
        else {
            continue;
        };
        let receiver = Receiver {
            name: "self",
            class: owner,
            mutable: *receiver_mutable,
            value: class.value.is_some(),
        };
        // Ham metot `raw_<ad>` olur: asıl ad, aynı tipe yazılan güvenli metoda kalır.
        block.push(function_decl(
            &raw_method_name(&method.rust_name),
            Some(cpp_name),
            Some(receiver),
            method,
        )?);
    }
    Ok(())
}

/// cxx katmanındaki metodun Rust adı. Güvenli katman metodu asıl adla aynı tipe yazar
/// (`impl ffi::Vehicle { pub fn wheels(..) }`); ikisi çakışmasın diye ham olana ön ek gelir.
pub(crate) fn raw_method_name(rust_name: &str) -> String {
    format!("raw_{rust_name}")
}

/// Paylaşılan enum: cxx, değerlerin C++'takiyle aynı olduğunu derleme zamanında doğrular.
fn shared_enum(enumeration: &EnumPlan) -> Result<TokenStream, CodegenError> {
    let namespace = enumeration.namespace.join("::");
    let name = ident(&enumeration.type_ref.rust_name)?;
    let repr = ident(enumeration.repr.rust_name())?;
    let rename = cxx_name_attr(&enumeration.type_ref.rust_name, &enumeration.cpp_name);
    let variants = enumeration
        .variants
        .iter()
        .map(|variant| {
            let name = ident(&variant.name)?;
            let magnitude = Literal::u64_unsuffixed(variant.value.unsigned_abs());
            Ok(if variant.value < 0 {
                quote!(#name = -#magnitude)
            } else {
                quote!(#name = #magnitude)
            })
        })
        .collect::<Result<Vec<_>, CodegenError>>()?;
    let namespace_attr = namespace_attr(&namespace);
    Ok(quote! {
        #namespace_attr
        #rename
        #[repr(#repr)]
        #[derive(Debug)]
        enum #name {
            #(#variants),*
        }
    })
}

/// Değer tipi: Rust'taki `#[repr(C)]` struct'a takma ad; cxx onu `ExternType` ile tanır.
fn value_type(type_ref: &TypeRef, cpp_name: &str) -> Result<TokenStream, CodegenError> {
    let name = ident(&type_ref.rust_name)?;
    let rename = cxx_name_attr(&type_ref.rust_name, cpp_name);
    // `prettyplease` `extern` bloğundaki `type X = super::X;`'i yazdıramıyor: işaretli bir
    // `type X;` üretilir, biçimlendirmeden sonra takma ada çevrilir (`with_type_aliases`).
    Ok(quote! {
        #[doc = #ALIAS_MARKER]
        #rename
        type #name;
    })
}

/// [`value_type`]'ın işaretleyicisi; üretilen dosyada kalmaz.
const ALIAS_MARKER: &str = "__ferrobridge_value_type__";

/// İşaretli `type X;` satırlarını `type X = super::X;` yapar ve işaretleyiciyi siler.
fn with_type_aliases(code: &str) -> String {
    // `prettyplease` `#[doc = "..."]`'yı `///` yorumu olarak yazar.
    let markers = [
        format!("#[doc = \"{ALIAS_MARKER}\"]"),
        format!("///{ALIAS_MARKER}"),
        format!("/// {ALIAS_MARKER}"),
    ];
    let mut out = String::with_capacity(code.len());
    let mut pending = false;
    for line in code.lines() {
        let trimmed = line.trim_start();
        if markers.iter().any(|m| m == trimmed) {
            pending = true;
            continue;
        }
        if pending && trimmed.starts_with("type ") && trimmed.ends_with(';') {
            let name = trimmed["type ".len()..trimmed.len() - 1].trim();
            let indent = &line[..line.len() - trimmed.len()];
            out.push_str(&format!("{indent}type {name} = super::{name};\n"));
            pending = false;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn extern_type(type_ref: &TypeRef, cpp_name: &str) -> Result<TokenStream, CodegenError> {
    let name = ident(&type_ref.rust_name)?;
    let rename = cxx_name_attr(&type_ref.rust_name, cpp_name);
    Ok(quote! {
        #rename
        type #name;
    })
}

/// Metodun alıcısı.
struct Receiver<'a> {
    name: &'static str,
    class: &'a TypeRef,
    mutable: bool,
    /// Değer tipi mi (değiştirilebilir alıcı `Pin` değil düz `&mut`)?
    value: bool,
}

fn shim_receiver<'p>(
    plan: &'p FunctionPlan,
    value_classes: &BTreeSet<String>,
) -> Option<Receiver<'p>> {
    match &plan.callee {
        Callee::Method {
            class,
            receiver_mutable,
            ..
        } => Some(Receiver {
            name: SHIM_SELF,
            class,
            mutable: *receiver_mutable,
            value: value_classes.contains(&class.rust_name),
        }),
        _ => None,
    }
}

fn function_decl(
    rust_name: &str,
    cpp_name: Option<&str>,
    receiver: Option<Receiver<'_>>,
    plan: &FunctionPlan,
) -> Result<TokenStream, CodegenError> {
    let name = ident(rust_name)?;
    let rename = cpp_name.map(|cpp| cxx_name_attr(rust_name, cpp));
    // Shim'ler serbest fonksiyondur; `self` ömür kuralı işlemez. Dönen referans alıcıya açık
    // bir ömürle bağlanır (başka referans parametresi varsa cxx bunu zorunlu tutar).
    let lifetime = receiver
        .as_ref()
        .filter(|r| r.name != "self" && returns_reference(plan))
        .map(|_| quote!('a));
    let generics = lifetime.as_ref().map(|lt| quote!(<#lt>));
    let mut inputs = Vec::new();
    if let Some(receiver) = receiver {
        let self_name = if receiver.name == "self" {
            quote!(self)
        } else {
            let name = ident(receiver.name)?;
            quote!(#name)
        };
        let class = ident(&receiver.class.rust_name)?;
        inputs.push(if receiver.mutable && receiver.value {
            quote!(#self_name: &#lifetime mut #class)
        } else if receiver.mutable {
            quote!(#self_name: Pin<&#lifetime mut #class>)
        } else {
            quote!(#self_name: &#lifetime #class)
        });
    }
    for ParamPlan { rust_name, ty, .. } in &plan.params {
        let param = ident(rust_name)?;
        let ty = rust_type(ty)?;
        inputs.push(quote!(#param: #ty));
    }
    let output = match (rust_return(plan, lifetime.as_ref())?, plan.fallible) {
        (None, false) => quote!(),
        (None, true) => quote!(-> Result<()>),
        (Some(ty), false) => quote!(-> #ty),
        (Some(ty), true) => quote!(-> Result<#ty>),
    };
    Ok(quote! {
        #rename
        fn #name #generics(#(#inputs),*) #output;
    })
}

/// Rust adı C++ adından farklıysa `#[cxx_name = "..."]`.
fn cxx_name_attr(rust_name: &str, cpp_name: &str) -> TokenStream {
    if rust_name == cpp_name {
        quote!()
    } else {
        quote!(#[cxx_name = #cpp_name])
    }
}

fn namespace_attr(namespace: &str) -> TokenStream {
    if namespace.is_empty() {
        quote!()
    } else {
        quote!(#[namespace = #namespace])
    }
}

/// Namespace başına bir `unsafe extern "C++"` bloğu, ilk görülme sırasıyla.
#[derive(Default)]
struct NamespaceBlocks {
    blocks: Vec<(Vec<String>, Vec<TokenStream>)>,
}

impl NamespaceBlocks {
    fn get(&mut self, namespace: &[String]) -> &mut Vec<TokenStream> {
        let index = match self.blocks.iter().position(|(ns, _)| ns == namespace) {
            Some(index) => index,
            None => {
                self.blocks.push((namespace.to_vec(), Vec::new()));
                self.blocks.len() - 1
            }
        };
        &mut self.blocks[index].1
    }

    fn into_tokens(self) -> Vec<TokenStream> {
        self.blocks
            .into_iter()
            .map(|(namespace, items)| {
                let attr = namespace_attr(&namespace.join("::"));
                quote! {
                    #attr
                    unsafe extern "C++" {
                        #(#items)*
                    }
                }
            })
            .collect()
    }
}
