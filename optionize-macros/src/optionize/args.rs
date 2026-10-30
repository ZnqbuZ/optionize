use darling::util::{Flag, Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use derive_more::Deref;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Expr, ExprClosure, ExprPath, Lit, LitStr, Meta, Path, Token, Type, TypePath, parse_quote,
    parse2,
};

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

/// A type written directly or as a string, read with the `Parsed` grammar.
#[derive(Debug)]
pub(super) enum TypeArg<Parsed = TypePath> {
    Str(LitStr),
    Direct(Parsed),
}

impl<Parsed: Parse> FromMeta for TypeArg<Parsed> {
    fn from_expr(expr: &Expr) -> Result<Self> {
        match expr {
            Expr::Lit(lit) if let Lit::Str(value) = &lit.lit => Ok(Self::Str(value.clone())),
            Expr::Group(group) => Self::from_expr(&group.expr),
            // Attribute values parse as expressions; read their tokens again
            // with the grammar this argument expects.
            _ => parse2(expr.to_token_stream())
                .map(Self::Direct)
                .map_err(Error::from),
        }
    }
}

impl<Parsed: ToTokens> ToTokens for TypeArg<Parsed> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Str(value) => value.to_tokens(tokens),
            Self::Direct(parsed) => parsed.to_tokens(tokens),
        }
    }
}

impl<Parsed: Parse> TypeArg<Parsed> {
    pub(super) fn parse(self) -> Result<Parsed> {
        match self {
            Self::Str(value) => value.parse().map_err(Error::from),
            Self::Direct(parsed) => Ok(parsed),
        }
    }

    pub(super) fn format(self, ident: &Ident) -> Result<Parsed> {
        match self {
            Self::Str(pattern) => format(&pattern, ident),
            Self::Direct(parsed) => Ok(parsed),
        }
    }
}

/// A function written after `from`, `into`, or `default`.
///
/// Only two shapes are accepted, and the leading token decides which one: a
/// closure with exactly one parameter or a function path. Any other expression
/// is a syntax error at the point where the value starts, which keeps closure
/// modifiers (`async`, `move`, `const`, `for<...>`) out of the grammar rather
/// than giving each one a diagnostic.
#[derive(Debug)]
pub(super) enum FunctionArg {
    /// A non-capturing closure, coerced to a function pointer.
    Closure(ExprClosure),
    /// A function called through a function pointer.
    Function(ExprPath),
}

impl Parse for FunctionArg {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if !input.peek(Token![|]) {
            return Ok(Self::Function(input.parse()?));
        }
        let closure = input.parse::<ExprClosure>()?;
        if closure.inputs.len() != 1 {
            // With at least one parameter the diagnostic points at the
            // parameter list; `|| ...` has none, so the closure itself is
            // highlighted.
            let span = if closure.inputs.is_empty() {
                closure.span()
            } else {
                closure.inputs.span()
            };
            return Err(syn::Error::new(
                span,
                "closure must take exactly one parameter",
            ));
        }
        Ok(Self::Closure(closure))
    }
}

impl FromMeta for FunctionArg {
    fn from_expr(expr: &Expr) -> Result<Self> {
        match expr {
            // The contents of a string must satisfy the same grammar.
            Expr::Lit(lit) if let Lit::Str(value) = &lit.lit => Ok(value.parse()?),
            // Invisible groups appear when the attribute is forwarded through
            // a `macro_rules!` fragment.
            Expr::Group(group) => Self::from_expr(&group.expr),
            _ => Ok(parse2(expr.to_token_stream())?),
        }
    }
}

impl ToTokens for FunctionArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Closure(closure) => closure.to_tokens(tokens),
            Self::Function(path) => path.to_tokens(tokens),
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

/// The functions that convert a field with `as`, relative to the annotated
/// field: `from` converts the `as` type into the field's type, and `into`
/// converts the field's type into the `as` type.
#[derive(Debug)]
pub(super) struct WithArgs {
    pub(super) from: Option<FunctionArg>,
    pub(super) into: Option<FunctionArg>,
    /// The span of `with` itself, which errors about the whole list point at.
    pub(super) span: Span,
}

impl FromMeta for WithArgs {
    fn from_meta(meta: &Meta) -> Result<Self> {
        #[derive(Default, FromMeta)]
        #[darling(default)]
        struct Functions {
            from: Option<FunctionArg>,
            into: Option<FunctionArg>,
        }

        let Functions { from, into } = Functions::from_meta(meta)?;
        Ok(Self {
            from,
            into,
            span: meta.path().span(),
        })
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
    pub(super) r#as: Option<TypeArg<Type>>,
    pub(super) with: Option<WithArgs>,
    pub(super) skip: Flag,
    pub(super) default: Option<SpannedValue<Override<FunctionArg>>>,
}

impl FieldArgs {
    fn finalize(self) -> Result<Self> {
        if self.skip.is_present()
            && (self.general.is_some()
                || self.flatten.is_present()
                || self.nest.is_some()
                || self.r#as.is_some()
                || self.with.is_some())
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

        if let Some(with) = &self.with {
            if self.r#as.is_none() {
                return Err(Error::custom("`with` requires `as`").with_span(&with.span));
            }
            if with.from.is_none() && with.into.is_none() {
                return Err(Error::custom("`with` requires `from` or `into`").with_span(&with.span));
            }
        }

        Ok(self)
    }
}
