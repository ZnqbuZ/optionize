use darling::ast::NestedMeta;
use darling::util::Override;
use darling::{FromAttributes, FromMeta};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
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
#[darling(default, attributes(optionize))]
struct FieldArgs {
    name: Option<String>,
    attributes: Option<MetaList>,
    wrap: Option<bool>,
    nest: Option<Type>,
    skip: Option<Override<SkipArgs>>,
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

struct FieldMeta {
    original_field: TokenStream,
    optionized_field: TokenStream,
    original_name: String,
    optionized_name: String,
    wrap: bool,
    nest: bool,
    skip: bool,
    upgrade: Expr,
    local: Ident,
}

impl Default for FieldMeta {
    fn default() -> Self {
        Self {
            original_field: quote! {},
            optionized_field: quote! {},
            original_name: String::new(),
            optionized_name: String::new(),
            wrap: false,
            nest: false,
            skip: false,
            upgrade: parse_quote! { ::core::default::Default::default() },
            local: format_ident!("_"),
        }
    }
}

impl FieldMeta {
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

                let mut field = FieldMeta {
                    original_field,
                    skip: true,
                    ..Default::default()
                };

                if let Some(upgrade) = upgrade {
                    field.upgrade = upgrade;
                }

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

            this.push(FieldMeta {
                original_field: original_field.clone(),
                optionized_field: optionized_field.clone(),
                original_name: original_field.to_string(),
                optionized_name: optionized_field.to_string(),
                wrap,
                nest,
                local,
                ..Default::default()
            });

            fields.push(field);
        }

        Ok(this)
    }

    fn patch(&self) -> TokenStream {
        let Self {
            original_field,
            optionized_field,
            wrap,
            nest,
            ..
        } = self;

        let patch = if *wrap {
            quote! { v }
        } else {
            quote! { self.#optionized_field }
        };
        let mut patch = if *nest {
            quote! { ::optionize::PartialOptionized::patch(#patch, &mut subject.#original_field); }
        } else {
            quote! { subject.#original_field = #patch; }
        };
        if *wrap {
            patch = quote! {
                if let Some(v) = self.#optionized_field {
                    #patch
                }
            }
        };

        patch
    }

    fn merge(&self) -> TokenStream {
        let Self {
            optionized_field,
            wrap,
            nest,
            ..
        } = self;

        match (wrap, nest) {
            (true, true) => quote! {
                match (&mut self.#optionized_field, other.#optionized_field) {
                    (Some(this), Some(other)) => ::optionize::PartialOptionized::merge(this, other),
                    (None, Some(other)) => self.#optionized_field = Some(other),
                    _ => {}
                }
            },
            (true, false) => quote! {
                if other.#optionized_field.is_some() {
                    self.#optionized_field = other.#optionized_field;
                }
            },
            (false, true) => quote! {
                ::optionize::PartialOptionized::merge(&mut self.#optionized_field, other.#optionized_field);
            },
            (false, false) => quote! {
                self.#optionized_field = other.#optionized_field;
            },
        }
    }

    fn optionize(&self, named: bool) -> TokenStream {
        let Self {
            original_field,
            optionized_field,
            wrap,
            nest,
            ..
        } = self;

        let mut optionize = if *nest {
            quote! { ::optionize::PartialOptionized::optionize(subject.#original_field) }
        } else {
            quote! { subject.#original_field }
        };

        if *wrap {
            optionize = quote! { ::core::option::Option::Some(#optionize) }
        };

        if named {
            quote! { #optionized_field: #optionize, }
        } else {
            quote! { #optionize, }
        }
    }

    fn upgrade(&self, original: &Ident, optionized: &Ident, errors: &Ident) -> TokenStream {
        let Self {
            optionized_field,
            original_name,
            optionized_name,
            wrap,
            nest,
            local,
            ..
        } = self;

        let renamed = original_name == optionized_name;

        let (missing_err, nest_map_err) = {
            let original = original.to_string();
            let optionized = optionized.to_string();

            let ty = quote! {
                ::optionize::TypeInfo {
                    original: #original,
                    optionized: #optionized,
                }
            };

            let field = if renamed {
                quote! { ::optionize::FieldInfo::Identical ( #original_name ) }
            } else {
                quote! { ::optionize::FieldInfo::Renamed { original: #original_name, optionized: #optionized_name } }
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

        let mut upgrade = quote! { let #local = self.#optionized_field; };

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

        upgrade.extend(quote! { let #local = #expr; });
        upgrade
    }

    fn ok(&self, named: bool) -> TokenStream {
        let Self {
            original_field,
            skip,
            upgrade,
            local,
            ..
        } = self;

        if *skip {
            return if named {
                quote! { #original_field: #upgrade, }
            } else {
                quote! { #upgrade, }
            };
        }

        let local = quote! {
            match #local {
                ::core::result::Result::Ok(v) => v,
                _ => unreachable!()
            }
        };

        if named {
            quote! { #original_field: #local, }
        } else {
            quote! { #local, }
        }
    }

    fn err(&self, named: bool) -> TokenStream {
        let Self {
            optionized_field,
            wrap,
            nest,
            local,
            ..
        } = self;

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

        if named {
            quote! { #optionized_field: #rollback, }
        } else {
            quote! { #rollback, }
        }
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
            if let Some(parsed) = errors.handle(FieldArgs::from_attributes(&field.attrs)) {
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

    let original_fields = FieldMeta::extract(fields, field_args, partial)?;
    let optionized_fields = original_fields
        .iter()
        .filter(|f| !f.skip)
        .collect::<Vec<_>>();

    let (optionizes, patches, merges) = optionized_fields.iter().fold(
        (
            Vec::with_capacity(optionized_fields.len()),
            Vec::with_capacity(optionized_fields.len()),
            Vec::with_capacity(optionized_fields.len()),
        ),
        |(mut optionizes, mut patches, mut merges), field| {
            optionizes.push(field.optionize(named));
            patches.push(field.patch());
            merges.push(field.merge());
            (optionizes, patches, merges)
        },
    );

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

        let upgrades = optionized_fields
            .iter()
            .map(|f| f.upgrade(original_ident, optionized_ident, &errors));

        let ok = {
            let oks = original_fields.iter().map(|f| f.ok(named));

            match &original.fields {
                syn::Fields::Named(_) => quote! { #original_ident { #(#oks)* } },
                syn::Fields::Unnamed(_) => quote! { #original_ident ( #(#oks)* ) },
                syn::Fields::Unit => quote! { #original_ident },
            }
        };

        let err = {
            let errs = optionized_fields.iter().map(|f| f.err(named));

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
