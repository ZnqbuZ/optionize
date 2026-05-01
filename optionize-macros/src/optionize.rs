use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use std::iter::zip;
use std::mem::take;
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, parse_quote, Attribute, Error, Expr, GenericArgument, Index, ItemStruct, Lit, Meta,
    PathArguments, Token, Type,
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

    let mut patch = proc_macro2::TokenStream::new();
    let mut merge = proc_macro2::TokenStream::new();
    let mut upgrade = proc_macro2::TokenStream::new();

    let mut renamed = false;
    let mut skipped_types = Vec::new();

    for (i, (mut field, args)) in zip(take(fields), fields_args).enumerate() {
        let original_field = match &field.ident {
            Some(ident) => quote! { #ident },
            None => {
                let index = Index::from(i);
                quote! { #index }
            }
        };

        if args.skip {
            skipped_types.push(field.ty.clone());
            if named {
                upgrade.extend(quote! { #original_field: ::core::default::Default::default(), });
            } else {
                upgrade.extend(quote! { ::core::default::Default::default(), });
            }
            continue;
        }

        if let Some(name) = args.name {
            let Some(ident) = &field.ident else {
                return Error::new_spanned(field.ty, "cannot rename an unnamed field")
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

        let wrapped = args.wrapped.unwrap_or_else(|| !is_option(&field.ty));
        let init = if wrapped {
            let ty = &field.ty;
            field.ty = parse_quote! { Option<#ty> };

            patch.extend(quote! {
                if let Some(value) = self.#optionized_field {
                    target.#original_field = value;
                }
            });
            merge.extend(quote! {
                if other.#optionized_field.is_some() {
                    self.#optionized_field = other.#optionized_field;
                }
            });

            let original_name = original_field.to_string();
            let optionized_name = optionized_field.to_string();
            if original_name == optionized_name {
                quote! {
                    self.#optionized_field.ok_or(#error_ident::MissingField(#original_name))?
                }
            } else {
                renamed = true;
                quote! {
                    self.#optionized_field.ok_or(#error_ident::MissingRenamedField {
                        original: #original_name,
                        optionized: #optionized_name,
                    })?
                }
            }
        } else {
            patch.extend(quote! {
                target.#original_field = self.#optionized_field;
            });
            merge.extend(quote! {
                self.#optionized_field = other.#optionized_field;
            });

            quote! { self.#optionized_field }
        };

        if named {
            upgrade.extend(quote! { #original_field: #init, });
        } else {
            upgrade.extend(quote! { #init, });
        }

        fields.push(field);
    }

    let (impl_generics, type_generics, where_clause) = optionized.generics.split_for_impl();
    let mut output = quote! {
        #original
        #optionized

        impl #impl_generics optionize::Optionized for #optionized_ident #type_generics #where_clause {
            type Target = #original_ident #type_generics;

            fn patch(self, target: &mut Self::Target) {
                #patch
            }

            fn merge(&mut self, other: Self) {
                #merge
            }
        }
    };

    let original = match &original.fields {
        syn::Fields::Named(_) => quote! { #original_ident { #upgrade } },
        syn::Fields::Unnamed(_) => quote! { #original_ident ( #upgrade ) },
        syn::Fields::Unit => quote! { #original_ident },
    };

    let where_clause = if skipped_types.is_empty() {
        quote! { #where_clause }
    } else {
        quote! {
            where #(#skipped_types: ::core::default::Default),*
        }
    };

    let mut error = quote! {
        MissingField(&'static str),
    };
    let mut error_display = quote! {
        Self::MissingField(field) => write!(f, "Missing required field for upgrade: {}", field),
    };

    if renamed {
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


    output.extend(quote! {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum #error_ident {
            #error
        }

        impl ::std::error::Error for #error_ident {}

        impl ::core::fmt::Display for #error_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #error_display
                }
            }
        }

        impl #impl_generics optionize::Upgradable for #optionized_ident #type_generics #where_clause {
            type Error = #error_ident;
            fn upgrade(self) -> ::core::result::Result<Self::Target, Self::Error> {
                ::core::result::Result::Ok(#original)
            }
        }
    });

    output.into()
}
