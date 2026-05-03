use darling::ast::NestedMeta;
use darling::util::{Flag, Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident, quote, quote_spanned as qs};
use std::collections::HashSet;
use std::default::Default;
use std::iter::zip;
use std::mem::take;
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Brace, Bracket, Comma, Paren, Pound};
use syn::{
    AttrStyle, Attribute, Data, DeriveInput, Expr, Field, Fields, FieldsNamed, FieldsUnnamed,
    Index, LitStr, Meta, Path, Type, WherePredicate, parse_quote, parse_quote_spanned as pqs,
    parse_str, parse2,
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

#[derive(Debug, Clone, FromMeta)]
#[darling(default)]
struct Crate(Path);

impl Crate {
    fn infer() -> Self {
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
struct Attributes {
    #[doc(hidden)]
    #[darling(rename = "attrs", multiple)]
    _attributes: Vec<MetaList>,
    #[darling(skip)]
    attributes: Option<Vec<Attribute>>,
}

impl Attributes {
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
struct OptionizedArgs {
    #[darling(rename = "crate")]
    krate: Option<Crate>,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct GeneralArgs {
    name: Option<LitStr>,
    #[darling(flatten)]
    attrs: Attributes,
}

impl GeneralArgs {
    fn is_some(&self) -> bool {
        self.name.is_some() || self.attrs.attributes.is_some()
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct MarkedArgs {
    name: Option<Ident>,
    #[darling(flatten)]
    attrs: Attributes,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct PartialArgs {
    upgradable: Flag,
    marked: Option<SpannedValue<Override<MarkedArgs>>>,
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(optionize))]
struct StructArgs {
    #[darling(flatten)]
    general: GeneralArgs,
    partial: Option<SpannedValue<Override<PartialArgs>>>,
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
    flatten: Flag,
    nest: Option<Type>,
    skip: Option<SpannedValue<Override<SkipArgs>>>,
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

// region codegen

// region ir

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
                let local = if let Some(ident) = ident.clone() {
                    format_ident!("v_{}", ident, span = Span::mixed_site())
                } else {
                    format_ident!("v_{}", i, span = Span::mixed_site())
                };

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

            let wrap = !args.flatten.is_present();
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

// endregion

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
                    <#nest as #krate::Optionized::<#ty>>::Errors: 'static
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

        let mut optionize = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized::<#ty>>::optionize(#subject.#original) }
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

        let patch = if *wrap {
            q! { v }
        } else {
            q! { self.#optionized }
        };
        let mut patch = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized::<#ty>>::patch(#patch, &mut #subject.#original); }
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

        let merge = match (wrap, nest) {
            (true, Some(nest)) => q! {
                match (&mut self.#optionized, #other.#optionized) {
                    (::core::option::Option::Some(this), ::core::option::Option::Some(other)) => <#nest as #krate::PartialOptionized::<#ty>>::merge(this, other),
                    (::core::option::Option::None, ::core::option::Option::Some(other)) => self.#optionized = ::core::option::Option::Some(other),
                    _ => {}
                }
            },
            (true, None) => q! {
                if ::core::option::Option::is_some(&#other.#optionized) {
                    self.#optionized = #other.#optionized;
                }
            },
            (false, Some(nest)) => q! {
                <#nest as #krate::PartialOptionized::<#ty>>::merge(&mut self.#optionized, #other.#optionized);
            },
            (false, None) => q! {
                self.#optionized = #other.#optionized;
            },
        };

        tokens.extend(merge);
    }
}

struct Validate<'l> {
    field: &'l FieldIr,
    original: &'l Ident,
    optionized: &'l Ident,
    failed: &'l Ident,
    errors: &'l Ident,
}

impl<'l> ToTokens for Validate<'l> {
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
                    #krate::Error::MissingField {
                        ty: #ty,
                        field: #field
                    }
                },
                q! {
                    |e| #krate::Error::NestedError {
                        ty: #ty,
                        field: #field,
                        source: #krate::__private::alloc::boxed::Box::new(e) as _
                    }
                },
            )
        };

        let failed = self.failed;
        let errors = self.errors;

        tokens.extend(q! { let #local = &self.#optionized; });

        let validate = nest.as_ref().map(|nest| {
            q! {
                if let ::core::result::Result::Err(e) = <#nest as #krate::Optionized::<#ty>>::validate(#local) {
                    #failed = true;
                    #errors.extend(::core::iter::IntoIterator::into_iter(e).map(#nest_map_err));
                }
            }
        });

        let validate = if *wrap {
            q! {
                if let ::core::option::Option::Some(#local) = #local {
                    #validate
                } else {
                    #failed = true;
                    #errors.push(#missing_err);
                }
            }
        } else {
            q! { #validate }
        };

        tokens.extend(validate);
    }
}

struct Upgrade<'l> {
    field: &'l FieldIr,
}

impl<'l> ToTokens for Upgrade<'l> {
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

        tokens.extend(q! { let #local = self.#optionized; });
        if *wrap {
            tokens.extend(q! { let #local = ::core::option::Option::unwrap(#local); });
        }
        if let Some(nest) = nest {
            tokens.extend(q! {
                let #local = <#nest as #krate::Optionized::<#ty>>::upgrade_unchecked(#local);
            })
        }
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

struct UpgradeField<'l> {
    field: &'l FieldIr,
    named: bool,
}

impl<'l> ToTokens for UpgradeField<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                original,
                local,
            }
        }

        let field = if self.named {
            q! { #original: #local, }
        } else {
            q! { #local, }
        };

        tokens.extend(field);
    }
}

// endregion

#[derive(Debug, Clone, Copy)]
enum StructStyle {
    Named,
    Unnamed,
    Unit,
}

fn parse(krate: Crate, input: TokenStream) -> Result<TokenStream> {
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

    let (partial, upgradable, marked) = args
        .partial
        .map(|partial| {
            let span = partial.span();
            let (upgradable, marked) = match partial.into_inner() {
                Override::Explicit(p) => (
                    p.upgradable.is_present().then(|| p.upgradable.span()),
                    p.marked,
                ),
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
                    pub #ident: ::core::marker::PhantomData<fn() -> *const #Subject>
                },
            )
        } else {
            (
                qs! { span => ::core::marker::PhantomData, },
                pqs! { span =>
                    #(#attrs)*
                    pub ::core::marker::PhantomData<fn() -> *const #Subject>
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

        let subject = &format_ident!("subject", span = Span::mixed_site());

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
        let other = &format_ident!("other", span = Span::mixed_site());
        let merges = optionized_fields.iter().map(|field| Merge { field, other });

        output.push(q! {
            #[automatically_derived]
            impl #impl_generics #krate::PartialOptionized<#Subject> for #optionized #type_generics #where_clause {
                #[inline]
                fn optionize(#subject: #Subject) -> Self { #optionize }
                #[inline]
                fn patch(self, #subject: &mut #Subject) { #(#patches)* }
                #[inline]
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

        let failed = &format_ident!("failed", span = Span::mixed_site());
        let errors = &format_ident!("errors", span = Span::mixed_site());

        let validates = optionized_fields.iter().map(|field| Validate {
            field,
            original,
            optionized,
            failed,
            errors,
        });

        let skips = original_fields.iter().map(|field| UpgradeSkip { field });
        let upgrades = optionized_fields.iter().map(|field| Upgrade { field });
        let subject = {
            let fields = original_fields
                .iter()
                .map(|field| UpgradeField { field, named });
            construct!(original_style, span => [#original] #(#fields)*)
        };

        output.push(qs! { span =>
            #[allow(non_snake_case)]
            #[automatically_derived]
            impl #impl_generics #krate::Optionized<#Subject> for #optionized #type_generics #where_clause {
                type Errors = #krate::ErrorCollection;
                #[inline]
                fn validate(&self) -> ::core::result::Result<(), Self::Errors> {
                    let mut #failed = false;
                    let mut #errors = #krate::ErrorCollection::default();
                    #(#validates)*
                    if !#failed {
                        ::core::result::Result::Ok(())
                    } else {
                        ::core::result::Result::Err(#errors)
                    }
                }
                #[inline]
                fn upgrade_unchecked(self) -> #Subject {
                    #(#skips)*
                    #(#upgrades)*
                    #subject
                }
            }
        });
    }

    Ok(q! { #(#output)* })
}

pub fn proc(args: TokenStream, input: &TokenStream) -> Result<TokenStream> {
    let args = OptionizedArgs::from_list(&NestedMeta::parse_meta_list(args)?)?;
    let krate = args.krate.unwrap_or_else(Crate::infer);
    let output = parse(krate.clone(), input.clone()).unwrap_or_else(|e| e.write_errors());
    let output = quote! {
        #[derive(#krate::__private::Optionize)]
        #input
        #output
    };
    Ok(output)
}
