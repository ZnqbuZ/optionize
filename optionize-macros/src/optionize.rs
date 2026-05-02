use darling::ast::NestedMeta;
use darling::util::{Override, SpannedValue};
use darling::{Error, Result};
use darling::{FromAttributes, FromMeta};
use proc_macro2::Span;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote_spanned as qs, ToTokens};
use std::collections::HashSet;
use std::default::Default;
use std::iter::zip;
use std::mem::take;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Bracket, Comma, Pound};
use syn::{
    parse2, parse_quote_spanned as pqs, parse_str, AttrStyle, Attribute, Data, DeriveInput, Expr, Field, Fields,
    GenericArgument, Index, LitBool, LitStr, Meta, PathArguments, Type,
};

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
#[darling(default)]
struct PartialArgs {
    upgradable: bool,
    marked: bool,
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "Self::finalize")]
struct StructArgs {
    name: Option<LitStr>,
    #[darling(rename = "attrs", multiple)]
    _attributes: Vec<MetaList>,
    #[darling(skip)]
    attributes: Option<Vec<Attribute>>,
    partial: Option<Override<PartialArgs>>,
}

impl StructArgs {
    fn finalize(mut self) -> Result<Self> {
        self.attributes = MetaList::merge(&mut self._attributes);
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
    name: Option<LitStr>,
    #[darling(rename = "attrs", multiple)]
    _attributes: Vec<MetaList>,
    #[darling(skip)]
    attributes: Option<Vec<Attribute>>,
    wrap: Option<LitBool>,
    nest: Option<Type>,
    skip: Option<SpannedValue<Override<SkipArgs>>>,
}

impl FieldArgs {
    fn finalize(mut self) -> Result<Self> {
        let mut errors = Error::accumulator();

        if let Some(skip) = &self.skip {
            let skip = skip.span();

            if let Some(wrap) = &self.wrap {
                let wrap = wrap.span;
                errors.push(
                    Error::custom("`wrap` cannot be used with `skip`")
                        .with_span(&skip.join(wrap).unwrap_or(wrap)),
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

        self.attributes = MetaList::merge(&mut self._attributes);

        errors.finish_with(self)
    }
}

fn is_option(ty: &Type) -> bool {
    let path = match ty {
        Type::Path(path) => path,
        Type::Paren(ty) => return is_option(&ty.elem),
        _ => return false,
    };

    if path.qself.is_some() {
        return false;
    }

    let Some(segment) = path.path.segments.last() else {
        return false;
    };

    segment.ident == "Option"
        && matches!(
            &segment.arguments,
            PathArguments::AngleBracketed(args)
                if args.args.len() == 1
                    && matches!(args.args.first(), Some(GenericArgument::Type(_)))
        )
}

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
    span: Span,
    original: TokenStream,
    optionized: TokenStream,
    strategy: FieldStrategy,
    local: Ident,
}

impl Default for FieldIr {
    fn default() -> Self {
        Self {
            span: Span::call_site(),
            original: Default::default(),
            optionized: Default::default(),
            strategy: Default::default(),
            local: format_ident!("_"),
        }
    }
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

impl FieldIr {
    fn extract(
        fields: &mut Punctuated<Field, Comma>,
        args: Vec<FieldArgs>,
        partial: bool,
    ) -> Result<Vec<Self>> {
        let mut errors = Error::accumulator();

        let mut this = Vec::new();
        let mut skipped = 0;

        for (i, (mut field, args)) in zip(take(fields), args).enumerate() {
            let _span = field.span();
            span!(_span);

            let ident = &field.ident;
            let ty = &field.ty;

            let original_field = match ident {
                Some(ident) => q! { #ident },
                None => {
                    let index = Index {
                        index: i as u32,
                        span: _span,
                    };
                    q! { #index }
                }
            };

            let (skip, upgrade) = match args.skip {
                Some(skip) => match skip.into_inner() {
                    Override::Inherit => (true, None),
                    Override::Explicit(s) => (true, s.upgrade),
                },
                None => (false, None),
            };

            if skip {
                if !partial {
                    errors.push(
                        Error::custom(
                            "`skip` attribute is only allowed when `partial` is specified",
                        )
                        .with_span(&field),
                    );
                    continue;
                }

                let field = FieldIr {
                    span: _span,
                    original: original_field,
                    strategy: FieldStrategy::Skip {
                        upgrade: upgrade.unwrap_or_else(|| {
                            pq! { ::core::default::Default::default() }
                        }),
                    },
                    ..Default::default()
                };

                skipped += 1;
                this.push(field);
                continue;
            }

            if let Some(name) = args.name {
                let Some(ident) = ident.as_ref() else {
                    errors.push(
                        Error::custom("`name` attribute cannot be used on unnamed fields")
                            .with_span(ty),
                    );
                    continue;
                };
                let span = name.span();
                let mut ident = match format(&name.value(), ident) {
                    Ok(ident) => ident,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                ident.set_span(span);
                field.ident = Some(ident.clone());
            }

            if let Some(attrs) = args.attributes {
                field.attrs = attrs;
            }

            let optionized_field = match &field.ident {
                Some(ident) => q! { #ident },
                None => {
                    let index = Index {
                        index: (i - skipped) as u32,
                        span: _span,
                    };
                    q! { #index }
                }
            };

            let (nest, ty) = if let Some(optionized) = &args.nest {
                (Some(ty.clone()), optionized)
            } else {
                (None, ty)
            };

            let wrap = args
                .wrap
                .as_ref()
                .map(LitBool::value)
                .unwrap_or_else(|| !is_option(ty));
            field.ty = if wrap {
                pqs! { ty.span() => Option<#ty> }
            } else {
                ty.clone()
            };

            let mut local = if let Some(ident) = field.ident.clone() {
                format_ident!("v_{}", ident)
            } else {
                format_ident!("v_{}", i)
            };
            local.set_span(_span);

            this.push(FieldIr {
                span: _span,
                original: original_field.clone(),
                optionized: optionized_field.clone(),
                strategy: FieldStrategy::Optionize { wrap, nest },
                local,
            });

            fields.push(field);
        }

        errors.finish_with(this)
    }
}

// region Generators

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

struct Optionize<'l> {
    field: &'l FieldIr,
    subject: &'l Ident,
    named: bool,
}

impl<'l> ToTokens for Optionize<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
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
            q! { ::optionize::PartialOptionized::<#nest>::optionize(#subject.#original) }
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
            q! { ::optionize::PartialOptionized::<#nest>::patch(#patch, &mut #subject.#original); }
        } else {
            q! { #subject.#original = #patch; }
        };
        if *wrap {
            patch = q! {
                if let Some(v) = self.#optionized {
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
                    (Some(this), Some(other)) => ::optionize::PartialOptionized::<#nest>::merge(this, other),
                    (None, Some(other)) => self.#optionized = Some(other),
                    _ => {}
                }
            },
            (true, None) => q! {
                if #other.#optionized.is_some() {
                    self.#optionized = #other.#optionized;
                }
            },
            (false, Some(nest)) => q! {
                ::optionize::PartialOptionized::<#nest>::merge(&mut self.#optionized, #other.#optionized);
            },
            (false, None) => q! {
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
                    ::optionize::TypeInfo {
                        original: #original_str,
                        optionized: #optionized_str,
                    }
                }
            };

            let field = if renamed {
                q! { ::optionize::FieldInfo::Identical ( #original_str ) }
            } else {
                q! { ::optionize::FieldInfo::Renamed { original: #original_str, optionized: #optionized_str } }
            };

            (
                q! {
                    ::optionize::UpgradeError::MissingField {
                        ty: #ty,
                        field: #field
                    }
                },
                q! {
                    |e| ::optionize::UpgradeError::NestedError {
                        ty: #ty,
                        field: #field,
                        source: ::optionize::__private::alloc::boxed::Box::new(e) as _
                    }
                },
            )
        };

        let failed = self.failed;
        let errors = self.errors;

        tokens.extend(q! { let #local = self.#optionized; });

        let mut expr = if let Some(nest) = nest {
            let err = if *wrap {
                q!(::core::option::Option::Some(v))
            } else {
                q!(v)
            };
            q! {
                ::optionize::Optionized::<#nest>::upgrade(#local).map_err(|(e, v)| {
                    #failed = true;
                    #errors.extend(e.into_iter().map(#nest_map_err));
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

        let ok = match strategy {
            FieldStrategy::Skip { upgrade } => {
                if self.named {
                    q! { #original: #upgrade, }
                } else {
                    q! { #upgrade, }
                }
            }
            FieldStrategy::Optionize { .. } => {
                let local = q! { ::core::result::Result::unwrap(#local) };

                if self.named {
                    q! { #original: #local, }
                } else {
                    q! { #local, }
                }
            }
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
                optionized,
                strategy,
                local,
            }
        }

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let mut ok = q! { v };
        if let Some(nest) = nest {
            ok = q! { ::optionize::PartialOptionized::<#nest>::optionize(#ok) };
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

enum StructStyle {
    Named,
    Unnamed,
    Unit,
}

fn format(pattern: &str, ident: &Ident) -> Result<Ident> {
    let old = ident.to_string();
    let old = old.strip_prefix("r#").unwrap_or(&old);
    let new = format!("r#{}", pattern.replace("{}", old));
    parse_str::<Ident>(&new)
        .map_err(|_| Error::custom(format!("`{}` is not a valid identifier", new)).with_span(ident))
}

pub fn derive(input: TokenStream) -> Result<TokenStream> {
    let original = parse2::<DeriveInput>(input)?;
    let _span = original.span();
    span!(_span);

    macro_rules! construct {
        ($style:expr, [$($ty:tt)+] $($fields:tt)*) => {
            match $style {
                StructStyle::Named => q! { $($ty)* { $($fields)* } },
                StructStyle::Unnamed => q! { $($ty)* ( $($fields)* ) },
                StructStyle::Unit => q! { $($ty)* },
            }
        };
    }

    let struct_args = StructArgs::from_attributes(&original.attrs)?;

    let (partial, upgradable, marked) = match &struct_args.partial {
        Some(Override::Inherit) => (true, false, false),
        Some(Override::Explicit(p)) => (true, p.upgradable, p.marked),
        None => (false, false, false),
    };

    let mut optionized = original.clone();
    let original = &original.ident;

    let (impl_generics, type_generics, where_clause) = optionized.generics.split_for_impl();
    #[allow(non_snake_case)]
    let Subject = q! { #original #type_generics };

    let (style, fields) = match &mut optionized.data {
        Data::Struct(data) => match &mut data.fields {
            Fields::Named(fields) => (StructStyle::Named, &mut fields.named),
            Fields::Unnamed(fields) => (StructStyle::Unnamed, &mut fields.unnamed),
            Fields::Unit => (StructStyle::Unit, &mut Default::default()),
        },
        _ => {
            return Err(
                Error::custom("Optionize can only be derived for structs").with_span(&_span)
            );
        }
    };
    let named = matches!(style, StructStyle::Named);

    let field_args = {
        let mut field_args = Vec::with_capacity(fields.len());
        let mut errors = Error::accumulator();

        for field in fields.iter_mut() {
            if let Some(parsed) = errors
                .handle(FieldArgs::from_attributes(&field.attrs).map_err(|e| e.with_span(&field)))
            {
                field_args.push(parsed);
            }
            field
                .attrs
                .retain(|attr| !attr.path().is_ident("optionize"));
        }

        errors.finish_with(field_args)?
    };

    optionized.ident = {
        let ident = optionized.ident;
        let (span, mut ident) = match struct_args.name {
            Some(name) => (name.span(), format(&name.value(), &ident)?),
            None => (ident.span(), format("{}Optional", &ident)?),
        };
        ident.set_span(span);
        ident
    };

    if let Some(attrs) = struct_args.attributes {
        optionized.attrs = attrs;
    } else {
        optionized
            .attrs
            .retain(|attr| !attr.path().is_ident("optionize"));
    }

    let original_fields = FieldIr::extract(fields, field_args, partial)?;
    let optionized_fields = original_fields
        .iter()
        .filter(|f| matches!(f.strategy, FieldStrategy::Optionize { .. }))
        .collect::<Vec<_>>();

    let mut marker = None;
    if marked {
        match style {
            StructStyle::Named => {
                let names = fields
                    .iter()
                    .filter_map(|f| f.ident.as_ref())
                    .map(|i| i.to_string())
                    .collect::<HashSet<_>>();
                let mut ident = "_marker".to_owned();
                while names.contains(&ident) {
                    ident.insert(0, '_');
                }
                let ident = format_ident!("{}", ident);
                fields.push(pq! {
                    #[doc(hidden)]
                    #ident: ::core::marker::PhantomData<#Subject>
                });
                marker = Some(ident);
            }
            StructStyle::Unnamed => {
                let marker = pq! {
                    #[doc(hidden)]
                    ::core::marker::PhantomData<#Subject>
                };
                fields.push(marker);
            }
            _ => {}
        }
    }

    let mut output = vec![q! { #optionized }];
    let optionized = &optionized.ident;

    let marker = marked.then(|| {
        if let Some(marker) = marker {
            q! { #marker: ::core::marker::PhantomData, }
        } else {
            q! { ::core::marker::PhantomData }
        }
    });

    {
        let subject = &format_ident!("subject");

        let optionize = {
            let optionizes = optionized_fields.iter().map(|field| Optionize {
                field,
                subject,
                named,
            });

            construct!(style, [Self] #(#optionizes)* #marker )
        };
        let patches = optionized_fields
            .iter()
            .map(|field| Patch { field, subject });
        let other = &format_ident!("other");
        let merges = optionized_fields.iter().map(|field| Merge { field, other });

        output.push(q! {
            impl #impl_generics ::optionize::PartialOptionized<#Subject> for #optionized #type_generics #where_clause {
                fn optionize(#subject: #Subject) -> Self { #optionize }
                fn patch(self, #subject: &mut #Subject) { #(#patches)* }
                fn merge(&mut self, #other: Self) { #(#merges)* }
            }
        });
    }

    if !partial || upgradable {
        let failed = &format_ident!("failed");
        let errors = &format_ident!("errors");

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

            construct!(style, [#original] #(#oks)*)
        };

        let err = {
            let errs = optionized_fields
                .iter()
                .map(|field| UpgradeErr { field, named });

            construct!(style, [Self] #(#errs)* #marker)
        };

        output.push(q! {
            #[allow(non_snake_case)]
            impl #impl_generics ::optionize::Optionized<#Subject> for #optionized #type_generics #where_clause {
                type UpgradeErrors = ::optionize::UpgradeErrorCollection;
                fn upgrade(self) -> ::core::result::Result<#Subject, (Self::UpgradeErrors, Self)> {
                    let mut #failed = false;
                    let mut #errors = ::optionize::UpgradeErrorCollection::default();
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

#[derive(FromMeta)]
struct OptionizedArgs {}

pub fn proc(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let args = NestedMeta::parse_meta_list(args)?;
    let _ = OptionizedArgs::from_list(&args)?;

    let input = qs! { input.span() =>
        #[derive(::optionize::__private::Optionize)]
        #input
    };

    Ok(input)
}
