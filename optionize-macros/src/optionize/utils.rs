use darling::{Error, Result};
use proc_macro2::{Ident, Span, TokenStream};
use quote::format_ident;
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::{Attribute, LitStr, Member, Meta, Token, WhereClause, WherePredicate};

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

/// Whether an attribute deprecates its item, or allows or expects the
/// `deprecated` lint on it.
pub(super) fn is_deprecation(attr: &Attribute) -> bool {
    let path = attr.path();
    path.segments
        .last()
        .is_some_and(|s| s.ident == "deprecated")
        || ((path.is_ident("allow") || path.is_ident("expect"))
            && attr
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .is_ok_and(|lints| lints.iter().any(|lint| lint.path().is_ident("deprecated"))))
}

pub(super) fn member_to_string(member: &Member) -> String {
    match member {
        Member::Named(ident) => ident.unraw().to_string(),
        Member::Unnamed(index) => index.index.to_string(),
    }
}

pub(super) fn extend_where_clause(
    where_clause: &mut WhereClause,
    predicates: impl IntoIterator<Item = WherePredicate>,
) {
    let mut seen = where_clause
        .predicates
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    where_clause.predicates.extend(
        predicates
            .into_iter()
            .filter(|predicate| seen.insert(predicate.clone())),
    );
}

pub(super) fn collect_idents(tokens: TokenStream, idents: &mut HashSet<String>) {
    for token in tokens {
        match token {
            proc_macro2::TokenTree::Ident(ident) => {
                idents.insert(ident.unraw().to_string());
            }
            proc_macro2::TokenTree::Group(group) => collect_idents(group.stream(), idents),
            _ => {}
        }
    }
}

pub(super) fn new_ident(name: &str, idents: &HashSet<String>) -> Ident {
    let (mut ident, mut i) = (name.to_owned(), 0);
    while idents.contains(&ident) {
        ident = format!("{name}{i:X}");
        i += 1;
    }
    format_ident!("{ident}", span = Span::mixed_site())
}

// Tokens written by this macro only borrow the location of `$span`, so
// diagnostics still point at the user's code. They keep the macro's context:
// they follow this crate's edition whatever the caller's is, and lints treat
// them as generated code. Interpolated user tokens keep their own spans.
macro_rules! qs {
    ($span:expr => $($tt:tt)*) => {
        ::quote::quote_spanned! { ::proc_macro2::Span::mixed_site().located_at($span) => $($tt)* }
    };
}

macro_rules! pqs {
    ($span:expr => $($tt:tt)*) => {
        ::syn::parse_quote_spanned! { ::proc_macro2::Span::mixed_site().located_at($span) => $($tt)* }
    };
}

macro_rules! span {
    ($span:expr) => {
        $crate::optionize::utils::span!(@impl $span, $)
    };

    (@impl $span:expr, $_:tt) => {
        #[allow(unused_macros)]
        macro_rules! q {
            ($_($_ tt:tt)*) => {
                $crate::optionize::utils::qs! { $span => $_($_ tt)* }
            };
        }
        #[allow(unused_macros)]
        macro_rules! pq {
            ($_($_ tt:tt)*) => {
                $crate::optionize::utils::pqs! { $span => $_($_ tt)* }
            };
        }
    };
}

pub(super) use {pqs, qs, span};
