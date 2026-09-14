use darling::{Error, FromMeta, Result};
use proc_macro2::Span;
use syn::parse::{Parse, ParseStream, Parser};
use syn::spanned::Spanned;
use syn::token::{Bracket, Pound};
use syn::{AttrStyle, Attribute, Lit, Meta, Path, Token};

use super::utils::is_optionize;

#[derive(Debug)]
enum Item {
    All,
    Include(Path),
    Exclude(Path),
    Attribute(Meta),
}

impl Parse for Item {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(Token![..]) {
            input.parse::<Token![..]>()?;
            Ok(Self::All)
        } else if input.peek(Token![+]) || input.peek(Token![-]) {
            let include = if input.peek(Token![+]) {
                input.parse::<Token![+]>()?;
                true
            } else {
                input.parse::<Token![-]>()?;
                false
            };
            let path = input.call(Path::parse_mod_style)?;
            if !input.is_empty() && !input.peek(Token![,]) {
                return Err(input.error("attribute selectors accept only a path"));
            }
            Ok(if include {
                Self::Include(path)
            } else {
                Self::Exclude(path)
            })
        } else {
            input.parse().map(Self::Attribute)
        }
    }
}

#[derive(Debug)]
struct AttributeList {
    items: Vec<Item>,
    span: Span,
}

impl FromMeta for AttributeList {
    fn from_meta(meta: &Meta) -> Result<Self> {
        (match meta {
            Meta::List(meta) => {
                let mut errors = Error::accumulator();
                let attributes = (|input: ParseStream<'_>| {
                    let mut attributes = Vec::new();
                    while !input.is_empty() {
                        if input.peek(Lit) {
                            let literal = input.parse::<Lit>()?;
                            errors.push(Error::unsupported_format("literal").with_span(&literal));
                        } else {
                            attributes.push(input.parse()?);
                        }
                        if !input.is_empty() {
                            input.parse::<Token![,]>()?;
                        }
                    }
                    Ok(Self {
                        items: attributes,
                        span: meta.span(),
                    })
                })
                .parse2(meta.tokens.clone());
                errors.finish_with(attributes)?.map_err(Error::from)
            }
            Meta::Path(_) => Self::from_word(),
            Meta::NameValue(meta) => Self::from_expr(&meta.value),
        })
        .map_err(|error| error.with_span(meta))
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct Attributes {
    #[darling(rename = "attrs", multiple)]
    attributes: Vec<AttributeList>,
}

impl Attributes {
    pub(super) fn span(&self) -> Option<Span> {
        self.attributes
            .iter()
            .map(|attributes| attributes.span)
            .reduce(|span, other| span.join(other).unwrap_or(other))
    }

    pub(super) fn patch(self, attrs: &mut Vec<Attribute>) {
        let mut all = self.attributes.is_empty();
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        let mut added = Vec::new();
        for item in self.attributes.into_iter().flat_map(|list| list.items) {
            match item {
                Item::All => all = true,
                Item::Include(path) => include.push(path),
                Item::Exclude(path) => exclude.push(path),
                Item::Attribute(meta) => {
                    let span = meta.span();
                    added.push(Attribute {
                        pound_token: Pound(span),
                        style: AttrStyle::Outer,
                        bracket_token: Bracket(span),
                        meta,
                    });
                }
            }
        }
        attrs.retain(|attr| {
            !is_optionize(attr)
                && (all || include.contains(attr.path()))
                && !exclude.contains(attr.path())
        });
        attrs.extend(added);
    }
}

#[cfg(test)]
mod tests {
    use darling::ast::NestedMeta;
    use proc_macro2::TokenStream;
    use quote::quote;

    use super::*;

    fn patch(original: TokenStream, options: TokenStream) -> TokenStream {
        let mut attrs = Attribute::parse_outer.parse2(original).unwrap();
        Attributes::from_list(&NestedMeta::parse_meta_list(options).unwrap())
            .unwrap()
            .patch(&mut attrs);
        quote!(#(#attrs)*)
    }

    #[test]
    fn attributes_select_originals_once_and_append_after_exclusions() {
        let output = patch(
            quote! {
                #[doc = "first"]
                #[derive(Clone, Debug)]
                #[doc = "second"]
                #[must_use]
                #[optionize(attrs(..))]
            },
            quote! {
                attrs(derive(Default), .., -derive),
                attrs(),
                attrs(+doc, +doc, doc = "added"),
            },
        );
        assert_eq!(
            output.to_string(),
            quote! {
                #[doc = "first"]
                #[doc = "second"]
                #[must_use]
                #[derive(Default)]
                #[doc = "added"]
            }
            .to_string()
        );
    }

    #[test]
    fn attributes_match_full_paths_and_preserve_duplicate_originals() {
        let original = quote! {
            #[tool::flag(first)]
            #[flag(second)]
            #[::tool::flag(third)]
            #[tool::flag(first)]
        };
        let output = patch(original.clone(), quote!(attrs(+tool::flag, +missing)));
        assert_eq!(
            output.to_string(),
            quote!(#[tool::flag(first)] #[tool::flag(first)]).to_string()
        );
        let output = patch(original, quote!(attrs(.., -tool::flag)));
        assert_eq!(
            output.to_string(),
            quote!(#[flag(second)] #[::tool::flag(third)]).to_string()
        );
    }

    #[test]
    fn attributes_select_in_original_order_and_never_inherit_helpers() {
        let output = patch(
            quote! {
                #[doc = "first"]
                #[must_use]
                #[optionize(attrs(+optionize))]
                #[doc = "last"]
            },
            quote!(attrs(+must_use, +doc, +optionize, -missing)),
        );
        assert_eq!(
            output.to_string(),
            quote!(#[doc = "first"] #[must_use] #[doc = "last"]).to_string()
        );
    }

    #[test]
    fn attributes_treat_unprefixed_names_and_nested_tokens_as_opaque() {
        let output = patch(
            quote!(#[doc = "removed"]),
            quote! {
                attrs(except(prost), inherit(doc), attrs_remove(prost)),
                attrs(custom(+doc, -derive, ..), derive(Clone, Debug)),
            },
        );
        assert_eq!(
            output.to_string(),
            quote! {
                #[except(prost)]
                #[inherit(doc)]
                #[attrs_remove(prost)]
                #[custom(+doc, -derive, ..)]
                #[derive(Clone, Debug)]
            }
            .to_string()
        );
    }

    #[test]
    fn attributes_do_not_implicitly_inherit_or_deduplicate_additions() {
        let output = patch(
            quote!(#[doc = "removed"]),
            quote!(attrs(-missing, doc = "added", doc = "added")),
        );
        assert_eq!(
            output.to_string(),
            quote!(#[doc = "added"] #[doc = "added"]).to_string()
        );
    }

    #[test]
    fn attributes_reject_malformed_selectors_and_accumulate_literal_errors() {
        for options in [
            quote!(attrs(+)),
            quote!(attrs(-)),
            quote!(attrs(+derive(Clone))),
            quote!(attrs(-derive(Clone))),
            quote!(attrs(+doc = "text")),
            quote!(attrs(+doc -derive)),
            quote!(attrs(...)),
        ] {
            assert!(
                Attributes::from_list(&NestedMeta::parse_meta_list(options.clone()).unwrap())
                    .is_err(),
                "accepted {options}"
            );
        }
        let errors =
            Attributes::from_list(&NestedMeta::parse_meta_list(quote!(attrs("first", 2))).unwrap())
                .unwrap_err()
                .flatten()
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|error| error.contains("literal")));
    }
}
