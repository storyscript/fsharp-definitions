// Copyright 2019 Ian Castleden
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::source_builder::SourceBuilder;

use super::{ast, ident_from_str, Ctxt};
use quote::{quote, ToTokens};

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, Lit, Meta, /* MetaList,*/ MetaNameValue, NestedMeta};

// This is a helper to allow us to parse attributes
struct AttrT(Vec<Attribute>);

impl Parse for AttrT {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(AttrT(input.call(Attribute::parse_outer)?))
    }
}

#[derive(Debug)]
pub struct Attrs {
    /// list of blocks of "/// " prefixed comment lines
    merged_line_comments: SourceBuilder,
    pub fs_type: Option<String>,
    pub ts_handler_name: Option<String>,
    pub ts_handler_return: Option<String>,
    pub ts_factory_name: Option<String>,
    pub ts_factory_return_name: Option<String>,
    pub ts_as: Option<syn::Type>,
}

#[inline]
fn path_to_str(path: &syn::Path) -> String {
    quote!(#path).to_string()
}

#[allow(unused)]
pub fn turbofish_check(v: &str) -> Result<TokenStream, String> {
    match v.parse::<proc_macro2::TokenStream>() {
        // just get LexError as error message... so make our own.
        Err(_) => Err(format!("Can't lex turbofish \"{}\"", v)),
        Ok(tokens) => match syn::parse2::<syn::DeriveInput>(quote!( struct S{ a:v#tokens} )) {
            Err(_) => Err(format!("Can't parse turbofish \"{}\"", v)),
            Ok(_) => Ok(tokens),
        },
    }
}
impl Attrs {
    pub fn new() -> Attrs {
        Attrs {
            merged_line_comments: SourceBuilder::default(),
            // turbofish: None,
            fs_type: None,
            ts_handler_name: None,
            ts_handler_return: None,
            ts_factory_name: None,
            ts_factory_return_name: None,
            ts_as: None, // isa: HashMap::new(),
        }
    }
    pub fn push_doc_comment(&mut self, attrs: &[Attribute]) {
        if self.merged_line_comments.is_not_empty() {
            self.merged_line_comments.ln_push("///");
        }
        self.merged_line_comments
            .push_source(Attrs::to_comments(&attrs));
    }

    pub fn to_comment_source(&self) -> SourceBuilder {
        self.merged_line_comments.clone()
    }

    fn to_comments(attrs: &[Attribute]) -> SourceBuilder {
        let doc_comments = attrs
            .iter()
            .filter_map(|attr| {
                if path_to_str(&attr.path) == "doc" {
                    attr.parse_meta().ok()
                } else {
                    None
                }
            })
            .filter_map(|attr| {
                use Lit::*;
                use Meta::*;
                if let NameValue(MetaNameValue { lit: Str(s), .. }) = attr {
                    let value = s.value();
                    let text = value
                        .trim_start_matches("//!")
                        .trim_start_matches("///")
                        .trim_start_matches("/*!")
                        .trim_start_matches("/**")
                        .trim_end_matches("*/")
                        .trim();
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut src = SourceBuilder::default();

        for merged_line in doc_comments {
            src.ln_push("/// ");
            src.push(merged_line.as_str());
        }

        src
    }

    fn err_msg<A: ToTokens>(&self, tokens: A, msg: String, ctxt: Option<&'_ Ctxt>) {
        if let Some(ctxt) = ctxt {
            ctxt.error_spanned_by(tokens, msg);
        } else {
            panic!("{}", msg)
        };
    }
    pub fn find_fsharp<'a>(
        attrs: &'a [Attribute],
        ctxt: Option<&'a Ctxt>,
    ) -> impl Iterator<Item = Meta> + 'a {
        use syn::Meta::*;
        use NestedMeta::*;

        fn err<A: quote::ToTokens>(tokens: A, msg: String, ctxt: Option<&'_ Ctxt>) {
            if let Some(ctxt) = ctxt {
                ctxt.error_spanned_by(tokens, format!("invalid fsharp syntax: {}", msg));
            } else {
                panic!("invalid fsharp syntax: {}", msg)
            };
        }

        attrs
            .iter()
            .filter_map(move |attr| match path_to_str(&attr.path).as_ref() {
                "fs" => match attr.parse_meta() {
                    Ok(v) => Some(v),
                    Err(msg) => {
                        err(attr, msg.to_string(), ctxt);
                        None
                    }
                },
                _ => None,
            })
            .filter_map(move |m| match m {
                List(l) => Some(l.nested),
                ref tokens => {
                    err(&m, quote!(#tokens).to_string(), ctxt);
                    None
                }
            })
            .flatten()
            .filter_map(move |m| match m {
                Meta(m) => Some(m),
                ref tokens => {
                    err(&m, quote!(#tokens).to_string(), ctxt);
                    None
                }
            })
    }
    pub fn push_attrs(&mut self, _struct_ident: &Ident, attrs: &[Attribute], ctxt: Option<&Ctxt>) {
        use syn::Meta::*;
        use Lit::*;
        // use NestedMeta::*;

        for attr in Self::find_fsharp(&attrs, ctxt) {
            match attr {
                // #[fs(handler_name = "HandleFooBar")]
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "handler_name") => {
                    self.ts_handler_name = Some(value.value())
                }
                // #[fs(handler_return = "boolean")]
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "handler_return") => {
                    self.ts_handler_return = Some(value.value())
                }
                // #[fs(factory_name = "FooBar")]
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "factory_name") => {
                    self.ts_factory_name = Some(value.value())
                }
                // #[fs(factory_return_name = "FooBar")]
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "factory_return_name") => {
                    self.ts_factory_return_name = Some(value.value())
                }
                ref i @ NameValue(..) | ref i @ List(..) | ref i @ Path(..) => {
                    self.err_msg(i, format!("unsupported option: {}", quote!(#i)), ctxt);
                }
            }
        }
    }
    pub fn push_field_attrs(
        &mut self,
        _struct_ident: &Ident,
        attrs: &[Attribute],
        ctxt: Option<&Ctxt>,
    ) {
        use syn::Meta::*;
        use Lit::*;
        // use NestedMeta::*;

        for attr in Self::find_fsharp(&attrs, ctxt) {
            match attr {
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "fs_type") => {
                    let v = value.value();

                    self.fs_type = Some(v);
                }
                NameValue(MetaNameValue {
                    ref path,
                    lit: Str(ref value),
                    ..
                }) if is_path_ident(path, "ts_as") => {
                    let v = value.value();
                    match syn::parse_str::<syn::Type>(&v) {
                        Ok(t) => self.ts_as = Some(t),
                        Err(..) => {
                            self.err_msg(
                                attr,
                                format!("ts_as: \"{}\" is not a valid rust type", v),
                                ctxt,
                            );
                        }
                    }
                    //
                }

                ref i @ NameValue(..) | ref i @ List(..) | ref i @ Path(..) => {
                    self.err_msg(i, format!("unsupported option: {}", quote!(#i)), ctxt);
                }
            }
        }
    }

    pub fn from_variant(variant: &ast::Variant) -> Attrs {
        let mut res = Self::new();
        res.push_doc_comment(&variant.original.attrs);
        res
    }

    pub fn from_field(field: &ast::Field, ctxt: Option<&Ctxt>) -> Attrs {
        let mut res = Self::new();
        if let Some(ref ident) = field.original.ident {
            res.push_field_attrs(ident, &field.original.attrs, ctxt);
        } else {
            let id = ident_from_str("unnamed");
            res.push_field_attrs(&id, &field.original.attrs, ctxt);
        }
        res.push_doc_comment(&field.original.attrs);
        res
    }
}

fn is_path_ident(path: &syn::Path, test: &str) -> bool {
    if let Some(ref ident) = path.get_ident() {
        format!("{}", ident) == *test
    } else {
        false
    }
}
