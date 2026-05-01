use darling::ast::NestedMeta;
use darling::util::Override;
use darling::{FromAttributes, FromMeta};
use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use std::collections::HashSet;
use std::iter::zip;
use std::mem::take;
use syn::parse::Result;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{
    parse, parse_quote, Error, Expr, Field, GenericArgument, Index, ItemStruct, Meta, PathArguments,
    Type, Visibility,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum UpgradeError {
    Missing,
    Nested,
    MissingRenamed,
    RenamedNested,
}

impl UpgradeError {
    fn variant(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Missing => quote! {
                MissingField(&'static str),
            },
            Self::Nested => quote! {
                NestedError {
                    field: &'static str,
                    source: ::optionize::__private::alloc::boxed::Box<dyn ::core::error::Error + 'static>,
                },
            },
            Self::MissingRenamed => quote! {
                MissingRenamedField {
                    original: &'static str,
                    optionized: &'static str,
                },
            },
            Self::RenamedNested => quote! {
                RenamedNestedError {
                    original: &'static str,
                    optionized: &'static str,
                    source: ::optionize::__private::alloc::boxed::Box<dyn ::core::error::Error + 'static>,
                },
            },
        }
    }

    fn display(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Missing => quote! {
                Self::MissingField(field) => write!(f, "Missing required field for upgrade: {}", field),
            },
            Self::Nested => quote! {
                Self::NestedError { field, .. } => write!(f, "Failed to upgrade nested field `{}`", field),
            },
            Self::MissingRenamed => quote! {
                Self::MissingRenamedField { original, optionized } => write!(
                    f,
                    "Missing required field for upgrade: optionized field `{}` -> original field `{}`",
                    original, optionized
                ),
            },
            Self::RenamedNested => quote! {
                Self::RenamedNestedError { original, optionized, .. } => write!(
                    f,
                    "Failed to upgrade nested field: optionized field `{}` -> original field `{}`",
                    optionized, original
                ),
            },
        }
    }

    fn source(&self) -> Option<proc_macro2::TokenStream> {
        match self {
            Self::Nested => Some(quote! {
                Self::NestedError { source, .. } => ::core::option::Option::Some(&**source),
            }),
            Self::RenamedNested => Some(quote! {
                Self::RenamedNestedError { source, .. } => ::core::option::Option::Some(&**source),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct UpgradeErrorBuilder {
    errors: HashSet<UpgradeError>,
}

impl UpgradeErrorBuilder {
    fn update(&mut self, renamed: bool, nest: bool) {
        let error = match (renamed, nest) {
            (false, false) => UpgradeError::Missing,
            (false, true) => UpgradeError::Nested,
            (true, false) => UpgradeError::MissingRenamed,
            (true, true) => UpgradeError::RenamedNested,
        };
        self.errors.insert(error);
    }

    fn build(&self, vis: &Visibility, ident: &Ident) -> proc_macro2::TokenStream {
        if self.errors.is_empty() {
            return quote! {
                #vis type #ident = ::core::convert::Infallible;
            };
        }

        let variant = self.errors.iter().map(UpgradeError::variant);
        let display = self.errors.iter().map(UpgradeError::display);
        let source = self.errors.iter().filter_map(UpgradeError::source);

        quote! {
            #[derive(Debug)]
            #vis enum #ident {
                #(#variant)*
            }

            impl ::core::error::Error for #ident {
                fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                    match self {
                        #(#source)*
                        _ => ::core::option::Option::None,
                    }
                }
            }

            impl ::core::fmt::Display for #ident {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    match self {
                        #(#display)*
                    }
                }
            }
        }
    }
}

struct FieldMeta {
    original_field: proc_macro2::TokenStream,
    optionized_field: proc_macro2::TokenStream,
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

#[derive(Default)]
struct StructMeta {
    fields: Vec<FieldMeta>,
    skip: Vec<Type>,
    nest: Vec<(Type, Type)>,
}

impl StructMeta {
    fn extract(
        fields: &mut Punctuated<Field, Comma>,
        args: Vec<FieldArgs>,
        partial: bool,
    ) -> Result<Self> {
        let mut this = Self::default();

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

                this.skip.push(ty.clone());
                this.fields.push(field);
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
                    let index = Index::from(i - this.skip.len());
                    quote! { #index }
                }
            };

            let (ty, nest) = if let Some(nest) = &args.nest {
                this.nest.push((ty.clone(), nest.clone()));
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

            this.fields.push(FieldMeta {
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
}

pub fn proc(args: TokenStream, input: TokenStream) -> Result<TokenStream> {
    let struct_args = {
        let struct_args = NestedMeta::parse_meta_list(args.into())?;
        StructArgs::from_list(&struct_args)?
    };

    let (partial, upgradable) = match &struct_args.partial {
        Some(Override::Inherit) => (true, false),
        Some(Override::Explicit(p)) => (true, p.upgradable),
        None => (false, false),
    };

    let mut original = parse::<ItemStruct>(input)?;

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

    let meta = StructMeta::extract(fields, field_args, partial)?;
    let fields = meta.fields.iter().filter(|f| !f.skip).collect::<Vec<_>>();

    let mut optionize = Vec::with_capacity(fields.len());
    let mut patch = Vec::with_capacity(fields.len());
    let mut merge = Vec::with_capacity(fields.len());

    for field in &fields {
        let FieldMeta {
            original_field,
            optionized_field,
            wrap,
            nest,
            ..
        } = field;

        let optionize_value = match (wrap, nest) {
            (true, true) => {
                patch.push(quote! {
                    if let Some(value) = self.#optionized_field {
                        ::optionize::PartialOptionized::patch(value, &mut subject.#original_field);
                    }
                });
                merge.push(quote! {
                    match (&mut self.#optionized_field, other.#optionized_field) {
                        (Some(this), Some(other)) => ::optionize::PartialOptionized::merge(this, other),
                        (None, Some(other)) => self.#optionized_field = Some(other),
                        _ => {}
                    }
                });
                quote! { ::core::option::Option::Some(::optionize::PartialOptionized::optionize(subject.#original_field)) }
            }
            (true, false) => {
                patch.push(quote! {
                    if let Some(value) = self.#optionized_field {
                        subject.#original_field = value;
                    }
                });
                merge.push(quote! {
                    if other.#optionized_field.is_some() {
                        self.#optionized_field = other.#optionized_field;
                    }
                });
                quote! { ::core::option::Option::Some(subject.#original_field) }
            }
            (false, true) => {
                patch.push(quote! { ::optionize::PartialOptionized::patch(self.#optionized_field, &mut subject.#original_field); });
                merge.push(quote! { ::optionize::PartialOptionized::merge(&mut self.#optionized_field, other.#optionized_field); });
                quote! { ::optionize::PartialOptionized::optionize(subject.#original_field) }
            }
            (false, false) => {
                patch.push(quote! { subject.#original_field = self.#optionized_field; });
                merge.push(quote! { self.#optionized_field = other.#optionized_field; });
                quote! { subject.#original_field }
            }
        };

        optionize.push(if named {
            quote! { #optionized_field: #optionize_value, }
        } else {
            quote! { #optionize_value, }
        });
    }

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
        syn::Fields::Named(_) => quote! { Self { #(#optionize)* } },
        syn::Fields::Unnamed(_) => quote! { Self ( #(#optionize)* ) },
        syn::Fields::Unit => quote! { Self },
    };

    output.push(quote! {
        impl #impl_generics ::optionize::PartialOptionized for #optionized_ident #type_generics #where_clause {
            type Subject = #subject;
            fn optionize(subject: Self::Subject) -> Self { #optionize }
            fn patch(self, subject: &mut Self::Subject) { #(#patch)* }
            fn merge(&mut self, other: Self) { #(#merge)* }
        }
    });

    if !partial || upgradable {
        let error_ident = Ident::new(
            &format!("{}UpgradeError", optionized_ident),
            optionized_ident.span(),
        );
        let mut error_builder = UpgradeErrorBuilder::default();
        let mut upgrade = Vec::with_capacity(fields.len());

        upgrade.push(quote! { let mut errors = ::optionize::__private::alloc::vec::Vec::new(); });

        for field in &fields {
            let FieldMeta {
                optionized_field,
                original_name,
                optionized_name,
                wrap,
                nest,
                local,
                ..
            } = field;

            let (renamed, missing_err, nest_err) = if original_name == optionized_name {
                (
                    false,
                    quote! { #error_ident::MissingField(#original_name) },
                    quote! { |e| #error_ident::NestedError { field: #original_name, source: ::optionize::__private::alloc::boxed::Box::new(e) as _ } },
                )
            } else {
                (
                    true,
                    quote! { #error_ident::MissingRenamedField { original: #original_name, optionized: #optionized_name } },
                    quote! { |e| #error_ident::RenamedNestedError { original: #original_name, optionized: #optionized_name, source: ::optionize::__private::alloc::boxed::Box::new(e) as _ } },
                )
            };

            error_builder.update(renamed, *nest);

            upgrade.push(quote! { let #local = self.#optionized_field; });

            upgrade.push(match (wrap, nest) {
                (true, true) => quote! {
                    let #local = match #local {
                        ::core::option::Option::Some(v) => match ::optionize::Upgradable::upgrade(v) {
                            ::core::result::Result::Ok(upgraded) => ::core::result::Result::Ok(upgraded),
                            ::core::result::Result::Err((e, v)) => {
                                errors.extend(e.into_iter().map(#nest_err));
                                ::core::result::Result::Err(::core::option::Option::Some(v))
                            }
                        },
                        ::core::option::Option::None => {
                            errors.push(#missing_err);
                            ::core::result::Result::Err(::core::option::Option::None)
                        }
                    };
                },
                (true, false) => quote! {
                    let #local = match #local {
                        ::core::option::Option::Some(v) => ::core::result::Result::Ok(v),
                        ::core::option::Option::None => {
                            errors.push(#missing_err);
                            ::core::result::Result::Err(::core::option::Option::None)
                        }
                    };
                },
                (false, true) => quote! {
                    let #local = match ::optionize::Upgradable::upgrade(#local) {
                        ::core::result::Result::Ok(v) => ::core::result::Result::Ok(v),
                        ::core::result::Result::Err((e, v)) => {
                            errors.extend(e.into_iter().map(#nest_err));
                            ::core::result::Result::Err(v)
                        }
                    };
                },
                (false, false) => quote! {
                    let #local = ::core::result::Result::Ok(#local);
                }
            });
        }

        output.push(error_builder.build(&original.vis, &error_ident));


        let ok = {
            let mut ok_fields = Vec::with_capacity(meta.fields.len());
            for field in &meta.fields {
                let FieldMeta {
                    original_field,
                    skip,
                    upgrade,
                    local,
                    ..
                } = field;

                if *skip {
                    if named {
                        ok_fields.push(quote! { #original_field: #upgrade, });
                    } else {
                        ok_fields.push(quote! { #upgrade, });
                    }
                    continue;
                }

                let local = quote! { match #local { ::core::result::Result::Ok(v) => v, _ => unreachable!() } };

                if named {
                    ok_fields.push(quote! { #original_field: #local, });
                } else {
                    ok_fields.push(quote! { #local, });
                }
            }

            match &original.fields {
                syn::Fields::Named(_) => quote! { #original_ident { #(#ok_fields)* } },
                syn::Fields::Unnamed(_) => quote! { #original_ident ( #(#ok_fields)* ) },
                syn::Fields::Unit => quote! { #original_ident },
            }
        };

        let err = {
            let mut err_fields = Vec::with_capacity(fields.len());
            for field in &fields {
                let FieldMeta {
                    optionized_field,
                    wrap,
                    nest,
                    local,
                    ..
                } = field;

                let rollback = match (wrap, nest) {
                    (true, true) => quote! {
                        match #local {
                            ::core::result::Result::Ok(v) => ::core::option::Option::Some(::optionize::Optionizable::downgrade(v)),
                            ::core::result::Result::Err(v) => v,
                        }
                    },
                    (true, false) => quote! {
                        match #local {
                            ::core::result::Result::Ok(v) => ::core::option::Option::Some(v),
                            ::core::result::Result::Err(v) => v,
                        }
                    },
                    (false, true) => quote! {
                        match #local {
                            ::core::result::Result::Ok(v) => ::optionize::Optionizable::downgrade(v),
                            ::core::result::Result::Err(v) => v,
                        }
                    },
                    (false, false) => quote! {
                        match #local {
                            ::core::result::Result::Ok(v) => v,
                            ::core::result::Result::Err(v) => v,
                        }
                    },
                };

                if named {
                    err_fields.push(quote! { #optionized_field: #rollback, });
                } else {
                    err_fields.push(quote! { #rollback, });
                }
            }

            if named {
                quote! { Self { #(#err_fields)* } }
            } else {
                quote! { Self ( #(#err_fields)* ) }
            }
        };

        upgrade.push(quote! {
            if errors.is_empty() {
                ::core::result::Result::Ok(#ok)
            } else {
                ::core::result::Result::Err((errors, #err))
            }
        });

        output.push(quote! {
            #[allow(non_snake_case)]
            impl #impl_generics ::optionize::Upgradable<#subject> for #optionized_ident #type_generics #where_clause {
                type Error = #error_ident;
                fn upgrade(self) -> ::core::result::Result<#subject, (::optionize::__private::alloc::vec::Vec<Self::Error>, Self)> {
                    #(#upgrade)*
                }
            }
        });
    }

    Ok(quote! { #(#output)* }.into())
}
