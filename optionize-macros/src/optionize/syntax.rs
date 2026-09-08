use darling::{Error, Result};
use proc_macro2::{Ident, Span, TokenStream};
use quote::format_ident;
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::{Attribute, LitStr, Member};

pub(super) fn format<Parsed: syn::parse::Parse>(pattern: &LitStr, ident: &Ident) -> Result<Parsed> {
    let value = pattern.value().replace("{}", &ident.unraw().to_string());
    LitStr::new(&value, pattern.span())
        .parse()
        .map_err(Error::from)
}

pub(super) fn is_optionize(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "optionize")
}

pub(super) fn member_to_string(member: &Member) -> String {
    match member {
        Member::Named(ident) => ident.unraw().to_string(),
        Member::Unnamed(index) => index.index.to_string(),
    }
}

pub(super) fn collect_idents(tokens: TokenStream, names: &mut HashSet<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Ident(ident) => {
                names.insert(ident.unraw().to_string());
            }
            proc_macro2::TokenTree::Group(group) => collect_idents(group.stream(), names),
            _ => {}
        }
    }
}

pub(super) fn fresh_ident(prefix: &str, names: &HashSet<String>) -> Ident {
    let mut name = prefix.to_owned();
    while names.contains(&name) {
        name.push('_');
    }
    format_ident!("{name}", span = Span::mixed_site())
}

macro_rules! span {
    ($span:expr) => {
        $crate::optionize::syntax::span!(@impl $span, $)
    };

    (@impl $span:expr, $_:tt) => {
        #[allow(unused_macros)]
        macro_rules! q {
            ($_($_ tt:tt)*) => {
                ::quote::quote_spanned! { $span => $_($_ tt)* }
            };
        }
        #[allow(unused_macros)]
        macro_rules! pq {
            ($_($_ tt:tt)*) => {
                ::syn::parse_quote_spanned! { $span => $_($_ tt)* }
            };
        }
    };
}

pub(super) use span;
