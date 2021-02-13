// Copyright 2020 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use quote::quote;
use syn::{
    parse::Parser, parse_quote, punctuated::Punctuated, token::Unsafe, Attribute, FnArg,
    ForeignItem, Ident, ImplItem, ReturnType,
};

use super::RsCodegenResult;
use crate::types::make_ident;
use crate::{
    conversion::{
        analysis::fun::{ArgumentAnalysis, FnAnalysisBody},
        api::{ImplBlockDetails, Use},
        parse::unqualify::{unqualify_params, unqualify_ret_type},
    },
    types::{Namespace, TypeName},
};

pub(crate) fn gen_function(ns: &Namespace, analysis: FnAnalysisBody) -> RsCodegenResult {
    let rename_using_rust_attr = analysis.rename_using_rust_attr;
    let cxxbridge_name = analysis.cxxbridge_name;
    let rust_name = analysis.rust_name;
    let self_ty = analysis.self_ty;
    let ret_type = analysis.ret_type;
    let is_constructor = analysis.is_constructor;
    let param_details = analysis.param_details;
    let cpp_call_name = analysis.cpp_call_name;
    let wrapper_function_needed = analysis.wrapper_function_needed;
    let requires_unsafe = analysis.requires_unsafe;
    let is_a_method = analysis.is_a_method;
    let params = analysis.params;
    let vis = analysis.vis;
    // CUT HERE

    let mut cpp_name_attr = Vec::new();
    let use_alias_required = None;
    let mut impl_entry = None;
    if cxxbridge_name != rust_name {
        if let Some(type_name) = &self_ty {
            // Method, or static method.
            impl_entry = Some(generate_method_impl(
                &param_details,
                is_constructor,
                type_name,
                &cxxbridge_name,
                &rust_name,
                &ret_type,
            ));
        }
    }
    let rust_name_attr: Vec<_> = if let Some(rust_name) = rename_using_rust_attr {
        Attribute::parse_outer
            .parse2(quote!(
                #[rust_name = #rust_name]
            ))
            .unwrap()
    } else {
        Vec::new()
    };
    if cxxbridge_name != cpp_call_name && !wrapper_function_needed {
        cpp_name_attr = Attribute::parse_outer
            .parse2(quote!(
                #[cxx_name = #cpp_call_name]
            ))
            .unwrap();
    }
    // Finally - namespace support. All the Types in everything
    // above this point are fully qualified. We need to unqualify them.
    // We need to do that _after_ the above wrapper_function_needed
    // work, because it relies upon spotting fully qualified names like
    // std::unique_ptr. However, after it's done its job, all such
    // well-known types should be unqualified already (e.g. just UniquePtr)
    // and the following code will act to unqualify only those types
    // which the user has declared.
    let params = unqualify_params(params);
    let ret_type = unqualify_ret_type(ret_type);
    // And we need to make an attribute for the namespace that the function
    // itself is in.
    let namespace_attr = if ns.is_empty() || wrapper_function_needed {
        Vec::new()
    } else {
        let namespace_string = ns.to_string();
        Attribute::parse_outer
            .parse2(quote!(
                #[namespace = #namespace_string]
            ))
            .unwrap()
    };
    // At last, actually generate the cxx::bridge entry.
    let unsafety: Option<Unsafe> = if requires_unsafe {
        Some(parse_quote!(unsafe))
    } else {
        None
    };
    let extern_c_mod_item = ForeignItem::Fn(parse_quote!(
        #(#namespace_attr)*
        #(#rust_name_attr)*
        #(#cpp_name_attr)*
        #vis #unsafety fn #cxxbridge_name ( #params ) #ret_type;
    ));
    let (id, use_stmt, id_for_allowlist) = if is_a_method {
        (
            make_ident(&rust_name),
            Use::Unused,
            self_ty.map(|ty| make_ident(ty.get_final_ident())),
        )
    } else {
        match use_alias_required {
            None => (make_ident(&rust_name), Use::Used, None),
            Some(alias) => (cxxbridge_name, Use::UsedWithAlias(alias), None),
        }
    };
    RsCodegenResult {
        extern_c_mod_item: Some(extern_c_mod_item),
        bridge_items: Vec::new(),
        global_items: Vec::new(),
        bindgen_mod_item: None,
        impl_entry,
    }
}

/// Generate an 'impl Type { methods-go-here }' item
fn generate_method_impl(
    param_details: &[ArgumentAnalysis],
    is_constructor: bool,
    impl_block_type_name: &TypeName,
    cxxbridge_name: &Ident,
    rust_name: &str,
    ret_type: &ReturnType,
) -> Box<ImplBlockDetails> {
    let mut wrapper_params: Punctuated<FnArg, syn::Token![,]> = Punctuated::new();
    let mut arg_list = Vec::new();
    for pd in param_details {
        let type_name = pd.conversion.converted_rust_type();
        let wrapper_arg_name = if pd.self_type.is_some() && !is_constructor {
            parse_quote!(self)
        } else {
            pd.name.clone()
        };
        wrapper_params.push(parse_quote!(
            #wrapper_arg_name: #type_name
        ));
        arg_list.push(wrapper_arg_name);
    }

    let rust_name = make_ident(&rust_name);
    Box::new(ImplBlockDetails {
        item: ImplItem::Method(parse_quote! {
            pub fn #rust_name ( #wrapper_params ) #ret_type {
                cxxbridge::#cxxbridge_name ( #(#arg_list),* )
            }
        }),
        ty: make_ident(impl_block_type_name.get_final_ident()),
    })
}
