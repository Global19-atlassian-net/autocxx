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

use crate::conversion::{api::{FuncToConvert, ImplBlockDetails, UnanalyzedApi}, codegen_cpp::function_wrapper::{ArgumentConversion, FunctionWrapper, FunctionWrapperPayload}};
use crate::{
    conversion::ConvertError,
    conversion::{api::ApiDetail, codegen_cpp::AdditionalNeed},
    types::{make_ident, Namespace, TypeName},
};
use quote::quote;
use std::collections::{HashMap, HashSet};
use syn::{
    parse::Parser, parse_quote, punctuated::Punctuated, token::Unsafe, Attribute, FnArg,
    ForeignItem, ForeignItemFn, Ident, ImplItem, ItemImpl, LitStr, Pat, ReturnType, Type, TypePtr,
};

use super::{
    super::api::Use,
    overload_tracker::OverloadTracker,
    unqualify::{unqualify_params, unqualify_ret_type},
};

/// Ways in which the conversion of a given extern "C" mod can
/// have more global effects or require more global knowledge outside
/// of its immediate conversion.
pub(crate) trait ForeignModParseCallbacks {
    fn convert_boxed_type(
        &mut self,
        ty: Box<Type>,
        ns: &Namespace,
        convert_ptrs_to_reference: bool,
    ) -> Result<(Box<Type>, HashSet<TypeName>, bool), ConvertError>;
    fn is_pod(&self, ty: &TypeName) -> bool;
    fn add_api(&mut self, api: UnanalyzedApi);
    fn get_cxx_bridge_name(
        &mut self,
        type_name: Option<&str>,
        found_name: &str,
        ns: &Namespace,
    ) -> String;
    fn ok_to_use_rust_name(&mut self, rust_name: &str) -> bool;
    fn is_on_allowlist(&self, type_name: &TypeName) -> bool;
    fn avoid_generating_type(&self, type_name: &TypeName) -> bool;
    /// In the future, this will take details of the function
    /// we're generating, in order to determine whether it should be unsafe
    /// according to a more nuanced policy.
    fn should_be_unsafe(&self) -> bool;
}

/// Converts a given bindgen-generated 'mod' into suitable
/// cxx::bridge runes. In bindgen output, a given mod concerns
/// a specific C++ namespace.
pub(crate) struct ParseForeignMod {
    ns: Namespace,
    overload_tracker: OverloadTracker,
    // We mostly act upon the functions we see within the 'extern "C"'
    // block of bindgen output, but we can't actually do this until
    // we've seen the (possibly subsequent) 'impl' blocks so we can
    // deduce which functions are actually static methods. Hence
    // store them.
    funcs_to_convert: Vec<FuncToConvert>,
    // Evidence from 'impl' blocks about which of these items
    // may actually be methods (static or otherwise). Mapping from
    // function name to type name.
    method_receivers: HashMap<Ident, TypeName>,
}

impl ParseForeignMod {
    pub(crate) fn new(ns: Namespace) -> Self {
        Self {
            ns,
            overload_tracker: OverloadTracker::new(),
            funcs_to_convert: Vec::new(),
            method_receivers: HashMap::new(),
        }
    }

    /// Record information from foreign mod items encountered
    /// in bindgen output.
    pub(crate) fn convert_foreign_mod_items(
        &mut self,
        foreign_mod_items: Vec<ForeignItem>,
        virtual_this_type: Option<TypeName>,
    ) -> Result<(), ConvertError> {
        for i in foreign_mod_items {
            match i {
                ForeignItem::Fn(item) => {
                    self.funcs_to_convert.push(FuncToConvert {
                        item,
                        virtual_this_type: virtual_this_type.clone(),
                        self_ty: None,
                    });
                }
                _ => return Err(ConvertError::UnexpectedForeignItem),
            }
        }
        Ok(())
    }

    /// Record information from impl blocks encountered in bindgen
    /// output.
    pub(crate) fn convert_impl_items(&mut self, imp: ItemImpl) {
        let ty_id = match *imp.self_ty {
            Type::Path(typ) => typ.path.segments.last().unwrap().ident.clone(),
            _ => return,
        };
        for i in imp.items {
            if let ImplItem::Method(itm) = i {
                let effective_fun_name = if itm.sig.ident == "new" {
                    ty_id.clone()
                } else {
                    itm.sig.ident
                };
                self.method_receivers.insert(
                    effective_fun_name,
                    TypeName::new(&self.ns, &ty_id.to_string()),
                );
            }
        }
    }

    /// Indicate that all foreign mods and all impl blocks have been
    /// fed into us, and we should process that information to generate
    /// the resulting APIs.
    pub(crate) fn finished(
        &mut self,
        callbacks: &mut impl ForeignModParseCallbacks,
    ) -> Result<(), ConvertError> {
        while !self.funcs_to_convert.is_empty() {
            let mut fun = self.funcs_to_convert.remove(0);
            fun.self_ty = self.method_receivers.get(&fun.item.sig.ident).cloned();
            callbacks.add_api(UnanalyzedApi {
                ns: self.ns,
                id: fun.item.sig.ident.clone(),
                use_stmt: Use::Unused, // filled in later - TODO make all these compile-time safe
                deps: HashSet::new(), // filled in later
                id_for_allowlist: None, // filled in later
                additional_cpp: None, // filled in later
                detail: ApiDetail::Function { fun, analysis: () }
            })
        }
        Ok(())
    }
}
