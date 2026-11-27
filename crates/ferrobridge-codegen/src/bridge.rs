//! `bridge.rs`: generates the `#[cxx::bridge]` module.

use ferrobridge_analysis::{
    BridgePlan, Callee, ClassPlan, EnumPlan, FunctionPlan, ParamPlan, TypeRef,
};
use std::collections::BTreeSet;

use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::spelling::{ident, returns_reference, rust_return, rust_type};
use crate::{CodegenError, CodegenOptions, ShimFn};

/// The parameter name of the receiver (C++'s `this`) in shim functions. It must not be a C++
/// keyword: cxx copies parameter names into the generated C++ code as they are.
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

    // `#[rustfmt::skip]`: the file is committed (ADR-0004); `cargo fmt` must not format it differently
    // from prettyplease and make it inconsistent with the generator's output.
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

/// `prettyplease` drops comments; adds the justification of the value types' `unsafe impl ExternType`
/// (rule 5) after formatting, above the impl's attributes.
fn with_safety_comments(code: &str) -> String {
    const SAFETY: &str = "// SAFETY: the Rust struct is `#[repr(C)]` and its fields have the same order and \
types as the C++ class; `shim.cc` checks with `static_assert` that the C++ type is trivially \
copyable (`is_trivially_copyable`) and standard-layout, and checks its size, alignment and every \
field's offset; the Rust side checks them with `size_of`/`offset_of!`. So cxx's `Trivial` kind \
(moving by value, changing through `&mut`) is safe.";
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
    // A template instantiation has no name in C++: it has an alias with the Rust name in the shim namespace.
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
        // The raw method becomes `raw_<name>`: the real name is left to the safe method written on the same type.
        block.push(function_decl(
            &raw_method_name(&method.rust_name),
            Some(cpp_name),
            Some(receiver),
            method,
        )?);
    }
    Ok(())
}

/// The Rust name of a method in the cxx layer. The safe layer writes the method with the real name
/// on the same type (`impl ffi::Vehicle { pub fn wheels(..) }`); so the two do not clash, the raw one gets a prefix.
pub(crate) fn raw_method_name(rust_name: &str) -> String {
    format!("raw_{rust_name}")
}

/// A shared enum: cxx checks at compile time that the values are the same as in C++.
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

/// A value type: an alias to the `#[repr(C)]` struct in Rust; cxx recognizes it through `ExternType`.
fn value_type(type_ref: &TypeRef, cpp_name: &str) -> Result<TokenStream, CodegenError> {
    let name = ident(&type_ref.rust_name)?;
    let rename = cxx_name_attr(&type_ref.rust_name, cpp_name);
    // `prettyplease` cannot print `type X = super::X;` in an `extern` block: a marked
    // `type X;` is generated and turned into the alias after formatting (`with_type_aliases`).
    Ok(quote! {
        #[doc = #ALIAS_MARKER]
        #rename
        type #name;
    })
}

/// The marker of [`value_type`]; it does not stay in the generated file.
const ALIAS_MARKER: &str = "__ferrobridge_value_type__";

/// Turns the marked `type X;` lines into `type X = super::X;` and removes the marker.
fn with_type_aliases(code: &str) -> String {
    // `prettyplease` writes `#[doc = "..."]` as a `///` comment.
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

/// The receiver of a method.
struct Receiver<'a> {
    name: &'static str,
    class: &'a TypeRef,
    mutable: bool,
    /// Is it a value type (a mutable receiver is a plain `&mut`, not `Pin`)?
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
    // Shims are free functions; the `self` lifetime rule does not apply. The returned reference is
    // tied to the receiver with an explicit lifetime (cxx requires this if there is another reference parameter).
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

/// `#[cxx_name = "..."]` if the Rust name differs from the C++ name.
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

/// One `unsafe extern "C++"` block per namespace, in order of first appearance.
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
