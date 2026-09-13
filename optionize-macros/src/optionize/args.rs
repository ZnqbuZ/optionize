use darling::ast::NestedMeta;
use darling::util::{Flag, Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use derive_more::Deref;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident};
use std::mem::take;
use syn::spanned::Spanned;
use syn::token::{Bracket, Pound};
use syn::{AttrStyle, Attribute, Expr, Lit, LitStr, Meta, Path, TypePath, parse_quote};

use super::utils::{format, is_optionize};

#[derive(Debug, Default)]
struct MetaList(Vec<Meta>);

impl MetaList {
    fn merge(lists: &mut Vec<Self>) -> Option<SpannedValue<Vec<Attribute>>> {
        (!lists.is_empty()).then(|| {
            let mut span = Span::call_site();
            let attributes = take(lists)
                .into_iter()
                .flat_map(|ml| ml.0)
                .map(|meta| {
                    span = span.join(meta.span()).unwrap_or(meta.span());
                    let span = meta.span();
                    Attribute {
                        pound_token: Pound(span),
                        style: AttrStyle::Outer,
                        bracket_token: Bracket(span),
                        meta,
                    }
                })
                .collect();
            SpannedValue::new(attributes, span)
        })
    }
}

impl FromMeta for MetaList {
    fn from_list(items: &[NestedMeta]) -> Result<Self> {
        let mut errors = Error::accumulator();
        let metas = items
            .iter()
            .filter_map(|item| match item {
                NestedMeta::Meta(m) => Some(m.clone()),
                NestedMeta::Lit(l) => {
                    errors.push(Error::unsupported_format("literal").with_span(l));
                    None
                }
            })
            .collect();
        errors.finish_with(Self(metas))
    }
}

#[derive(Debug, Clone, FromMeta)]
#[darling(default)]
pub(super) struct Crate(Path);

impl Crate {
    pub(super) fn infer() -> Self {
        match crate_name("optionize") {
            Ok(FoundCrate::Name(name)) => {
                let name = format_ident!("{}", name);
                Self(parse_quote! { ::#name })
            }
            _ => Default::default(),
        }
    }
}

impl Default for Crate {
    fn default() -> Self {
        Self(parse_quote! { ::optionize })
    }
}

impl ToTokens for Crate {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default, and_then = "Self::finalize")]
pub(super) struct Attributes {
    #[doc(hidden)]
    #[darling(rename = "attrs", multiple)]
    _attributes: Vec<MetaList>,
    #[darling(skip)]
    attributes: Option<SpannedValue<Vec<Attribute>>>,
}

impl Attributes {
    fn finalize(mut self) -> Result<Self> {
        self.attributes = MetaList::merge(&mut self._attributes);
        Ok(self)
    }

    pub(super) fn patch(self, attrs: &mut Vec<Attribute>) {
        if let Some(attributes) = self.attributes {
            *attrs = attributes.into_inner();
        } else {
            attrs.retain(|attr| !is_optionize(attr));
        }
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct OptionizedArgs {
    #[darling(rename = "crate")]
    pub(super) krate: Option<Crate>,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct GeneralArgs {
    pub(super) name: Option<LitStr>,
    #[darling(flatten)]
    pub(super) attrs: Attributes,
}

impl GeneralArgs {
    fn is_some(&self) -> bool {
        self.name.is_some() || self.attrs.attributes.is_some()
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct MarkedArgs {
    pub(super) name: Option<Ident>,
    #[darling(flatten)]
    pub(super) attrs: Attributes,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct PartialArgs {
    pub(super) upgradable: Flag,
    pub(super) marked: Option<SpannedValue<Override<MarkedArgs>>>,
}

#[derive(Debug)]
pub(super) enum TypeArg {
    Str(LitStr),
    Path(TypePath),
}

impl FromMeta for TypeArg {
    fn from_expr(expr: &Expr) -> Result<Self> {
        match expr {
            Expr::Lit(lit) if let Lit::Str(value) = &lit.lit => Ok(Self::Str(value.clone())),
            Expr::Group(group) => Self::from_expr(&group.expr),
            _ => TypePath::from_expr(expr).map(Self::Path),
        }
    }
}

impl TypeArg {
    pub(super) fn parse(self) -> Result<TypePath> {
        match self {
            Self::Str(value) => value.parse().map_err(Error::from),
            Self::Path(path) => Ok(path),
        }
    }

    pub(super) fn format(self, ident: &Ident) -> Result<TypePath> {
        match self {
            Self::Str(pattern) => format(&pattern, ident),
            Self::Path(path) => Ok(path),
        }
    }
}

#[derive(Debug, Default, Deref, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
pub(super) struct StructArgs {
    #[deref]
    #[darling(flatten)]
    pub(super) general: GeneralArgs,
    pub(super) partial: Option<SpannedValue<Override<PartialArgs>>>,
    pub(super) object: Option<TypeArg>,
    pub(super) subject: Option<TypeArg>,
}

impl StructArgs {
    fn finalize(self) -> Result<Self> {
        let mut errors = Error::accumulator();

        if self.subject.is_some() && self.object.is_some() {
            errors.push(Error::custom("`subject` and `object` cannot be combined"));
        }

        if self.object.is_some() || self.subject.is_some() {
            if let Some(name) = &self.name {
                errors.push(
                    Error::custom("`name` cannot be used when `object` or `subject` is specified")
                        .with_span(name),
                );
            }

            if let Some(attrs) = &self.attrs.attributes {
                errors.push(
                    Error::custom("`attrs` cannot be used when `object` or `subject` is specified")
                        .with_span(&attrs.span()),
                );
            }
        }

        if self.object.is_some()
            && let Some(partial) = &self.partial
            && let Override::Explicit(partial) = &**partial
            && let Some(marked) = &partial.marked
        {
            errors.push(
                Error::custom("`marked` cannot be used when `object` is specified")
                    .with_span(&marked.span()),
            );
        }

        errors.finish_with(self)
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
pub(super) struct SkipArgs {
    pub(super) upgrade: Option<Expr>,
}

#[derive(Debug, Default, Deref, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
pub(super) struct FieldArgs {
    #[deref]
    #[darling(flatten)]
    pub(super) general: GeneralArgs,
    pub(super) flatten: Flag,
    pub(super) nest: Option<TypeArg>,
    pub(super) skip: Option<SpannedValue<Override<SkipArgs>>>,
}

impl FieldArgs {
    fn finalize(self) -> Result<Self> {
        if let Some(skip) = &self.skip
            && (self.general.is_some() || self.flatten.is_present() || self.nest.is_some())
        {
            return Err(
                Error::custom("`skip` attribute cannot be combined with other attributes")
                    .with_span(&skip.span()),
            );
        }

        Ok(self)
    }
}
