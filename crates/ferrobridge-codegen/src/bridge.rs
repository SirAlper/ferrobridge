//! `bridge.rs`: `#[cxx::bridge]` modülünü üretir.

use ferrobridge_analysis::{
    BridgePlan, Callee, ClassPlan, EnumPlan, FunctionPlan, ParamPlan, TypeRef,
};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::spelling::{ident, rust_return, rust_type};
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
    for class in &plan.classes {
        add_class(&mut blocks, class)?;
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
    let shim_decls = shims
        .iter()
        .map(|shim| function_decl(&shim.name, None, shim_receiver(shim.plan), shim.plan))
        .collect::<Result<Vec<_>, _>>()?;
    let namespace_blocks = blocks.into_tokens();

    let module = quote! {
        #[cxx::bridge]
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
    };
    let file: syn::File =
        syn::parse2(module).map_err(|error| CodegenError::Internal(error.to_string()))?;
    Ok(prettyplease::unparse(&file))
}

fn add_class(blocks: &mut NamespaceBlocks, class: &ClassPlan) -> Result<(), CodegenError> {
    let block = blocks.get(&class.namespace);
    block.push(extern_type(&class.type_ref, &class.cpp_name)?);
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
        };
        block.push(function_decl(
            &method.rust_name,
            Some(cpp_name),
            Some(receiver),
            method,
        )?);
    }
    Ok(())
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
}

fn shim_receiver(plan: &FunctionPlan) -> Option<Receiver<'_>> {
    match &plan.callee {
        Callee::Method {
            class,
            receiver_mutable,
            ..
        } => Some(Receiver {
            name: SHIM_SELF,
            class,
            mutable: *receiver_mutable,
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
    let mut inputs = Vec::new();
    if let Some(receiver) = receiver {
        let self_name = if receiver.name == "self" {
            quote!(self)
        } else {
            let name = ident(receiver.name)?;
            quote!(#name)
        };
        let class = ident(&receiver.class.rust_name)?;
        inputs.push(if receiver.mutable {
            quote!(#self_name: Pin<&mut #class>)
        } else {
            quote!(#self_name: &#class)
        });
    }
    for ParamPlan { rust_name, ty } in &plan.params {
        let param = ident(rust_name)?;
        let ty = rust_type(ty)?;
        inputs.push(quote!(#param: #ty));
    }
    let output = match (rust_return(plan)?, plan.fallible) {
        (None, false) => quote!(),
        (None, true) => quote!(-> Result<()>),
        (Some(ty), false) => quote!(-> #ty),
        (Some(ty), true) => quote!(-> Result<#ty>),
    };
    Ok(quote! {
        #rename
        fn #name(#(#inputs),*) #output;
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
