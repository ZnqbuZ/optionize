use darling::ast::NestedMeta;
use darling::util::{Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use proc_macro2::{Ident, Span, TokenStream};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote_spanned as qs, ToTokens};
use std::collections::HashSet;
use std::default::Default;
use std::iter::zip;
use std::mem::take;
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Brace, Bracket, Comma, Paren, Pound};
use syn::{
    parse2, parse_quote, parse_quote_spanned as pqs, parse_str, AttrStyle, Attribute, Data, DeriveInput, Expr,
    Field, Fields, FieldsNamed, FieldsUnnamed, Index, LitStr, Meta, Path,
    Type, WherePredicate,
};

// region args

#[derive(Debug, Default)]
struct MetaList(Vec<Meta>);

impl MetaList {
    fn merge(lists: &mut Vec<Self>) -> Option<Vec<Attribute>> {
        (!lists.is_empty()).then(|| {
            take(lists)
                .into_iter()
                .flat_map(|ml| ml.0)
                .map(|meta| {
                    let span = meta.span();
                    Attribute {
                        pound_token: Pound(span),
                        style: AttrStyle::Outer,
                        bracket_token: Bracket(span),
                        meta,
                    }
                })
                .collect()
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

#[derive(Debug, Default, FromMeta)]
#[darling(default, and_then = "Self::finalize")]
struct AttributeArgs {
    #[doc(hidden)]
    #[darling(rename = "attrs", multiple)]
    _attributes: Vec<MetaList>,
    #[darling(skip)]
    attributes: Option<Vec<Attribute>>,
}

impl AttributeArgs {
    fn finalize(mut self) -> Result<Self> {
        self.attributes = MetaList::merge(&mut self._attributes);
        Ok(self)
    }

    fn patch(self, attrs: &mut Vec<Attribute>) {
        if let Some(attributes) = self.attributes {
            *attrs = attributes;
        } else {
            attrs.retain(|attr| !is_optionize(attr));
        }
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct GeneralArgs {
    name: Option<LitStr>,
    #[darling(flatten)]
    attrs: AttributeArgs,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct MarkedArgs {
    name: Option<Ident>,
    #[darling(flatten)]
    attrs: AttributeArgs,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct PartialArgs {
    upgradable: SpannedValue<bool>,
    marked: Option<SpannedValue<Override<MarkedArgs>>>,
}

#[derive(Debug, Clone, FromMeta)]
#[darling(default)]
struct Crate(Path);

impl Crate {
    fn infer() -> Self {
        match crate_name("optionize") {
            Ok(FoundCrate::Name(name)) => {
                let name = format_ident!("{}", name);
                Self(parse_quote! { ::#name })
            },
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

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
struct StructArgs {
    #[darling(flatten)]
    general: GeneralArgs,
    #[doc(hidden)]
    #[darling(rename = "crate", multiple)]
    _krate: Vec<Crate>,
    #[darling(skip)]
    krate: Crate,
    partial: Option<SpannedValue<Override<PartialArgs>>>,
}

impl StructArgs {
    fn finalize(mut self) -> Result<Self> {
        let Some(krate) = self._krate.last() else {
            return Err(Error::missing_field("crate"));
        };
        self.krate = krate.clone();
        Ok(self)
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct SkipArgs {
    upgrade: Option<Expr>,
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
struct FieldArgs {
    #[darling(flatten)]
    general: GeneralArgs,
    flatten: SpannedValue<bool>,
    nest: Option<Type>,
    skip: Option<SpannedValue<Override<SkipArgs>>>,
}

impl FieldArgs {
    fn finalize(self) -> Result<Self> {
        let mut errors = Error::accumulator();

        if let Some(skip) = &self.skip {
            let skip = skip.span();

            if *self.flatten {
                let flatten = self.flatten.span();
                errors.push(
                    Error::custom("`flatten` cannot be used with `skip`")
                        .with_span(&skip.join(flatten).unwrap_or(flatten)),
                );
            }
            if let Some(nest) = &self.nest {
                let nest = nest.span();
                errors.push(
                    Error::custom("`nest` cannot be used with `skip`")
                        .with_span(&skip.join(nest).unwrap_or(nest)),
                );
            }
        }

        errors.finish_with(self)
    }
}

// endregion

// region utils

fn format(pattern: &LitStr, ident: &Ident) -> Result<Ident> {
    let span = pattern.span();
    let ident = pattern.value().replace("{}", &ident.unraw().to_string());
    let mut ident = parse_str::<Ident>(&ident).map_err(|_| {
        Error::custom(format!("`{}` is not a valid identifier", ident)).with_span(&span)
    })?;
    ident.set_span(span);
    Ok(ident)
}

fn is_optionize(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "optionize")
}

macro_rules! span {
    ($span:expr) => {
        span!(@impl $span, $)
    };

    (@impl $span:expr, $_:tt) => {
        #[allow(unused_macros)]
        macro_rules! q {
            ($_($_ tt:tt)*) => {
                qs! { $span => $_($_ tt)* }
            };
        }
        #[allow(unused_macros)]
        macro_rules! pq {
            ($_($_ tt:tt)*) => {
                pqs! { $span => $_($_ tt)* }
            };
        }
    };
}

//endregion

#[derive(Debug)]
enum FieldStrategy {
    Skip { upgrade: Expr },
    Optionize { wrap: bool, nest: Option<Type> },
}

impl Default for FieldStrategy {
    fn default() -> Self {
        Self::Optionize {
            wrap: true,
            nest: None,
        }
    }
}

struct FieldIr {
    krate: Crate,
    ty: Type,
    span: Span,
    original: TokenStream,
    optionized: TokenStream,
    strategy: FieldStrategy,
    local: Ident,
}

impl Default for FieldIr {
    fn default() -> Self {
        Self {
            krate: Default::default(),
            ty: parse_quote!(()),
            span: Span::call_site(),
            original: Default::default(),
            optionized: Default::default(),
            strategy: Default::default(),
            local: format_ident!("_"),
        }
    }
}

impl FieldIr {
    fn extract(
        fields: &mut Punctuated<Field, Comma>,
        krate: Crate,
        partial: bool,
    ) -> Result<Vec<Self>> {
        let mut errors = Error::accumulator();

        let args = fields
            .iter_mut()
            .filter_map(|field| errors.handle(FieldArgs::from_attributes(&field.attrs)))
            .collect::<Vec<_>>();

        let mut this = Vec::new();
        let mut skipped = 0;

        for (i, (mut field, args)) in zip(take(fields), args).enumerate() {
            let ty = field.ty.clone();
            let ident = &field.ident;
            let span = {
                let ty = ty.span();
                ident.as_ref().map_or(ty, |ident| {
                    let ident = ident.span();
                    ty.join(ident).unwrap_or(ident)
                })
            };

            let _span = field
                .attrs
                .iter()
                .filter(|attr| is_optionize(attr))
                .map(|attr| attr.bracket_token.span.span())
                .reduce(|a, s| s.join(a).unwrap_or(a))
                .unwrap_or(span);
            span!(_span);

            let mut ir = {
                let mut local = if let Some(ident) = ident.clone() {
                    format_ident!("v_{}", ident)
                } else {
                    format_ident!("v_{}", i)
                };
                local.set_span(_span);

                let original = match ident {
                    Some(ident) => ident.to_token_stream(),
                    None => Index::from(i).to_token_stream(),
                };
                let original = qs! { span => #original };

                FieldIr {
                    krate: krate.clone(),
                    ty: ty.clone(),
                    span: _span,
                    original,
                    local,
                    ..Default::default()
                }
            };

            let (skip, upgrade) = match args.skip {
                Some(skip) => {
                    let span = skip.span();
                    let upgrade = if let Override::Explicit(s) = skip.into_inner() {
                        s.upgrade
                    } else {
                        None
                    };
                    (Some(span), upgrade)
                }
                None => (None, None),
            };

            if let Some(span) = skip {
                if !partial {
                    errors.push(
                        Error::custom(
                            "`skip` attribute is only allowed when `partial` is specified",
                        )
                        .with_span(&span),
                    );
                    continue;
                }

                ir.strategy = FieldStrategy::Skip {
                    upgrade: upgrade.unwrap_or_else(|| {
                        pq! { <#ty as ::core::default::Default>::default() }
                    }),
                };

                skipped += 1;
                this.push(ir);
                continue;
            }

            if let Some(name) = args.general.name {
                let Some(ident) = ident.as_ref() else {
                    errors.push(
                        Error::custom("`name` attribute cannot be used on unnamed fields")
                            .with_span(&name),
                    );
                    continue;
                };
                let ident = match format(&name, ident) {
                    Ok(ident) => ident,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                field.ident = Some(ident);
            }

            args.general.attrs.patch(&mut field.attrs);

            ir.optionized = match &field.ident {
                Some(ident) => ident.to_token_stream(),
                None => Index {
                    index: (i - skipped) as u32,
                    span: _span,
                }
                .to_token_stream(),
            };

            let wrap = !*args.flatten;
            let nest = args.nest;

            {
                let ty = nest.as_ref().unwrap_or(&ty);
                field.ty = if wrap {
                    pq! { ::core::option::Option<#ty> }
                } else {
                    ty.clone()
                };
            }

            ir.strategy = FieldStrategy::Optionize { wrap, nest };

            this.push(ir);
            fields.push(field);
        }

        errors.finish_with(this)
    }
}

// region codegen

macro_rules! expand {
    ($target:expr => { $($field:ident $(: $bind:pat)?),* $(,)? }) => {
        let FieldIr {
            span,
            $(
                $field $(: $bind)?,
            )*
            ..
        } = $target;

        span!(*span);
    };
}

impl FieldIr {
    fn partial_optionized_where(&self) -> Vec<WherePredicate> {
        expand! {
            self => {
                krate,
                ty,
                strategy,
            }
        }

        if let FieldStrategy::Optionize {
            nest: Some(nest), ..
        } = &strategy
        {
            vec![pq! {
                #nest: #krate::PartialOptionized::<#ty>
            }]
        } else {
            Default::default()
        }
    }

    fn optionized_where(&self) -> Vec<WherePredicate> {
        expand! {
            self => {
                krate,
                ty,
                strategy,
            }
        }

        if let FieldStrategy::Optionize {
            nest: Some(nest), ..
        } = &strategy
        {
            vec![
                pq! {
                    #nest: #krate::Optionized::<#ty>
                },
                pq! {
                    <#nest as #krate::Optionized::<#ty>>::UpgradeErrors: 'static
                },
            ]
        } else {
            Default::default()
        }
    }
}

struct Optionize<'l> {
    field: &'l FieldIr,
    subject: &'l Ident,
    named: bool,
}

impl<'l> ToTokens for Optionize<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
            }
        }

        let subject = self.subject;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        let nest = nest.is_some();

        let mut optionize = if nest {
            q! { #krate::PartialOptionized::<#ty>::optionize(#subject.#original) }
        } else {
            q! { #subject.#original }
        };

        if *wrap {
            optionize = q! { ::core::option::Option::Some(#optionize) }
        };

        let optionize = if self.named {
            q! { #optionized: #optionize, }
        } else {
            q! { #optionize, }
        };

        tokens.extend(optionize);
    }
}

struct Patch<'l> {
    field: &'l FieldIr,
    subject: &'l Ident,
}

impl<'l> ToTokens for Patch<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
            }
        }

        let subject = self.subject;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        let nest = nest.is_some();

        let patch = if *wrap {
            q! { v }
        } else {
            q! { self.#optionized }
        };
        let mut patch = if nest {
            q! { #krate::PartialOptionized::<#ty>::patch(#patch, &mut #subject.#original); }
        } else {
            q! { #subject.#original = #patch; }
        };
        if *wrap {
            patch = q! {
                if let ::core::option::Option::Some(v) = self.#optionized {
                    #patch
                }
            }
        };

        tokens.extend(patch);
    }
}

struct Merge<'l> {
    field: &'l FieldIr,
    other: &'l Ident,
}

impl<'l> ToTokens for Merge<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                optionized,
                strategy,
            }
        }

        let other = self.other;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        let nest = nest.is_some();

        let merge = match (wrap, nest) {
            (true, true) => q! {
                match (&mut self.#optionized, #other.#optionized) {
                    (Some(this), Some(other)) => #krate::PartialOptionized::<#ty>::merge(this, other),
                    (None, Some(other)) => self.#optionized = Some(other),
                    _ => {}
                }
            },
            (true, false) => q! {
                if ::core::option::Option::is_some(&#other.#optionized) {
                    self.#optionized = #other.#optionized;
                }
            },
            (false, true) => q! {
                #krate::PartialOptionized::<#ty>::merge(&mut self.#optionized, #other.#optionized);
            },
            (false, false) => q! {
                self.#optionized = #other.#optionized;
            },
        };

        tokens.extend(merge);
    }
}

struct Upgrade<'l> {
    field: &'l FieldIr,
    original: &'l Ident,
    optionized: &'l Ident,
    failed: &'l Ident,
    errors: &'l Ident,
}

impl<'l> ToTokens for Upgrade<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
                local,
            }
        }

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        let nest = nest.is_some();

        let original_str = original.to_string();
        let optionized_str = optionized.to_string();

        let renamed = original_str == optionized_str;

        let (missing_err, nest_map_err) = {
            let ty = {
                let original_str = self.original.to_string();
                let optionized_str = self.optionized.to_string();

                q! {
                    #krate::TypeInfo {
                        original: #original_str,
                        optionized: #optionized_str,
                    }
                }
            };

            let field = if renamed {
                q! { #krate::FieldInfo::Identical ( #original_str ) }
            } else {
                q! { #krate::FieldInfo::Renamed { original: #original_str, optionized: #optionized_str } }
            };

            (
                q! {
                    #krate::UpgradeError::MissingField {
                        ty: #ty,
                        field: #field
                    }
                },
                q! {
                    |e| #krate::UpgradeError::NestedError {
                        ty: #ty,
                        field: #field,
                        source: #krate::__private::alloc::boxed::Box::new(e) as _
                    }
                },
            )
        };

        let failed = self.failed;
        let errors = self.errors;

        tokens.extend(q! { let #local = self.#optionized; });

        let mut expr = if nest {
            let err = if *wrap {
                q!(::core::option::Option::Some(v))
            } else {
                q!(v)
            };
            q! {
                #krate::Optionized::<#ty>::upgrade(#local).map_err(|(e, v)| {
                    #failed = true;
                    #errors.extend(::core::iter::IntoIterator::into_iter(e).map(#nest_map_err));
                    #err
                })
            }
        } else {
            q! { ::core::result::Result::Ok(#local) }
        };

        if *wrap {
            expr = q! {
                match #local {
                    ::core::option::Option::Some(#local) => #expr,
                    ::core::option::Option::None => {
                        #failed = true;
                        #errors.push(#missing_err);
                        ::core::result::Result::Err(::core::option::Option::None)
                    }
                }
            };
        }

        tokens.extend(q! { let #local = #expr; });
    }
}

struct UpgradeSkip<'l> {
    field: &'l FieldIr,
}

impl<'l> ToTokens for UpgradeSkip<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                ty,
                strategy,
                local,
            }
        }

        if let FieldStrategy::Skip { upgrade } = strategy {
            tokens.extend(q! { let #local: #ty = { #upgrade }; });
        }
    }
}

struct UpgradeOk<'l> {
    field: &'l FieldIr,
    named: bool,
}

impl<'l> ToTokens for UpgradeOk<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                original,
                strategy,
                local,
            }
        }

        let local = match strategy {
            FieldStrategy::Skip { .. } => {
                q! { #local }
            }
            FieldStrategy::Optionize { .. } => {
                q! { ::core::result::Result::unwrap_or_else(#local, |_| ::core::unreachable!()) }
            }
        };

        let ok = if self.named {
            q! { #original: #local, }
        } else {
            q! { #local, }
        };

        tokens.extend(ok);
    }
}

struct UpgradeErr<'l> {
    field: &'l FieldIr,
    named: bool,
}

impl<'l> ToTokens for UpgradeErr<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                optionized,
                strategy,
                local,
            }
        }

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        let nest = nest.is_some();

        let mut ok = q! { v };
        if nest {
            ok = q! { #krate::PartialOptionized::<#ty>::optionize(#ok) };
        }
        if *wrap {
            ok = q! { ::core::option::Option::Some(#ok) };
        }
        let rollback = q! {
            match #local {
                ::core::result::Result::Ok(v) => #ok,
                ::core::result::Result::Err(v) => v,
            }
        };

        let err = if self.named {
            q! { #optionized: #rollback, }
        } else {
            q! { #rollback, }
        };

        tokens.extend(err);
    }
}

// endregion

#[derive(Debug, Clone, Copy)]
enum StructStyle {
    Named,
    Unnamed,
    Unit,
}

pub fn derive(input: TokenStream) -> Result<TokenStream> {
    let original = parse2::<DeriveInput>(input)?;
    let _span = original.span();
    span!(_span);

    macro_rules! construct {
        ($style:expr, $span:expr => [$($ty:tt)+] $($fields:tt)*) => {
            match $style {
                StructStyle::Named => qs! { $span => $($ty)* { $($fields)* } },
                StructStyle::Unnamed => qs! { $span => $($ty)* ( $($fields)* ) },
                StructStyle::Unit => qs! { $span => $($ty)* },
            }
        };
    }

    let args = StructArgs::from_attributes(&original.attrs)?;

    let krate = args.krate;

    let (partial, upgradable, marked) = args
        .partial
        .map(|partial| {
            let span = partial.span();
            let (upgradable, marked) = match partial.into_inner() {
                Override::Explicit(p) => (p.upgradable.then(|| p.upgradable.span()), p.marked),
                _ => Default::default(),
            };
            (Some(span), upgradable, marked)
        })
        .unwrap_or_default();

    let mut optionized = original;
    let original = &optionized.ident.clone();

    optionized.ident = match args.general.name {
        Some(name) => format(&name, original)?,
        None => format(&pqs! { original.span() => "{}Optional"}, original)?,
    };

    args.general.attrs.patch(&mut optionized.attrs);

    let (impl_generics, type_generics, where_clause) = optionized.generics.split_for_impl();
    #[allow(non_snake_case)]
    let Subject = q! { #original #type_generics };

    let data = match &mut optionized.data {
        Data::Struct(data) => data,
        _ => {
            return Err(
                Error::custom("Optionize can only be derived for structs").with_span(&_span)
            );
        }
    };

    let original_style = match &data.fields {
        Fields::Named(_) => StructStyle::Named,
        Fields::Unnamed(_) => StructStyle::Unnamed,
        Fields::Unit => StructStyle::Unit,
    };
    let optionized_style = if matches!(original_style, StructStyle::Unit)
        && let Some(marked) = &marked
    {
        let span = marked.span();
        let punctuated = Default::default();
        if let Override::Explicit(marked) = marked.as_ref()
            && marked.name.is_some()
        {
            data.fields = Fields::Named(FieldsNamed {
                brace_token: Brace(span),
                named: punctuated,
            });
            StructStyle::Named
        } else {
            data.fields = Fields::Unnamed(FieldsUnnamed {
                paren_token: Paren(span),
                unnamed: punctuated,
            });
            StructStyle::Unnamed
        }
    } else {
        original_style
    };

    let fields = match &mut data.fields {
        Fields::Named(fields) => &mut fields.named,
        Fields::Unnamed(fields) => &mut fields.unnamed,
        Fields::Unit => &mut Default::default(),
    };

    let original_fields = FieldIr::extract(fields, krate.clone(), partial.is_some())?;
    let optionized_fields = original_fields
        .iter()
        .filter(|f| matches!(f.strategy, FieldStrategy::Optionize { .. }))
        .collect::<Vec<_>>();

    let marker = if let Some(marked) = marked {
        let span = marked.span();
        let marked = marked.into_inner().unwrap_or_default();

        let mut attrs = vec![pqs! { span => #[doc(hidden)] }];
        marked.attrs.patch(&mut attrs);

        let ident = match (original_style, marked.name) {
            (StructStyle::Named, None) => {
                let names = fields
                    .iter()
                    .filter_map(|f| f.ident.as_ref())
                    .map(|i| i.to_string())
                    .collect::<HashSet<_>>();
                let mut ident = "_marker".to_owned();
                while names.contains(&ident) {
                    ident.insert(0, '_');
                }
                Some(format_ident!("{}", ident, span = span))
            }
            (StructStyle::Unnamed, Some(name)) => {
                return Err(
                    Error::custom("`name` attribute cannot be used on unnamed structs")
                        .with_span(&name),
                );
            }
            (_, Some(name)) => Some(name),
            _ => None,
        };

        let (marker, field) = if let Some(ident) = ident {
            (
                qs! { ident.span() => #ident: ::core::marker::PhantomData, },
                pqs! { span =>
                    #(#attrs)*
                    pub #ident: ::core::marker::PhantomData<#Subject>
                },
            )
        } else {
            (
                qs! { span => ::core::marker::PhantomData, },
                pqs! { span =>
                    #(#attrs)*
                    ::core::marker::PhantomData<#Subject>
                },
            )
        };

        fields.push(field);
        Some(marker)
    } else {
        None
    };

    let mut output = vec![q! { #optionized }];
    let optionized = &optionized.ident;

    let named = matches!(original_style, StructStyle::Named);

    let mut where_clause = where_clause.cloned().unwrap_or_else(|| pq! { where });

    {
        where_clause.predicates.extend(
            optionized_fields
                .iter()
                .copied()
                .flat_map(FieldIr::partial_optionized_where),
        );

        let subject = &format_ident!("subject");

        let optionize = {
            let optionizes = optionized_fields.iter().map(|field| Optionize {
                field,
                subject,
                named,
            });

            construct!(optionized_style, _span => [Self] #(#optionizes)* #marker )
        };
        let patches = optionized_fields
            .iter()
            .map(|field| Patch { field, subject });
        let other = &format_ident!("other");
        let merges = optionized_fields.iter().map(|field| Merge { field, other });

        output.push(q! {
            impl #impl_generics #krate::PartialOptionized<#Subject> for #optionized #type_generics #where_clause {
                fn optionize(#subject: #Subject) -> Self { #optionize }
                fn patch(self, #subject: &mut #Subject) { #(#patches)* }
                fn merge(&mut self, #other: Self) { #(#merges)* }
            }
        });
    }

    let span = if partial.is_none() {
        Some(_span)
    } else {
        upgradable
    };

    if let Some(span) = span {
        where_clause.predicates.extend(
            optionized_fields
                .iter()
                .copied()
                .flat_map(FieldIr::optionized_where),
        );

        let failed = &format_ident!("failed");
        let errors = &format_ident!("errors");

        let skips = original_fields.iter().map(|field| UpgradeSkip { field });

        let upgrades = optionized_fields.iter().map(|field| Upgrade {
            field,
            original,
            optionized,
            failed,
            errors,
        });

        let ok = {
            let oks = original_fields
                .iter()
                .map(|field| UpgradeOk { field, named });

            construct!(original_style, span => [#original] #(#oks)*)
        };

        let err = {
            let errs = optionized_fields
                .iter()
                .map(|field| UpgradeErr { field, named });

            construct!(optionized_style, span => [Self] #(#errs)* #marker)
        };

        output.push(qs! { span =>
            #[allow(non_snake_case)]
            impl #impl_generics #krate::Optionized<#Subject> for #optionized #type_generics #where_clause {
                type UpgradeErrors = #krate::UpgradeErrorCollection;
                fn upgrade(self) -> ::core::result::Result<#Subject, (Self::UpgradeErrors, Self)> {
                    let mut #failed = false;
                    let mut #errors = #krate::UpgradeErrorCollection::default();
                    #(#skips)*
                    #(#upgrades)*
                    if !#failed {
                        ::core::result::Result::Ok(#ok)
                    } else {
                        ::core::result::Result::Err((#errors, #err))
                    }
                }
            }
        });
    }

    Ok(q! { #(#output)* })
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct OptionizedArgs {
    #[darling(rename = "crate")]
    krate: Option<Crate>,
}

pub fn proc(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let args = OptionizedArgs::from_list(&NestedMeta::parse_meta_list(args)?)?;

    let krate = args.krate.unwrap_or_else(Crate::infer);

    let output = qs! { input.span() =>
        #[derive(#krate::__private::Optionize)]
        #[optionize(crate = #krate)]
        #input
    };

    Ok(output)
}
