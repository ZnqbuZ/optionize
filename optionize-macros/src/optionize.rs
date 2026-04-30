use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use std::mem::take;
use syn::parse::{Parse, ParseStream, Parser, Result};
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, parse_quote, Attribute, Error, Expr, GenericArgument, ItemStruct, Lit, Meta, PathArguments,
    Token, Type,
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
    let Type::Path(path) = ty else {
        return false;
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
    let args = parse_macro_input!(args as StructArgs);
    let original = parse_macro_input!(input as ItemStruct);

    let mut optionized = original.clone();

    let ident = optionized.ident;
    let name = args.name.replace("{}", &ident.to_string());
    optionized.ident = Ident::new(&name, ident.span());

    if let Some(attributes) = args.attributes {
        optionized.attrs = attributes
            .into_iter()
            .map(|meta| parse_quote! { #[#meta] })
            .collect();
    }
    optionized
        .attrs
        .retain(|attr| !attr.path().is_ident("optionize"));

    let fields = match &mut optionized.fields {
        syn::Fields::Named(fields) => &mut fields.named,
        syn::Fields::Unnamed(fields) => &mut fields.unnamed,
        syn::Fields::Unit => return Default::default(),
    };
    for mut field in take(fields) {
        let args = match FieldArgs::extract(&mut field.attrs) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error().into(),
        };

        if args.skip {
            continue;
        }

        if let Some(name) = args.name {
            let Some(span) = field.ident.map(|id| id.span()) else {
                return Error::new_spanned(field.ty, "cannot rename unnamed field")
                    .to_compile_error()
                    .into();
            };
            field.ident = Some(Ident::new(&name, span));
        }

        if args.wrapped.unwrap_or_else(|| !is_option(&field.ty)) {
            let ty = &field.ty;
            field.ty = parse_quote! { Option<#ty> };
        }

        fields.push(field);
    }

    quote! {
        #original
        #optionized
    }
    .into()
}
