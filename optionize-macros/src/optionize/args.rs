use darling::util::{Flag, Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use derive_more::Deref;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident};
use syn::spanned::Spanned;
use syn::{Expr, Lit, LitStr, Path, TypePath, parse_quote};

use super::attrs::Attributes;
use super::utils::format;

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
        self.name.is_some() || self.attrs.span().is_some()
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

impl ToTokens for TypeArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Str(value) => value.to_tokens(tokens),
            Self::Path(path) => path.to_tokens(tokens),
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

        if let (Some(subject), Some(object)) = (&self.subject, &self.object) {
            let span = subject.span();
            let span = span.join(object.span()).unwrap_or(span);
            errors
                .push(Error::custom("`subject` and `object` cannot be combined").with_span(&span));
        }

        if self.object.is_some() || self.subject.is_some() {
            if let Some(name) = &self.name {
                errors.push(
                    Error::custom("`name` cannot be used when `object` or `subject` is specified")
                        .with_span(name),
                );
            }

            if let Some(span) = self.attrs.span() {
                errors.push(
                    Error::custom("`attrs` cannot be used when `object` or `subject` is specified")
                        .with_span(&span),
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

#[derive(Debug, Default, Deref, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
pub(super) struct FieldArgs {
    #[deref]
    #[darling(flatten)]
    pub(super) general: GeneralArgs,
    pub(super) flatten: Flag,
    pub(super) nest: Option<TypeArg>,
    #[darling(rename = "as")]
    pub(super) r#as: Option<TypeArg>,
    pub(super) skip: Flag,
    pub(super) default: Option<SpannedValue<Override<Expr>>>,
}

impl FieldArgs {
    fn finalize(self) -> Result<Self> {
        if self.skip.is_present()
            && (self.general.is_some()
                || self.flatten.is_present()
                || self.nest.is_some()
                || self.r#as.is_some())
        {
            return Err(Error::custom("`skip` can only be combined with `default`")
                .with_span(&self.skip.span()));
        }

        if self.flatten.is_present()
            && let Some(default) = &self.default
        {
            return Err(
                Error::custom("`default` cannot be used with `flatten`").with_span(&default.span())
            );
        }

        if let Some(convert) = &self.r#as
            && self.nest.is_some()
        {
            return Err(Error::custom("`as` cannot be used with `nest`").with_span(&convert.span()));
        }

        Ok(self)
    }
}
