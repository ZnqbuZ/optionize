use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, ToTokens};
use std::iter::zip;
use std::mem::take;
use syn::parse::{Parse, ParseStream, Parser, Result};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, parse_quote, parse_str, Attribute, Error, Expr, GenericArgument, Index, ItemStruct, Lit,
    Meta, PathArguments, Token, Type,
};

macro_rules! bail {
    ($tokens:expr, $message:expr) => {
        return Err(Error::new_spanned($tokens, $message))
    };
}

struct StructArgs {
    name: String,
    attributes: Option<Vec<Meta>>,
}

impl Parse for StructArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let metas = input.parse_terminated(Meta::parse, Token![,])?;

        let mut name = None;
        let mut attributes = None::<Vec<_>>;

        for meta in &metas {
            match meta {
                Meta::NameValue(nv) if nv.path.is_ident("name") => {
                    if let Expr::Lit(expr_lit) = &nv.value
                        && let Lit::Str(lit_str) = &expr_lit.lit
                    {
                        name = Some(lit_str.value());
                    } else {
                        bail!(nv, "expected string literal");
                    }
                }
                Meta::List(meta_list) if meta_list.path.is_ident("attributes") => {
                    let Ok(nested) =
                        meta_list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    else {
                        bail!(meta_list, "invalid attributes format");
                    };
                    attributes.get_or_insert_default().extend(nested);
                }
                Meta::Path(path) if let Some(ident) = path.get_ident() => {
                    name = Some(ident.to_string());
                }
                _ => bail!(meta, "unrecognized optionize argument"),
            }
        }

        let name = name.ok_or_else(|| input.error("a name must be specified"))?;

        Ok(Self { name, attributes })
    }
}

#[derive(Default)]
struct FieldArgs {
    name: Option<String>,
    attributes: Option<Vec<Meta>>,
    wrapped: Option<bool>,
    nested: Option<String>,
    skip: bool,
}

impl FieldArgs {
    fn extract(attributes: &mut Vec<Attribute>) -> Result<Self> {
        let mut args = Self::default();

        for attribute in take(attributes) {
            if !attribute.path().is_ident("optionize") {
                attributes.push(attribute);
                continue;
            }

            let metas =
                attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;

            for meta in &metas {
                match meta {
                    Meta::NameValue(nv) if nv.path.is_ident("name") => {
                        if let Expr::Lit(expr_lit) = &nv.value
                            && let Lit::Str(lit_str) = &expr_lit.lit
                        {
                            args.name = Some(lit_str.value());
                        } else {
                            bail!(nv, "expected string literal");
                        }
                    }
                    Meta::List(meta_list) if meta_list.path.is_ident("attributes") => {
                        let Ok(nested) = meta_list
                            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                        else {
                            bail!(meta_list, "invalid attributes format");
                        };
                        args.attributes.get_or_insert_default().extend(nested);
                    }
                    Meta::NameValue(nv) if nv.path.is_ident("wrapped") => {
                        if let Expr::Lit(expr_lit) = &nv.value
                            && let Lit::Bool(lit_bool) = &expr_lit.lit
                        {
                            args.wrapped = Some(lit_bool.value);
                        } else {
                            bail!(nv, "expected boolean literal");
                        }
                    }
                    Meta::NameValue(nv) if nv.path.is_ident("nested") => {
                        if let Expr::Lit(expr_lit) = &nv.value
                            && let Lit::Str(lit_str) = &expr_lit.lit
                        {
                            args.nested = Some(lit_str.value());
                        } else {
                            bail!(nv, "expected string literal");
                        }
                    }
                    Meta::Path(path) if path.is_ident("skip") => {
                        args.skip = true;
                    }
                    _ => bail!(meta, "unrecognized optionize argument"),
                }
            }
        }

        Ok(args)
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

    let segments = path
        .path
        .segments
        .iter()
        .map(|s| &s.ident)
        .collect::<Vec<_>>();

    match segments.as_slice() {
        [ident] if *ident == "Option" => {}
        [option, ident] if *option == "option" && *ident == "Option" => {}
        [prefix, option, ident]
            if (*prefix == "std" || *prefix == "core")
                && *option == "option"
                && *ident == "Option" => {}
        _ => return false,
    }

    let Some(segment) = path.path.segments.last() else {
        return false;
    };

    matches!(
        &segment.arguments,
        PathArguments::AngleBracketed(args)
            if args.args.len() == 1
                && matches!(args.args.first(), Some(GenericArgument::Type(_)))
    )
}

pub fn proc(args: TokenStream, input: TokenStream) -> TokenStream {
    let struct_args = parse_macro_input!(args as StructArgs);
    let mut original = parse_macro_input!(input as ItemStruct);

    let fields_args = match original
        .fields
        .iter_mut()
        .map(|field| FieldArgs::extract(&mut field.attrs))
        .collect::<Result<Vec<_>>>()
    {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    let mut optionized = original.clone();

    optionized.ident = {
        let ident = optionized.ident;
        let name = struct_args.name.replace("{}", &ident.to_string());
        Ident::new(&name, ident.span())
    };

    if let Some(attributes) = struct_args.attributes {
        optionized.attrs = attributes
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
        syn::Fields::Unit => return Default::default(),
    };

    let original_ident = &original.ident;
    let optionized_ident = &optionized.ident;
    let error_ident = Ident::new(
        &format!("{}UpgradeError", optionized_ident),
        optionized_ident.span(),
    );

    let mut optionize = proc_macro2::TokenStream::new();
    let mut patch = proc_macro2::TokenStream::new();
    let mut merge = proc_macro2::TokenStream::new();
    let mut upgrade = proc_macro2::TokenStream::new();

    let mut has_renamed = false;
    let mut has_nested = false;
    let mut has_renamed_nested = false;

    let mut skipped_types = Vec::new();
    let mut nested_types = Vec::new();

    for (i, (mut field, args)) in zip(take(fields), fields_args).enumerate() {
        let ident = &field.ident;
        let ty = &field.ty;

        let original_field = match ident {
            Some(ident) => quote! { #ident },
            None => {
                let index = Index::from(i);
                quote! { #index }
            }
        };

        if args.skip {
            skipped_types.push(ty.clone());
            if named {
                upgrade.extend(quote! { #original_field: ::core::default::Default::default(), });
            } else {
                upgrade.extend(quote! { ::core::default::Default::default(), });
            }
            continue;
        }

        if let Some(name) = args.name {
            let Some(ident) = ident else {
                return Error::new_spanned(ty, "cannot rename an unnamed field")
                    .to_compile_error()
                    .into();
            };
            let name = name.replace("{}", &ident.to_string());
            field.ident = Some(Ident::new(&name, ident.span()));
        }

        let optionized_field = match &field.ident {
            Some(ident) => quote! { #ident },
            None => {
                let index = Index::from(i - skipped_types.len());
                quote! { #index }
            }
        };

        let (ty, nested) = if let Some(nested) = &args.nested {
            has_nested = true;
            let nested = nested.replace("{}", &quote!(#ty).to_string());
            (
                match parse_str::<Type>(&nested) {
                    Ok(nested_ty) => {
                        nested_types.push((ty.clone(), nested_ty.clone()));
                        nested_ty
                    }
                    Err(e) => {
                        return Error::new_spanned(&ty, format!("invalid nested type: {}", e))
                            .to_compile_error()
                            .into();
                    }
                },
                true,
            )
        } else {
            (ty.clone(), false)
        };

        let wrapped = args.wrapped.unwrap_or_else(|| !is_option(&ty));
        if wrapped {
            field.ty = parse_quote! { Option<#ty> };
        }

        let original_name = original_field.to_string();
        let optionized_name = optionized_field.to_string();

        let (extract, map_err) = if original_name == optionized_name {
            (
                quote! { self.#optionized_field.ok_or(#error_ident::MissingField(#original_name))? },
                quote! {
                    |e| #error_ident::NestedError {
                        field: #original_name,
                        source: ::std::boxed::Box::new(e) as ::std::boxed::Box<dyn ::std::error::Error + 'static>,
                    }
                },
            )
        } else {
            if nested {
                has_renamed_nested = true;
            } else {
                has_renamed = true;
            }
            (
                quote! {
                    self.#optionized_field.ok_or(#error_ident::MissingRenamedField {
                        original: #original_name,
                        optionized: #optionized_name,
                    })?
                },
                quote! {
                    |e| #error_ident::RenamedNestedError {
                        original: #original_name,
                        optionized: #optionized_name,
                        source: ::std::boxed::Box::new(e) as ::std::boxed::Box<dyn ::std::error::Error + 'static>,
                    }
                },
            )
        };

        let (optionize_value, upgrade_value) = match (wrapped, nested) {
            (true, true) => {
                patch.extend(quote! {
                    if let Some(nested_value) = self.#optionized_field {
                        optionize::Optionized::patch(nested_value, &mut subject.#original_field);
                    }
                });
                merge.extend(quote! {
                    match (&mut self.#optionized_field, other.#optionized_field) {
                        (Some(this), Some(other)) => optionize::Optionized::merge(this, other),
                        (None, Some(other)) => self.#optionized_field = Some(other),
                        _ => {}
                    }
                });
                (
                    quote! {
                        ::core::option::Option::Some(<#ty as optionize::Optionized>::optionize(subject.#original_field))
                    },
                    quote! {
                        optionize::Upgradable::upgrade(#extract).map_err(#map_err)?
                    },
                )
            }
            (true, false) => {
                patch.extend(quote! {
                    if let Some(value) = self.#optionized_field {
                        subject.#original_field = value;
                    }
                });
                merge.extend(quote! {
                    if other.#optionized_field.is_some() {
                        self.#optionized_field = other.#optionized_field;
                    }
                });
                (
                    quote! { ::core::option::Option::Some(subject.#original_field) },
                    extract,
                )
            }
            (false, true) => {
                patch.extend(quote! {
                    optionize::Optionized::patch(self.#optionized_field, &mut subject.#original_field);
                });
                merge.extend(quote! {
                    optionize::Optionized::merge(&mut self.#optionized_field, other.#optionized_field);
                });
                (
                    quote! {
                        <#ty as optionize::Optionized>::optionize(subject.#original_field)
                    },
                    quote! {
                        optionize::Upgradable::upgrade(self.#optionized_field).map_err(#map_err)?
                    },
                )
            }
            (false, false) => {
                patch.extend(quote! {
                    subject.#original_field = self.#optionized_field;
                });
                merge.extend(quote! {
                    self.#optionized_field = other.#optionized_field;
                });
                (
                    quote! { subject.#original_field },
                    quote! { self.#optionized_field },
                )
            }
        };

        if named {
            upgrade.extend(quote! { #original_field: #upgrade_value, });
            optionize.extend(quote! { #optionized_field: #optionize_value, });
        } else {
            upgrade.extend(quote! { #upgrade_value, });
            optionize.extend(quote! { #optionize_value, });
        }

        fields.push(field);
    }

    let mut output = quote! {
        #original
        #optionized
    };

    let mut generics = optionized.generics.clone();
    let where_clause = generics.make_where_clause();
    for (subject_ty, ty) in &nested_types {
        where_clause
            .predicates
            .push(parse_quote! { #ty: optionize::Optionized<Subject = #subject_ty> });
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let optionize = match &original.fields {
        syn::Fields::Named(_) => quote! { Self { #optionize } },
        syn::Fields::Unnamed(_) => quote! { Self ( #optionize ) },
        syn::Fields::Unit => quote! { Self },
    };
    output.extend(quote! {
        impl #impl_generics optionize::Optionized for #optionized_ident #type_generics #where_clause {
            type Subject = #original_ident #type_generics;

            fn optionize(subject: Self::Subject) -> Self {
                #optionize
            }

            fn patch(self, subject: &mut Self::Subject) {
                #patch
            }

            fn merge(&mut self, other: Self) {
                #merge
            }
        }
    });

    let original = match &original.fields {
        syn::Fields::Named(_) => quote! { #original_ident { #upgrade } },
        syn::Fields::Unnamed(_) => quote! { #original_ident ( #upgrade ) },
        syn::Fields::Unit => quote! { #original_ident },
    };

    let where_clause = generics.make_where_clause();
    for (_, ty) in &nested_types {
        where_clause
            .predicates
            .push(parse_quote! { #ty: optionize::Upgradable });
        where_clause.predicates.push(
            parse_quote! { <#ty as optionize::Upgradable>::Error: ::std::error::Error + 'static },
        );
    }
    for ty in &skipped_types {
        where_clause
            .predicates
            .push(parse_quote! { #ty: ::core::default::Default });
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let mut error = quote! {
        MissingField(&'static str),
    };
    let mut error_display = quote! {
        Self::MissingField(field) => write!(f, "Missing required field for upgrade: {}", field),
    };
    let mut error_source = proc_macro2::TokenStream::new();

    if has_renamed {
        error.extend(quote! {
            MissingRenamedField {
                original: &'static str,
                optionized: &'static str,
            },
        });
        error_display.extend(quote! {
            Self::MissingRenamedField { original, optionized } => write!(
                f,
                "Missing required field for upgrade: optionized field `{}` -> original field `{}`",
                original, optionized
            ),
        });
    }

    if has_nested {
        error.extend(quote! {
            NestedError {
                field: &'static str,
                source: ::std::boxed::Box<dyn ::std::error::Error + 'static>,
            },
        });
        error_display.extend(quote! {
            Self::NestedError { field, .. } => write!(
                f,
                "Failed to upgrade nested field `{}`",
                field
            ),
        });
        error_source.extend(quote! {
            Self::NestedError { source, .. } => ::core::option::Option::Some(&**source),
        });
    }

    if has_renamed_nested {
        error.extend(quote! {
            RenamedNestedError {
                original: &'static str,
                optionized: &'static str,
                source: ::std::boxed::Box<dyn ::std::error::Error + 'static>,
            },
        });
        error_display.extend(quote! {
            Self::RenamedNestedError { original, optionized, .. } => write!(
                f,
                "Failed to upgrade nested field: optionized field `{}` -> original field `{}`",
                optionized, original
            ),
        });
        error_source.extend(quote! {
            Self::RenamedNestedError { source, .. } => ::core::option::Option::Some(&**source),
        });
    }

    error_source.extend(quote! {
        _ => ::core::option::Option::None,
    });

    output.extend(quote! {
        #[derive(Debug)]
        pub enum #error_ident {
            #error
        }

        impl ::std::error::Error for #error_ident {
            fn source(&self) -> ::core::option::Option<&(dyn ::std::error::Error + 'static)> {
                match self {
                    #error_source
                }
            }
        }

        impl ::core::fmt::Display for #error_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #error_display
                }
            }
        }

        impl #impl_generics optionize::Upgradable for #optionized_ident #type_generics #where_clause {
            type Error = #error_ident;
            fn upgrade(self) -> ::core::result::Result<Self::Subject, Self::Error> {
                ::core::result::Result::Ok(#original)
            }
        }
    });

    output.into()
}
