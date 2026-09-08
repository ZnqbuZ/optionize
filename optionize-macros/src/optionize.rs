use darling::ast::NestedMeta;
use darling::util::{Flag, Override, SpannedValue};
use darling::{Error, FromAttributes, FromMeta, Result};
use derive_more::Deref;
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
    Index, Lit, LitStr, Member, Meta, Path, Type, TypePath, WherePredicate, parse_quote,
    parse_quote_spanned as pqs, parse2,
};

// region args

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
    attributes: Option<SpannedValue<Vec<Attribute>>>,
}

impl Attributes {
    fn finalize(mut self) -> Result<Self> {
        self.attributes = MetaList::merge(&mut self._attributes);
        Ok(self)
    }

    fn patch(self, attrs: &mut Vec<Attribute>) {
        if let Some(attributes) = self.attributes {
            *attrs = attributes.into_inner();
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

#[derive(Debug)]
enum TypeArg {
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
    fn parse(self) -> Result<TypePath> {
        match self {
            Self::Str(value) => value.parse().map_err(Error::from),
            Self::Path(path) => Ok(path),
        }
    }

    fn format(self, ident: &Ident) -> Result<TypePath> {
        match self {
            Self::Str(pattern) => format(&pattern, ident),
            Self::Path(path) => Ok(path),
        }
    }
}

#[derive(Debug, Default, Deref, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
struct StructArgs {
    #[deref]
    #[darling(flatten)]
    general: GeneralArgs,
    partial: Option<SpannedValue<Override<PartialArgs>>>,
    object: Option<TypeArg>,
    subject: Option<TypeArg>,
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
        }

        errors.finish_with(self)
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct SkipArgs {
    upgrade: Option<Expr>,
}

#[derive(Debug, Default, Deref, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
struct FieldArgs {
    #[deref]
    #[darling(flatten)]
    general: GeneralArgs,
    flatten: Flag,
    nest: Option<TypeArg>,
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

fn format<T: syn::parse::Parse>(pattern: &LitStr, ident: &Ident) -> Result<T> {
    let value = pattern.value().replace("{}", &ident.unraw().to_string());
    LitStr::new(&value, pattern.span())
        .parse()
        .map_err(Error::from)
}

fn is_optionize(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "optionize")
}

fn member_to_string(member: &Member) -> String {
    match member {
        Member::Named(ident) => ident.unraw().to_string(),
        Member::Unnamed(index) => index.index.to_string(),
    }
}

fn collect_idents(tokens: TokenStream, names: &mut HashSet<String>) {
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

fn fresh_ident(prefix: &str, names: &HashSet<String>) -> Ident {
    let mut name = prefix.to_owned();
    while names.contains(&name) {
        name.push('_');
    }
    format_ident!("{name}", span = Span::mixed_site())
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
    visibility: syn::Visibility,
    index: usize,
    span: Span,
    original: Member,
    optionized: Member,
    strategy: FieldStrategy,
    local: Ident,
}

impl Default for FieldIr {
    fn default() -> Self {
        Self {
            krate: Default::default(),
            ty: parse_quote!(()),
            visibility: syn::Visibility::Inherited,
            index: 0,
            span: Span::call_site(),
            original: format_ident!("_").into(),
            optionized: format_ident!("_").into(),
            strategy: Default::default(),
            local: format_ident!("_"),
        }
    }
}

macro_rules! expand {
    ($target:expr => { $($field:ident $(: $bind:pat)?),* $(,)? }) => {
        let FieldIr {
            #[allow(unused_variables)]
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
    fn nested_descriptor(&self) -> Option<Type> {
        expand! { self => { krate, ty, strategy } }
        if let FieldStrategy::Optionize {
            nest: Some(nest), ..
        } = strategy
        {
            Some(pq! { <#nest as #krate::__private::Mapping<#ty>>::Descriptor })
        } else {
            None
        }
    }

    fn view_type(&self, borrow: &syn::Lifetime) -> TokenStream {
        expand! { self => { krate, ty } }
        if let Some(descriptor) = self.nested_descriptor() {
            q! { ::core::option::Option<<#descriptor as #krate::Schema<#ty>>::View<#borrow>> }
        } else {
            q! { ::core::option::Option<&#borrow #ty> }
        }
    }

    fn view_member(&self) -> Ident {
        format_ident!(
            "v_{}",
            member_to_string(&self.original),
            span = Span::mixed_site()
        )
    }

    fn view(&self, full: bool, root: &TokenStream) -> TokenStream {
        expand! { self => { krate, ty, original, optionized, strategy } }
        if full {
            if let Some(descriptor) = self.nested_descriptor() {
                return q! { ::core::option::Option::Some(<#descriptor as #krate::Schema<#ty>>::full_view(&#root.#original)) };
            }
            return q! { ::core::option::Option::Some(&#root.#original) };
        }
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return q! { ::core::option::Option::None };
        };
        if let Some(nest) = nest {
            let descriptor = self.nested_descriptor().unwrap();
            let view = q! { <#nest as #krate::PartialOptionized<#ty, #descriptor>>::view };
            if *wrap {
                q! { #root.#optionized.as_ref().map(#view) }
            } else {
                q! { ::core::option::Option::Some(#view(&#root.#optionized)) }
            }
        } else if *wrap {
            q! { #root.#optionized.as_ref() }
        } else {
            q! { ::core::option::Option::Some(&#root.#optionized) }
        }
    }

    fn retain_where(&self, borrow: &syn::Lifetime) -> Option<WherePredicate> {
        expand! { self => { krate, ty, strategy, index } }
        match strategy {
            FieldStrategy::Skip { .. } => None,
            FieldStrategy::Optionize { nest: None, .. } => {
                Some(pq! { for<#borrow> &#borrow #ty: #krate::__private::Equal<#index> })
            }
            FieldStrategy::Optionize {
                nest: Some(nest), ..
            } => {
                let descriptor = self.nested_descriptor().unwrap();
                // The unused binder defers concrete comparison bounds until
                // Retain is used, keeping other operations available without it.
                Some(pq! { for<#borrow> #nest: #krate::Retain<#ty, #descriptor> })
            }
        }
    }

    fn retain(&self, baseline: &Ident, remains: &Ident) -> TokenStream {
        expand! { self => { krate, ty, optionized, strategy, index } }
        let this = format_ident!("self", span = Span::mixed_site());
        let member = self.view_member();
        let descriptor = self.nested_descriptor();
        match strategy {
            FieldStrategy::Skip { .. } => TokenStream::new(),
            FieldStrategy::Optionize {
                wrap: true,
                nest: None,
            } => q! {
                if let (::core::option::Option::Some(value), ::core::option::Option::Some(baseline)) =
                    (#this.#optionized.as_ref(), #baseline.#member)
                    && #krate::__private::Equal::<#index>::equal(value, baseline)
                {
                    #this.#optionized = ::core::option::Option::None;
                }
                #remains |= #this.#optionized.is_some();
            },
            FieldStrategy::Optionize {
                wrap: false,
                nest: None,
            } => q! {
                #remains |= #baseline.#member.is_none_or(|baseline| {
                    !#krate::__private::Equal::<#index>::equal(&#this.#optionized, baseline)
                });
            },
            FieldStrategy::Optionize {
                wrap: true,
                nest: Some(_),
            } => q! {
                let changed = match (#this.#optionized.as_mut(), #baseline.#member) {
                    (::core::option::Option::None, _) => false,
                    (::core::option::Option::Some(_), ::core::option::Option::None) => true,
                    (::core::option::Option::Some(value), ::core::option::Option::Some(baseline)) => {
                        #krate::Retain::<#ty, #descriptor>::retain_view(value, baseline)
                    }
                };
                if !changed {
                    #this.#optionized = ::core::option::Option::None;
                }
                #remains |= changed;
            },
            FieldStrategy::Optionize {
                wrap: false,
                nest: Some(_),
            } => q! {
                #remains |= match #baseline.#member {
                    ::core::option::Option::None => true,
                    ::core::option::Option::Some(baseline) => {
                        #krate::Retain::<#ty, #descriptor>::retain_view(&mut #this.#optionized, baseline)
                    }
                };
            },
        }
    }

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
                    Some(ident) => ident.clone().into(),
                    None => Index {
                        index: i as u32,
                        span,
                    }
                    .into(),
                };

                FieldIr {
                    krate: krate.clone(),
                    ty: ty.clone(),
                    visibility: field.vis.clone(),
                    index: i,
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

            if let Some(name) = &args.general.name {
                let Some(ident) = ident.as_ref() else {
                    errors.push(
                        Error::custom("`name` attribute cannot be used on unnamed fields")
                            .with_span(name),
                    );
                    continue;
                };
                let ident = match format(name, ident) {
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
                Some(ident) => ident.clone().into(),
                None => Index {
                    index: (i - skipped) as u32,
                    span,
                }
                .into(),
            };

            let wrap = !args.flatten.is_present();
            let Some(nest) = errors.handle(args.nest.map(TypeArg::parse).transpose()) else {
                continue;
            };
            let nest = nest.map(Type::Path);

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

    fn extract_object(
        fields: &mut Punctuated<Field, Comma>,
        krate: Crate,
        partial: bool,
    ) -> Result<Vec<Self>> {
        let mut result = Vec::new();
        let mut errors = Error::accumulator();
        let mut skipped = 0;
        for (i, mut field) in take(fields).into_iter().enumerate() {
            let Some(args) = errors.handle(FieldArgs::from_attributes(&field.attrs)) else {
                continue;
            };
            let span = field.span();
            span!(span);
            let object_member: Member = field.ident.clone().map(Into::into).unwrap_or_else(|| {
                Index {
                    index: (i - skipped) as u32,
                    span,
                }
                .into()
            });
            let original = if let Some(name) = args.name.as_ref() {
                let Some(ident) = field.ident.as_ref() else {
                    errors.push(
                        Error::custom("`name` cannot be used on unnamed fields").with_span(name),
                    );
                    continue;
                };
                let Some(ident) = errors.handle(format::<Ident>(name, ident)) else {
                    continue;
                };
                ident.into()
            } else {
                field.ident.clone().map(Into::into).unwrap_or_else(|| {
                    Index {
                        index: i as u32,
                        span,
                    }
                    .into()
                })
            };
            let mut ir = Self {
                krate: krate.clone(),
                ty: field.ty.clone(),
                visibility: field.vis.clone(),
                index: i,
                span,
                original,
                optionized: object_member,
                local: format_ident!("v_{}", i, span = Span::mixed_site()),
                ..Default::default()
            };
            if let Some(skip) = args.skip {
                if !partial {
                    errors.push(
                        Error::custom(
                            "`skip` attribute is only allowed when `partial` is specified",
                        )
                        .with_span(&skip.span()),
                    );
                    continue;
                }
                let ty = &ir.ty;
                let upgrade = skip
                    .into_inner()
                    .explicit()
                    .and_then(|s| s.upgrade)
                    .unwrap_or_else(|| pq! { <#ty as ::core::default::Default>::default() });
                ir.strategy = FieldStrategy::Skip { upgrade };
                skipped += 1;
                result.push(ir);
                continue;
            }
            let wrap = !args.flatten.is_present();
            let object_ty = &field.ty;
            let payload: Type = if wrap {
                pq! { <#object_ty as #krate::__private::OptionField>::Value }
            } else {
                object_ty.clone()
            };
            let Some(nest) = errors.handle(args.nest.map(TypeArg::parse).transpose()) else {
                continue;
            };
            let nest = nest.map(|subject_ty| {
                ir.ty = Type::Path(subject_ty);
                payload.clone()
            });
            if nest.is_none() {
                ir.ty = payload;
            }
            ir.strategy = FieldStrategy::Optionize { wrap, nest };
            args.general.attrs.patch(&mut field.attrs);
            result.push(ir);
            fields.push(field);
        }
        errors.finish_with(result)
    }
}

// endregion

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
            let descriptor = self.nested_descriptor().unwrap();
            vec![
                pq! { #nest: #krate::__private::Mapping<#ty> },
                pq! { #nest: #krate::PartialOptionized<#ty, #descriptor> },
            ]
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
            let descriptor = self.nested_descriptor().unwrap();
            vec![
                pq! {
                    #nest: #krate::Optionized<#ty, #descriptor>
                },
                pq! {
                    <#nest as #krate::Optionized<#ty, #descriptor>>::Errors: 'static
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
        let descriptor = self.field.nested_descriptor();

        let subject = self.subject;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let mut optionize = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty, #descriptor>>::optionize(#subject.#original) }
        } else {
            q! { #subject.#original }
        };

        if *wrap {
            optionize = q! { ::core::option::Option::Some(#optionize) }
        };

        tokens.extend(q! { #optionized: #optionize, });
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
        let this = format_ident!("self", span = Span::mixed_site());
        let descriptor = self.field.nested_descriptor();

        let subject = self.subject;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let patch = if *wrap {
            q! { v }
        } else {
            q! { #this.#optionized }
        };
        let mut patch = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty, #descriptor>>::patch(#patch, &mut #subject.#original); }
        } else {
            q! { #subject.#original = #patch; }
        };
        if *wrap {
            patch = q! {
                if let ::core::option::Option::Some(v) = #this.#optionized {
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
        let this = format_ident!("self", span = Span::mixed_site());
        let descriptor = self.field.nested_descriptor();

        let other = self.other;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let merge = match (wrap, nest) {
            (true, Some(nest)) => q! {
                match (&mut #this.#optionized, #other.#optionized) {
                    (::core::option::Option::Some(this), ::core::option::Option::Some(other)) => <#nest as #krate::PartialOptionized<#ty, #descriptor>>::merge(this, other),
                    (::core::option::Option::None, ::core::option::Option::Some(other)) => #this.#optionized = ::core::option::Option::Some(other),
                    _ => {}
                }
            },
            (true, None) => q! {
                if ::core::option::Option::is_some(&#other.#optionized) {
                    #this.#optionized = #other.#optionized;
                }
            },
            (false, Some(nest)) => q! {
                <#nest as #krate::PartialOptionized<#ty, #descriptor>>::merge(&mut #this.#optionized, #other.#optionized);
            },
            (false, None) => q! {
                #this.#optionized = #other.#optionized;
            },
        };

        tokens.extend(merge);
    }
}

struct Validate<'l> {
    field: &'l FieldIr,
    subject: &'l TokenStream,
    object: &'l TokenStream,
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
        let this = format_ident!("self", span = Span::mixed_site());
        let descriptor = self.field.nested_descriptor();

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let original = member_to_string(original);
        let optionize = member_to_string(optionized);

        let renamed = original == optionize;

        let (missing_err, nest_map_err) = {
            let ty = {
                let subject = self.subject.to_string();
                let object = self.object.to_string();

                q! {
                    #krate::TypeInfo {
                        subject: #subject,
                        object: #object,
                    }
                }
            };

            let field = if renamed {
                q! { #krate::FieldInfo::Identical ( #original ) }
            } else {
                q! { #krate::FieldInfo::Renamed { original: #original, optionized: #optionize } }
            };

            (
                q! {
                    #krate::Error::Missing {
                        ty: #ty,
                        field: #field
                    }
                },
                q! {
                    |e| #krate::Error::Nested {
                        ty: #ty,
                        field: #field,
                        source: #krate::__private::alloc::boxed::Box::new(e) as _
                    }
                },
            )
        };

        let failed = self.failed;
        let errors = self.errors;

        tokens.extend(q! { let #local = &#this.#optionized; });

        let validate = nest.as_ref().map(|nest| {
            q! {
                if let ::core::result::Result::Err(e) = <#nest as #krate::Optionized<#ty, #descriptor>>::validate(#local) {
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

struct Upgrade<'l>(&'l FieldIr);

impl<'l> ToTokens for Upgrade<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.0 => {
                krate,
                ty,
                optionized,
                strategy,
                local,
            }
        }
        let this = format_ident!("self", span = Span::mixed_site());
        let descriptor = self.0.nested_descriptor();

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        tokens.extend(q! { let #local = #this.#optionized; });
        if *wrap {
            tokens.extend(
                q! { let #local = unsafe { ::core::option::Option::unwrap_unchecked(#local) }; },
            );
        }
        if let Some(nest) = nest {
            tokens.extend(q! {
                let #local = unsafe { <#nest as #krate::Optionized<#ty, #descriptor>>::upgrade_unchecked(#local) };
            })
        }
    }
}

struct UpgradeSkip<'l>(&'l FieldIr);

impl<'l> ToTokens for UpgradeSkip<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.0 => {
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

struct UpgradeFieldValue<'l>(&'l FieldIr);

impl<'l> ToTokens for UpgradeFieldValue<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.0 => { original, local } }
        tokens.extend(q! { #original: #local, });
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
    let subject = parse2::<DeriveInput>(input)?;
    let _span = subject.span();
    span!(_span);

    macro_rules! construct {
        ($style:expr, $span:expr => [$($ty:tt)+] $($fields:tt)*) => {
            match $style {
                StructStyle::Unit => qs! { $span => $($ty)* },
                _ => qs! { $span => #[allow(clippy::init_numbered_fields)] $($ty)* { $($fields)* } },
            }
        };
    }

    let this = format_ident!("self", span = Span::mixed_site());
    let mut borrow_name = "__optionize".to_owned();
    while subject
        .generics
        .lifetimes()
        .any(|parameter| parameter.lifetime.ident == borrow_name)
    {
        borrow_name.push('_');
    }
    let borrow = syn::Lifetime::new(&format!("'{borrow_name}"), Span::mixed_site());
    let args = StructArgs::from_attributes(&subject.attrs)?;
    let source = q! { #[derive(#krate::__private::Optionize)] #subject };
    let reverse = args.subject.is_some();

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

    let mut object = subject;
    let (impl_generics, type_generics, where_clause) = object.generics.split_for_impl();

    let subject = &object.ident.clone();
    #[allow(non_snake_case)]
    let (Subject, subject_constructor) = if let Some(target) = args.subject {
        let path = target.format(subject)?;
        let ty = q! { #path };
        let mut constructor = path;
        for segment in &mut constructor.path.segments {
            if let syn::PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                arguments.colon2_token.get_or_insert_with(Default::default);
            }
        }
        (ty, q! { #constructor })
    } else {
        (q! { #subject #type_generics }, q! { #subject })
    };

    let has_object = args.object.is_some();
    #[allow(non_snake_case)]
    let Object = if reverse {
        q! { #subject #type_generics }
    } else if let Some(object) = args.object {
        let path = object.format(subject)?;
        q! { #path }
    } else {
        object.ident = match &args.general.name {
            Some(name) => format(name, subject)?,
            None => format(&pqs! { subject.span() => "{}Optional"}, subject)?,
        };
        let name = &object.ident;
        q! { #name #type_generics }
    };

    if !has_object && !reverse {
        args.general.attrs.patch(&mut object.attrs);
    }

    let data = match &mut object.data {
        Data::Struct(data) => data,
        _ => {
            return Err(
                Error::custom("Optionize can only be derived for structs").with_span(&_span)
            );
        }
    };

    let subject_style = match &data.fields {
        Fields::Named(_) => StructStyle::Named,
        Fields::Unnamed(_) => StructStyle::Unnamed,
        Fields::Unit => StructStyle::Unit,
    };
    let object_style = if matches!(subject_style, StructStyle::Unit)
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
        subject_style
    };

    let fields = match &mut data.fields {
        Fields::Named(fields) => &mut fields.named,
        Fields::Unnamed(fields) => &mut fields.unnamed,
        Fields::Unit => &mut Default::default(),
    };

    let originals = if reverse {
        FieldIr::extract_object(fields, krate.clone(), partial.is_some())?
    } else {
        FieldIr::extract(fields, krate.clone(), partial.is_some())?
    };
    let optionizeds = originals
        .iter()
        .filter(|f| matches!(f.strategy, FieldStrategy::Optionize { .. }))
        .collect::<Vec<_>>();

    let marker = if let Some(marked) = marked {
        let span = marked.span();
        let marked = marked.into_inner().unwrap_or_default();

        let mut attrs = vec![pqs! { span => #[doc(hidden)] }];
        marked.attrs.patch(&mut attrs);

        let ident = match (subject_style, marked.name) {
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
            let index = Index {
                index: fields.len() as u32,
                span,
            };
            (
                qs! { span => #index: ::core::marker::PhantomData, },
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

    let mut output = Vec::new();
    if reverse {
        output.push(q! { #[derive(#krate::__private::Optionize)] #object });
    } else {
        output.push(source);
    }
    if !has_object && !reverse {
        output.push(q! { #object });
    }
    let declarations = take(&mut output);

    let mut where_clause = where_clause.cloned().unwrap_or_else(|| pq! { where });
    let mut where_predicates = HashSet::new();
    macro_rules! where_clause_extend {
        ($map:expr) => {
            where_clause.predicates.extend(
                optionizeds
                    .iter()
                    .copied()
                    .flat_map($map)
                    .filter(|p| where_predicates.insert(p.clone())),
            )
        };
    }

    where_clause_extend!(FieldIr::partial_optionized_where);

    #[allow(non_snake_case)]
    let Descriptor = if reverse { &Object } else { &Subject };
    let mut type_names = HashSet::new();
    let field_types = originals.iter().map(|field| {
        let ty = &field.ty;
        let nest = match &field.strategy {
            FieldStrategy::Optionize { nest, .. } => nest.as_ref(),
            FieldStrategy::Skip { .. } => None,
        };
        q! { #ty #nest }
    });
    collect_idents(
        q! { #Subject #Object #impl_generics #where_clause #(#field_types)* },
        &mut type_names,
    );
    let view_ident = fresh_ident("__OptionizeView", &type_names);
    let mut view_generics = object.generics.clone();
    view_generics.params.insert(0, parse_quote! { #borrow });
    view_generics.where_clause = Some(where_clause.clone());
    view_generics
        .make_where_clause()
        .predicates
        .push(pq! { #Subject: #borrow });
    view_generics
        .make_where_clause()
        .predicates
        .push(pq! { #Descriptor: #borrow });
    let (view_impl_generics, view_type_generics, view_where_clause) =
        view_generics.split_for_impl();
    let view_fields = originals.iter().map(|field| {
        let visibility = &field.visibility;
        let member = field.view_member();
        let ty = field.view_type(&borrow);
        q! { #visibility #member: #ty, }
    });
    let empty_fields = originals.iter().map(|field| {
        let member = field.view_member();
        q! { #member: ::core::option::Option::None, }
    });
    let full_fields = originals.iter().map(|field| {
        let member = field.view_member();
        let value = field.view(true, &q! { subject });
        q! { #member: #value, }
    });
    let partial_fields = originals.iter().map(|field| {
        let member = field.view_member();
        let value = field.view(false, &q! { #this });
        q! { #member: #value, }
    });
    let view = q! { #view_ident { #(#partial_fields)* __marker: ::core::marker::PhantomData } };
    output.push(q! {
        // A nominal view keeps private field types out of public associated types.
        // The anonymous const gives each mapping its own scope without user names.
        #[doc(hidden)]
        #[allow(private_bounds)]
        pub struct #view_ident #view_impl_generics #view_where_clause {
            #(#view_fields)*
            __marker: ::core::marker::PhantomData<fn() -> &#borrow #Descriptor>,
        }
        impl #view_impl_generics ::core::default::Default for #view_ident #view_type_generics #view_where_clause {
            fn default() -> Self {
                Self { #(#empty_fields)* __marker: ::core::marker::PhantomData }
            }
        }
        #[automatically_derived]
        impl #impl_generics #krate::Schema<#Subject> for #Descriptor #where_clause {
            type View<#borrow> = #view_ident #view_type_generics
            where #Subject: #borrow, Self: #borrow;
            #[inline]
            fn full_view<#borrow>(subject: &#borrow #Subject) -> Self::View<#borrow>
            where Self: #borrow {
                #view_ident { #(#full_fields)* __marker: ::core::marker::PhantomData }
            }
        }
        #[automatically_derived]
        impl #impl_generics #krate::__private::Mapping<#Subject> for #Object #where_clause {
            type Descriptor = #Descriptor;
        }
    });
    let mut retain_where = where_clause.clone();
    let mut retain_predicates = HashSet::new();
    retain_where.predicates.extend(
        originals
            .iter()
            .filter_map(|field| field.retain_where(&borrow))
            .filter(|predicate| retain_predicates.insert(predicate.clone())),
    );
    let baseline = format_ident!("baseline", span = Span::mixed_site());
    let remains = format_ident!("remains", span = Span::mixed_site());
    let retain = originals
        .iter()
        .map(|field| field.retain(&baseline, &remains));
    output.push(q! {
        #[automatically_derived]
        impl #impl_generics #krate::Retain<#Subject, #Descriptor> for #Object #retain_where {
            #[inline]
            fn retain_view<#borrow>(&mut #this, #baseline: #view_ident #view_type_generics) -> bool
            where #Subject: #borrow, #Descriptor: #borrow {
                let mut #remains = false;
                #(#retain)*
                #remains
            }
        }
    });
    if reverse {
        output.push(q! {
            #[automatically_derived]
            impl #impl_generics #krate::PartialOptionized<#Subject, #Descriptor> for #Subject #where_clause {
                #[inline]
                fn optionize(subject: #Subject) -> Self { subject }
                #[inline]
                fn patch(#this, subject: &mut #Subject) { *subject = #this; }
                #[inline]
                fn merge(&mut #this, other: Self) { *#this = other; }
                #[inline]
                fn view<#borrow>(&#borrow #this) -> <#Descriptor as #krate::Schema<#Subject>>::View<#borrow>
                where #Subject: #borrow, #Descriptor: #borrow {
                    <#Descriptor as #krate::Schema<#Subject>>::full_view(#this)
                }
            }
        });
    }

    output.push(q! {
        #[automatically_derived]
        impl #impl_generics #krate::Optionizable<#Object, #Descriptor> for #Subject #where_clause {}
    });

    {
        let subject = &format_ident!("subject", span = Span::mixed_site());

        let optionize = {
            let optionizes = optionizeds.iter().map(|field| Optionize { field, subject });

            construct!(object_style, _span => [Self] #(#optionizes)* #marker )
        };
        let patches = optionizeds.iter().map(|field| Patch { field, subject });
        let other = &format_ident!("other", span = Span::mixed_site());
        let merges = optionizeds.iter().map(|field| Merge { field, other });

        output.push(q! {
            #[automatically_derived]
            impl #impl_generics #krate::PartialOptionized<#Subject, #Descriptor> for #Object #where_clause {
                #[inline]
                fn optionize(#subject: #Subject) -> Self { #optionize }
                #[inline]
                fn patch(#this, #subject: &mut #Subject) { #(#patches)* }
                #[inline]
                fn merge(&mut #this, #other: Self) { #(#merges)* }
                #[inline]
                fn view<#borrow>(&#borrow #this) -> <#Descriptor as #krate::Schema<#Subject>>::View<#borrow>
                where #Subject: #borrow, #Descriptor: #borrow { #view }
            }
        });
    }

    let span = if partial.is_none() {
        Some(_span)
    } else {
        upgradable
    };

    if let Some(span) = span {
        where_clause_extend!(FieldIr::optionized_where);

        let failed = &format_ident!("failed", span = Span::mixed_site());
        let errors = &format_ident!("errors", span = Span::mixed_site());

        let validates = optionizeds.iter().map(|field| Validate {
            field,
            subject: &Subject,
            object: &Object,
            failed,
            errors,
        });

        let skips = originals.iter().map(UpgradeSkip);
        let upgrades = optionizeds.iter().copied().map(Upgrade);
        let subject = {
            let fields = originals.iter().map(UpgradeFieldValue);
            construct!(subject_style, span => [#subject_constructor] #(#fields)*)
        };

        output.push(qs! { span =>
            #[automatically_derived]
            impl #impl_generics #krate::Optionized<#Subject, #Descriptor> for #Object #where_clause {
                type Errors = #krate::ErrorCollection;
                #[inline]
                fn validate(&#this) -> ::core::result::Result<(), Self::Errors> {
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
                unsafe fn upgrade_unchecked(#this) -> #Subject {
                    #(#skips)*
                    #(#upgrades)*
                    #subject
                }
            }
        });
    }

    Ok(q! { #(#declarations)* const _: () = { #(#output)* }; })
}

pub fn proc(args: TokenStream, input: &TokenStream) -> Result<TokenStream> {
    let args = OptionizedArgs::from_list(&NestedMeta::parse_meta_list(args)?)?;
    let krate = args.krate.unwrap_or_else(Crate::infer);
    Ok(parse(krate.clone(), input.clone()).unwrap_or_else(|error| {
        let error = error.write_errors();
        quote! {
            #[derive(#krate::__private::Optionize)]
            #input
            #error
        }
    }))
}
