use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{
    parenthesized, parse_macro_input, parse_quote, Error, GenericArgument, Ident, ItemStruct, Meta, PathArguments,
    Token, Type,
};

mod kw {
    use syn::custom_keyword;

    custom_keyword!(name);
    custom_keyword!(attributes);
}

enum Arg {
    Ident(Ident),
    Attributes(Vec<Meta>),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();

        if lookahead.peek(kw::name) {
            input.parse::<kw::name>()?;
            input.parse::<Token![=]>()?;
            let name = input.parse::<syn::Ident>()?;

            return Ok(Arg::Ident(name));
        }

        if lookahead.peek(kw::attributes) {
            input.parse::<kw::attributes>()?;

            let metas = {
                let metas;
                parenthesized!(metas in input);
                metas
                    .parse_terminated(Meta::parse, Token![,])?
                    .into_iter()
                    .collect()
            };

            return Ok(Arg::Attributes(metas));
        }

        if lookahead.peek(Ident) {
            let ident = input.parse::<Ident>()?;
            return Ok(Arg::Ident(ident));
        }

        Err(lookahead.error())
    }
}

struct MacroArgs {
    ident: Ident,
    attributes: Option<Vec<Meta>>,
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let args = input.parse_terminated(Arg::parse, Token![,])?;
        let mut ident = None;
        let mut attributes = None::<Vec<_>>;

        for arg in args {
            match arg {
                Arg::Ident(id) => {
                    if ident.is_some() {
                        return Err(Error::new(id.span(), "only one name can be specified"));
                    }
                    ident = Some(id);
                }
                Arg::Attributes(mut metas) => attributes.get_or_insert_default().append(&mut metas),
            }
        }

        let Some(ident) = ident else {
            return Err(Error::new(input.span(), "a name must be specified"));
        };

        Ok(Self { ident, attributes })
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
    let args = parse_macro_input!(args as MacroArgs);
    let original = parse_macro_input!(input as ItemStruct);

    let mut optionized = original.clone();
    optionized.ident = args.ident;

    if let Some(attributes) = args.attributes {
        optionized.attrs = attributes
            .into_iter()
            .map(|meta| parse_quote! { #[#meta] })
            .collect();
    }
    optionized
        .attrs
        .retain(|attr| !attr.path().is_ident("optionize"));

    for field in &mut optionized.fields {
        let ty = &field.ty;
        if !is_option(ty) {
            field.ty = parse_quote! { Option<#ty> };
        }
    }

    let expanded = quote! {
        #original
        #optionized
    };

    expanded.into()
}
