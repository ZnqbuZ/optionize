use darling::ast::NestedMeta;
use darling::util::{Override, SpannedValue};
use darling::{Error, Result};
use darling::{FromAttributes, FromMeta};
use proc_macro2::Span;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote_spanned as qs, ToTokens};
use std::default::Default;
use std::iter::zip;
use std::mem::take;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    parse2, parse_quote_spanned as pqs, parse_str, Expr, Field, GenericArgument, Index, ItemStruct, LitBool, LitStr,
    Meta, PathArguments, Type,
};

#[derive(Debug, Default)]
struct MetaList(Vec<Meta>);

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
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct StructArgs {
    name: Option<LitStr>,
    attributes: Option<MetaList>,
    partial: Option<Override<PartialArgs>>,
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct SkipArgs {
    upgrade: Option<Expr>,
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(optionize), and_then = "FieldArgs::validate")]
struct FieldArgs {
    name: Option<LitStr>,
    attributes: Option<MetaList>,
    wrap: Option<LitBool>,
    nest: Option<Type>,
    skip: Option<SpannedValue<Override<SkipArgs>>>,
}

impl FieldArgs {
    fn validate(self) -> Result<Self> {
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
    Optionize { wrap: bool, nest: bool },
}

impl Default for FieldStrategy {
    fn default() -> Self {
        Self::Optionize {
            wrap: true,
            nest: false,
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
                let name = name.value().replace("{}", &ident.to_string());
                let Ok(mut ident) = parse_str::<Ident>(&name) else {
                    errors.push(
                        Error::custom(format!("`{}` is not a valid identifier", name))
                            .with_span(&span),
                    );
                    continue;
                };
                ident.set_span(span);
                field.ident = Some(ident.clone());
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

            let (ty, nest) = if let Some(nest) = &args.nest {
                (nest, true)
            } else {
                (ty, false)
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

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let mut optionize = if *nest {
            q! { ::optionize::PartialOptionized::optionize(subject.#original) }
        } else {
            q! { subject.#original }
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

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let patch = if *wrap {
            q! { v }
        } else {
            q! { self.#optionized }
        };
        let mut patch = if *nest {
            q! { ::optionize::PartialOptionized::patch(#patch, &mut subject.#original); }
        } else {
            q! { subject.#original = #patch; }
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
}

impl<'l> ToTokens for Merge<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                optionized,
                strategy,
            }
        }

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let merge = match (wrap, nest) {
            (true, true) => q! {
                match (&mut self.#optionized, other.#optionized) {
                    (Some(this), Some(other)) => ::optionize::PartialOptionized::merge(this, other),
                    (None, Some(other)) => self.#optionized = Some(other),
                    _ => {}
                }
            },
            (true, false) => q! {
                if other.#optionized.is_some() {
                    self.#optionized = other.#optionized;
                }
            },
            (false, true) => q! {
                ::optionize::PartialOptionized::merge(&mut self.#optionized, other.#optionized);
            },
            (false, false) => q! {
                self.#optionized = other.#optionized;
            },
        };

        tokens.extend(merge);
    }
}

struct Upgrade<'l> {
    field: &'l FieldIr,
    original: &'l Ident,
    optionized: &'l Ident,
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

        let errors = self.errors;

        tokens.extend(q! { let #local = self.#optionized; });

        let mut expr = if *nest {
            let err = if *wrap {
                q!(::core::option::Option::Some(v))
            } else {
                q!(v)
            };
            q! {
                ::optionize::Optionized::upgrade(#local).map_err(|(e, v)| {
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
                let local = q! { unsafe { ::core::result::Result::unwrap_unchecked(#local) } };

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
        if *nest {
            ok = q! { ::optionize::Optionizable::downgrade(#ok) };
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

pub fn proc(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let struct_args = {
        let struct_args = NestedMeta::parse_meta_list(args)?;
        StructArgs::from_list(&struct_args)?
    };

    let (partial, upgradable) = match &struct_args.partial {
        Some(Override::Inherit) => (true, false),
        Some(Override::Explicit(p)) => (true, p.upgradable),
        None => (false, false),
    };

    let mut original = parse2::<ItemStruct>(input)?;
    let _span = original.span();
    span!(_span);

    let field_args = {
        let mut field_args = Vec::with_capacity(original.fields.len());
        let mut errors = Error::accumulator();

        for field in original.fields.iter_mut() {
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

    let mut optionized = original.clone();

    optionized.ident = {
        let ident = optionized.ident;
        let (span, name) = match struct_args.name {
            Some(name) => (name.span(), name.value().replace("{}", &ident.to_string())),
            None => (ident.span(), format!("{}Optional", ident)),
        };
        let mut ident = parse_str::<Ident>(&name)
            .map_err(|_| syn::Error::new(span, format!("`{}` is not a valid identifier", name)))?;
        ident.set_span(span);
        ident
    };

    if let Some(attrs) = struct_args.attributes {
        optionized.attrs = attrs
            .0
            .into_iter()
            .map(|meta| pqs! { meta.span() => #[#meta] })
            .collect();
    } else {
        optionized
            .attrs
            .retain(|attr| !attr.path().is_ident("optionize"));
    }

    let (fields, named) = match &mut optionized.fields {
        syn::Fields::Named(fields) => (&mut fields.named, true),
        syn::Fields::Unnamed(fields) => (&mut fields.unnamed, false),
        syn::Fields::Unit => return Ok(Default::default()),
    };

    let original_fields = FieldIr::extract(fields, field_args, partial)?;
    let optionized_fields = original_fields
        .iter()
        .filter(|f| matches!(f.strategy, FieldStrategy::Optionize { .. }))
        .collect::<Vec<_>>();

    let mut output = vec![q! {
        #original
        #optionized
    }];

    let generics = optionized.generics;
    let original = original.ident;
    let optionized = optionized.ident;

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let subject = q! { #original #type_generics };

    let optionize = {
        let optionizes = optionized_fields
            .iter()
            .map(|field| Optionize { field, named });
        if named {
            q! { Self { #(#optionizes)* } }
        } else {
            q! { Self ( #(#optionizes)* ) }
        }
    };
    let patches = optionized_fields.iter().map(|field| Patch { field });
    let merges = optionized_fields.iter().map(|field| Merge { field });

    output.push(q! {
        impl #impl_generics ::optionize::PartialOptionized<#subject> for #optionized #type_generics #where_clause {
            fn optionize(subject: #subject) -> Self { #optionize }
            fn patch(self, subject: &mut #subject) { #(#patches)* }
            fn merge(&mut self, other: Self) { #(#merges)* }
        }
    });

    if !partial || upgradable {
        let errors = format_ident!("errors");

        let upgrades = optionized_fields.iter().map(|field| Upgrade {
            field,
            original: &original,
            optionized: &optionized,
            errors: &errors,
        });

        let ok = {
            let oks = original_fields
                .iter()
                .map(|field| UpgradeOk { field, named });

            if named {
                q! { #original { #(#oks)* } }
            } else {
                q! { #original ( #(#oks)* ) }
            }
        };

        let err = {
            let errs = optionized_fields
                .iter()
                .map(|field| UpgradeErr { field, named });

            if named {
                q! { Self { #(#errs)* } }
            } else {
                q! { Self ( #(#errs)* ) }
            }
        };

        output.push(q! {
            #[allow(non_snake_case)]
            impl #impl_generics ::optionize::Optionized<#subject> for #optionized #type_generics #where_clause {
                type UpgradeErrors = ::optionize::UpgradeErrorCollection;
                fn upgrade(self) -> ::core::result::Result<#subject, (Self::UpgradeErrors, Self)> {
                    let mut #errors = ::optionize::UpgradeErrorCollection::default();
                    #(#upgrades)*
                    if #errors.is_empty() {
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
