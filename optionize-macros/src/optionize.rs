use darling::ast::NestedMeta;
use darling::util::Override;
use darling::{FromAttributes, FromMeta};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, ToTokens};
use std::iter::zip;
use std::mem::take;
use syn::parse::Result;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    parse2, parse_quote, Error, Expr, Field, GenericArgument, Index, ItemStruct, Meta, PathArguments,
    Type,
};

#[derive(Debug, Default)]
struct MetaList(Vec<Meta>);

impl FromMeta for MetaList {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut errors = darling::Error::accumulator();
        let metas = items
            .iter()
            .filter_map(|item| match item {
                NestedMeta::Meta(m) => Some(m.clone()),
                NestedMeta::Lit(l) => {
                    errors.push(darling::Error::unsupported_format("literal").with_span(l));
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
    name: Option<String>,
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
    name: Option<String>,
    attributes: Option<MetaList>,
    wrap: Option<bool>,
    nest: Option<Type>,
    skip: Option<Override<SkipArgs>>,
}

impl FieldArgs {
    fn validate(self) -> darling::Result<Self> {
        if self.skip.is_some() {
            let mut errors = darling::Error::accumulator();

            if self.wrap.is_some() {
                errors.push(darling::Error::custom("`wrap` cannot be used with `skip`"));
            }
            if self.nest.is_some() {
                errors.push(darling::Error::custom("`nest` cannot be used with `skip`"));
            }

            return errors.finish_with(self);
        }

        Ok(self)
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
    original: TokenStream,
    optionized: TokenStream,
    strategy: FieldStrategy,
    local: Ident,
}

impl Default for FieldIr {
    fn default() -> Self {
        Self {
            original: quote! {},
            optionized: quote! {},
            strategy: FieldStrategy::default(),
            local: format_ident!("_"),
        }
    }
}

impl FieldIr {
    fn extract(
        fields: &mut Punctuated<Field, Comma>,
        args: Vec<FieldArgs>,
        partial: bool,
    ) -> Result<Vec<Self>> {
        let mut this = Vec::new();
        let mut skipped = 0;

        for (i, (mut field, args)) in zip(take(fields), args).enumerate() {
            let ident = &field.ident;
            let ty = &field.ty;

            let original_field = match ident {
                Some(ident) => quote! { #ident },
                None => {
                    let index = Index {
                        index: i as u32,
                        span: field.ty.span(),
                    };
                    quote! { #index }
                }
            };

            let (skip, upgrade) = match args.skip {
                Some(Override::Inherit) => (true, None),
                Some(Override::Explicit(s)) => (true, s.upgrade),
                None => (false, None),
            };

            if skip {
                if !partial {
                    return Err(Error::new_spanned(
                        &field,
                        "`skip` attribute is only allowed when `partial` is specified",
                    ));
                }

                let field = FieldIr {
                    original: original_field,
                    strategy: FieldStrategy::Skip {
                        upgrade: upgrade.unwrap_or_else(|| {
                            parse_quote! { ::core::default::Default::default() }
                        }),
                    },
                    ..Default::default()
                };

                skipped += 1;
                this.push(field);
                continue;
            }

            if let Some(name) = args.name {
                let ident = ident
                    .as_ref()
                    .ok_or_else(|| Error::new_spanned(ty, "cannot rename an unnamed field"))?;
                let name = name.replace("{}", &ident.to_string());
                let ident = Ident::new(&name, ident.span());
                field.ident = Some(ident.clone());
            }

            let optionized_field = match &field.ident {
                Some(ident) => quote! { #ident },
                None => {
                    let index = Index::from(i - skipped);
                    quote! { #index }
                }
            };

            let (ty, nest) = if let Some(nest) = &args.nest {
                (nest, true)
            } else {
                (ty, false)
            };

            let wrap = args.wrap.unwrap_or_else(|| !is_option(ty));
            field.ty = if wrap {
                parse_quote! { Option<#ty> }
            } else {
                ty.clone()
            };

            let local = format_ident!(
                "v_{}",
                field
                    .ident
                    .clone()
                    .unwrap_or_else(|| format_ident!("{}", i))
            );

            this.push(FieldIr {
                original: original_field.clone(),
                optionized: optionized_field.clone(),
                strategy: FieldStrategy::Optionize { wrap, nest },
                local,
            });

            fields.push(field);
        }

        Ok(this)
    }
}

struct Optionize<'l> {
    field: &'l FieldIr,
    named: bool,
}

impl<'l> ToTokens for Optionize<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let FieldIr {
            original,
            optionized,
            strategy,
            ..
        } = self.field;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let mut optionize = if *nest {
            quote! { ::optionize::PartialOptionized::optionize(subject.#original) }
        } else {
            quote! { subject.#original }
        };

        if *wrap {
            optionize = quote! { ::core::option::Option::Some(#optionize) }
        };

        let optionize = if self.named {
            quote! { #optionized: #optionize, }
        } else {
            quote! { #optionize, }
        };

        tokens.extend(optionize);
    }
}

struct Patch<'l> {
    field: &'l FieldIr,
}

impl<'l> ToTokens for Patch<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let FieldIr {
            original,
            optionized,
            strategy,
            ..
        } = self.field;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let patch = if *wrap {
            quote! { v }
        } else {
            quote! { self.#optionized }
        };
        let mut patch = if *nest {
            quote! { ::optionize::PartialOptionized::patch(#patch, &mut subject.#original); }
        } else {
            quote! { subject.#original = #patch; }
        };
        if *wrap {
            patch = quote! {
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
        let FieldIr {
            optionized,
            strategy,
            ..
        } = self.field;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let merge = match (wrap, nest) {
            (true, true) => quote! {
                match (&mut self.#optionized, other.#optionized) {
                    (Some(this), Some(other)) => ::optionize::PartialOptionized::merge(this, other),
                    (None, Some(other)) => self.#optionized = Some(other),
                    _ => {}
                }
            },
            (true, false) => quote! {
                if other.#optionized.is_some() {
                    self.#optionized = other.#optionized;
                }
            },
            (false, true) => quote! {
                ::optionize::PartialOptionized::merge(&mut self.#optionized, other.#optionized);
            },
            (false, false) => quote! {
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
        let FieldIr {
            original,
            optionized,
            strategy,
            local,
            ..
        } = self.field;

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

                quote! {
                    ::optionize::TypeInfo {
                        original: #original_str,
                        optionized: #optionized_str,
                    }
                }
            };

            let field = if renamed {
                quote! { ::optionize::FieldInfo::Identical ( #original_str ) }
            } else {
                quote! { ::optionize::FieldInfo::Renamed { original: #original_str, optionized: #optionized_str } }
            };

            (
                quote! {
                    ::optionize::UpgradeError::MissingField {
                        ty: #ty,
                        field: #field
                    }
                },
                quote! {
                    |e| ::optionize::UpgradeError::NestedError {
                        ty: #ty,
                        field: #field,
                        source: ::optionize::__private::alloc::boxed::Box::new(e) as _
                    }
                },
            )
        };

        let errors = self.errors;

        tokens.extend(quote! { let #local = self.#optionized; });

        let mut expr = if *nest {
            let err = if *wrap {
                quote!(::core::option::Option::Some(v))
            } else {
                quote!(v)
            };
            quote! {
                ::optionize::Optionized::upgrade(#local).map_err(|(e, v)| {
                    #errors.extend(e.into_iter().map(#nest_map_err));
                    #err
                })
            }
        } else {
            quote! { ::core::result::Result::Ok(#local) }
        };

        if *wrap {
            expr = quote! {
                match #local {
                    ::core::option::Option::Some(#local) => #expr,
                    ::core::option::Option::None => {
                        #errors.push(#missing_err);
                        ::core::result::Result::Err(::core::option::Option::None)
                    }
                }
            };
        }

        tokens.extend(quote! { let #local = #expr; });
    }
}

struct UpgradeOk<'l> {
    field: &'l FieldIr,
    named: bool,
}

impl<'l> ToTokens for UpgradeOk<'l> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let FieldIr {
            original,
            strategy,
            local,
            ..
        } = self.field;

        let ok = match strategy {
            FieldStrategy::Skip { upgrade } => {
                if self.named {
                    quote! { #original: #upgrade, }
                } else {
                    quote! { #upgrade, }
                }
            }
            FieldStrategy::Optionize { .. } => {
                let local = quote! {
                    match #local {
                        ::core::result::Result::Ok(v) => v,
                        _ => unreachable!()
                    }
                };

                if self.named {
                    quote! { #original: #local, }
                } else {
                    quote! { #local, }
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
        let FieldIr {
            optionized,
            strategy,
            local,
            ..
        } = self.field;

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let mut ok = quote! { v };
        if *nest {
            ok = quote! { ::optionize::Optionizable::downgrade(#ok) };
        }
        if *wrap {
            ok = quote! { ::core::option::Option::Some(#ok) };
        }
        let rollback = quote! {
            match #local {
                ::core::result::Result::Ok(v) => #ok,
                ::core::result::Result::Err(v) => v,
            }
        };

        let err = if self.named {
            quote! { #optionized: #rollback, }
        } else {
            quote! { #rollback, }
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

    let field_args = {
        let mut field_args = Vec::with_capacity(original.fields.len());
        let mut errors = darling::Error::accumulator();

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
        let name = struct_args
            .name
            .unwrap_or_else(|| "{}Optional".to_string())
            .replace("{}", &ident.to_string());
        Ident::new(&name, ident.span())
    };

    if let Some(attributes) = struct_args.attributes {
        optionized.attrs = attributes
            .0
            .into_iter()
            .map(|meta| parse_quote! { #[#meta] })
            .collect();
    }
    optionized
        .attrs
        .retain(|attr| !attr.path().is_ident("optionize"));

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

    let optionizes = optionized_fields
        .iter()
        .map(|field| Optionize { field, named });
    let patches = optionized_fields.iter().map(|field| Patch { field });
    let merges = optionized_fields.iter().map(|field| Merge { field });

    let mut output = vec![quote! {
        #original
        #optionized
    }];

    let generics = optionized.generics.clone();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let original_ident = &original.ident;
    let optionized_ident = &optionized.ident;

    let subject = quote! { #original_ident #type_generics };

    let optionize = match &original.fields {
        syn::Fields::Named(_) => quote! { Self { #(#optionizes)* } },
        syn::Fields::Unnamed(_) => quote! { Self ( #(#optionizes)* ) },
        syn::Fields::Unit => quote! { Self },
    };

    output.push(quote! {
        impl #impl_generics ::optionize::PartialOptionized<#subject> for #optionized_ident #type_generics #where_clause {
            fn optionize(subject: #subject) -> Self { #optionize }
            fn patch(self, subject: &mut #subject) { #(#patches)* }
            fn merge(&mut self, other: Self) { #(#merges)* }
        }
    });

    if !partial || upgradable {
        let errors = format_ident!("errors");

        let upgrades = optionized_fields.iter().map(|field| Upgrade {
            field,
            original: original_ident,
            optionized: optionized_ident,
            errors: &errors,
        });

        let ok = {
            let oks = original_fields
                .iter()
                .map(|field| UpgradeOk { field, named });

            match &original.fields {
                syn::Fields::Named(_) => quote! { #original_ident { #(#oks)* } },
                syn::Fields::Unnamed(_) => quote! { #original_ident ( #(#oks)* ) },
                syn::Fields::Unit => quote! { #original_ident },
            }
        };

        let err = {
            let errs = optionized_fields
                .iter()
                .map(|field| UpgradeErr { field, named });

            if named {
                quote! { Self { #(#errs)* } }
            } else {
                quote! { Self ( #(#errs)* ) }
            }
        };

        output.push(quote! {
            #[allow(non_snake_case)]
            impl #impl_generics ::optionize::Optionized<#subject> for #optionized_ident #type_generics #where_clause {
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

    Ok(quote! { #(#output)* })
}
