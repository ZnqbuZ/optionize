use bitflags::bitflags;
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use std::iter::zip;
use std::mem::take;
use syn::meta::{parser, ParseNestedMeta};
use syn::parse::{Parse, Result};
use syn::punctuated::Punctuated;
use syn::token::{Comma, Paren};
use syn::{
    parse_macro_input, parse_quote, parse_str, Attribute, Error, Expr, Field, GenericArgument, Index, ItemStruct,
    LitBool, LitStr, Meta, PathArguments, Token, Type,
};

macro_rules! match_meta {
    (
        $meta:ident;
        $( $name:ident => $block:block )*
    ) => {
        $(
            if $meta.path.is_ident(stringify!($name)) {
                $block
                return Ok(());
            }
        )*
    };
}

macro_rules! match_meta_value {
    (
        $meta:ident, $self:ident;
        $( $name:ident : $ty:path ),* $(,)?
    ) => {
        match_meta! {
            $meta;
            $(
                $name => {
                    $self.$name = Some($meta.value()?.parse::<$ty>()?.value());
                }
            )*
        }
    };
}

#[derive(Default)]
struct StructArgs {
    name: Option<String>,
    attributes: Option<Vec<Meta>>,
    partial: bool,
    upgradable: bool,
}

impl StructArgs {
    fn parse(&mut self, meta: ParseNestedMeta) -> Result<()> {
        match_meta_value! {
            meta, self;
            name: LitStr,
        }
        match_meta! {
            meta;
            attributes => {
                let content;
                syn::parenthesized!(content in meta.input);
                self.attributes.get_or_insert_default().extend(content.parse_terminated(Meta::parse, Token![,])?);
            }
            partial => {
                self.partial = true;
                if meta.input.peek(Paren) {
                meta.parse_nested_meta(|nested| {
                    match_meta! {
                        nested;
                        upgradable => {
                            self.upgradable = true;
                        }
                    }
                    Err(nested.error("unrecognized argument"))
                })?;
            }
            }
        }
        Err(meta.error("unrecognized argument"))
    }
}

#[derive(Default)]
struct FieldArgs {
    name: Option<String>,
    attributes: Option<Vec<Meta>>,
    wrap: Option<bool>,
    nest: Option<String>,
    skip: bool,
    upgrade: Option<proc_macro2::TokenStream>,
}

impl FieldArgs {
    fn parse(&mut self, meta: ParseNestedMeta) -> Result<()> {
        match_meta_value! {
            meta, self;
            name: LitStr,
            wrap: LitBool,
            nest: LitStr,
        }
        match_meta! {
            meta;
            attributes => {
                let content;
                syn::parenthesized!(content in meta.input);
                self.attributes
                    .get_or_insert_default()
                    .extend(content.parse_terminated(Meta::parse, Token![,])?);
            }
            skip => {
                self.skip = true;
                if meta.input.peek(Paren) {
                    meta.parse_nested_meta(|nested| {
                        match_meta! {
                            nested;
                            upgrade => {
                                self.upgrade = Some(nested.value()?.parse::<Expr>()?.to_token_stream());
                            }
                        }
                        Err(nested.error("unrecognized argument"))
                    })?;
                }
            }
        }
        Err(meta.error("unrecognized argument"))
    }

    fn extract(attributes: &mut Vec<Attribute>) -> Result<Self> {
        let mut this = Self::default();
        let mut errors = None::<Error>;

        attributes.retain(|attribute| {
            if !attribute.path().is_ident("optionize") {
                return true;
            }

            if let Err(e) = attribute.parse_nested_meta(|meta| this.parse(meta)) {
                if let Some(errors) = errors.as_mut() {
                    errors.combine(e);
                } else {
                    errors = Some(e);
                }
            }

            false
        });

        if let Some(err) = errors {
            return Err(err);
        }

        Ok(this)
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
    if segment.ident != "Option" {
        return false;
    }

    matches!(
        &segment.arguments,
        PathArguments::AngleBracketed(args)
            if args.args.len() == 1
                && matches!(args.args.first(), Some(GenericArgument::Type(_)))
    )
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UpgradeErrorBuilder: u8 {
        const MISSING         = 1 << 0; // 0b0001
        const NESTED          = 1 << 1; // 0b0010
        const MISSING_RENAMED = 1 << 2; // 0b0100
        const RENAMED_NESTED  = 1 << 3; // 0b1000
    }
}

impl UpgradeErrorBuilder {
    fn update(&mut self, renamed: bool, nest: bool) {
        let offset = ((renamed as u8) << 1) | (nest as u8);
        self.insert(Self::from_bits_retain(1 << offset));
    }

    fn build(&self, vis: &syn::Visibility, ident: &Ident) -> proc_macro2::TokenStream {
        let mut error = quote! {};
        let mut display = quote! {};
        let mut source = proc_macro2::TokenStream::new();

        if self.is_empty() {
            return quote! {
                #vis type #ident = ::core::convert::Infallible;
            };
        }

        if self.contains(Self::MISSING) {
            error.extend(quote! {
                MissingField(&'static str),
            });
            display.extend(quote! {
                Self::MissingField(field) => write!(f, "Missing required field for upgrade: {}", field),
            });
        }

        if self.contains(Self::MISSING_RENAMED) {
            error.extend(quote! {
                MissingRenamedField {
                    original: &'static str,
                    optionized: &'static str,
                },
            });
            display.extend(quote! {
            Self::MissingRenamedField { original, optionized } => write!(
                f,
                "Missing required field for upgrade: optionized field `{}` -> original field `{}`",
                original, optionized
            ),
        });
        }

        if self.contains(Self::NESTED) {
            error.extend(quote! {
                NestedError {
                    field: &'static str,
                    source: ::optionize::__private::alloc::boxed::Box<dyn ::core::error::Error + 'static>,
                },
            });
            display.extend(quote! {
                Self::NestedError { field, .. } => write!(
                    f,
                    "Failed to upgrade nest field `{}`",
                    field
                ),
            });
            source.extend(quote! {
                Self::NestedError { source, .. } => ::core::option::Option::Some(&**source),
            });
        }

        if self.contains(Self::RENAMED_NESTED) {
            error.extend(quote! {
                RenamedNestedError {
                    original: &'static str,
                    optionized: &'static str,
                    source: ::optionize::__private::alloc::boxed::Box<dyn ::core::error::Error + 'static>,
                },
            });
            display.extend(quote! {
                Self::RenamedNestedError { original, optionized, .. } => write!(
                    f,
                    "Failed to upgrade nest field: optionized field `{}` -> original field `{}`",
                    optionized, original
                ),
            });
            source.extend(quote! {
                Self::RenamedNestedError { source, .. } => ::core::option::Option::Some(&**source),
            });
        }

        source.extend(quote! {
            _ => ::core::option::Option::None,
        });

        quote! {
            #[derive(Debug)]
            #vis enum #ident {
                #error
            }

            impl ::core::error::Error for #ident {
                fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                    match self {
                        #source
                    }
                }
            }

            impl ::core::fmt::Display for #ident {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    match self {
                        #display
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
    upgrade: proc_macro2::TokenStream,
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
            upgrade: quote! { ::core::default::Default::default() },
            local: Ident::new("_", Span::call_site()),
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
                    let index = Index::from(i);
                    quote! { #index }
                }
            };

            if args.skip {
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

                if let Some(upgrade) = args.upgrade {
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
                let nest = nest.replace("{}", &quote!(#ty).to_string());
                let nest_ty = parse_str::<Type>(&nest)
                    .map_err(|e| Error::new_spanned(ty, format!("invalid nest type: {}", e)))?;
                this.nest.push((ty.clone(), nest_ty.clone()));
                (nest_ty, true)
            } else {
                (ty.clone(), false)
            };

            let wrap = args.wrap.unwrap_or_else(|| !is_option(&ty));
            field.ty = if wrap {
                parse_quote! { Option<#ty> }
            } else {
                ty
            };

            let local = field
                .ident
                .clone()
                .unwrap_or_else(|| Ident::new(&format!("_{}", i), Span::call_site()));

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

pub fn proc(args: TokenStream, input: TokenStream) -> TokenStream {
    let struct_args = {
        let mut struct_args = StructArgs::default();
        let parser = parser(|meta| struct_args.parse(meta));
        parse_macro_input!(args with parser);
        struct_args
    };

    let mut original = parse_macro_input!(input as ItemStruct);

    let field_args = {
        let mut field_args = Vec::with_capacity(original.fields.len());
        let mut errors: Option<Error> = None;

        for field in original.fields.iter_mut() {
            match FieldArgs::extract(&mut field.attrs) {
                Ok(a) => field_args.push(a),
                Err(e) => {
                    if let Some(ref mut errors) = errors {
                        errors.combine(e);
                    } else {
                        errors = Some(e);
                    }
                }
            }
        }

        if let Some(e) = errors {
            return e.to_compile_error().into();
        }

        field_args
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

    let meta = match StructMeta::extract(fields, field_args, struct_args.partial) {
        Ok(meta) => meta,
        Err(e) => return e.to_compile_error().into(),
    };
    let fields = meta.fields.iter().filter(|f| !f.skip).collect::<Vec<_>>();

    let mut optionize = quote! {};
    let mut patch = quote! {};
    let mut merge = quote! {};

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
                patch.extend(quote! {
                    if let Some(value) = self.#optionized_field {
                        ::optionize::PartialOptionized::patch(value, &mut subject.#original_field);
                    }
                });
                merge.extend(quote! {
                    match (&mut self.#optionized_field, other.#optionized_field) {
                        (Some(this), Some(other)) => ::optionize::PartialOptionized::merge(this, other),
                        (None, Some(other)) => self.#optionized_field = Some(other),
                        _ => {}
                    }
                });
                quote! { ::core::option::Option::Some(::optionize::PartialOptionized::optionize(subject.#original_field)) }
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
                quote! { ::core::option::Option::Some(subject.#original_field) }
            }
            (false, true) => {
                patch.extend(quote! { ::optionize::PartialOptionized::patch(self.#optionized_field, &mut subject.#original_field); });
                merge.extend(quote! { ::optionize::PartialOptionized::merge(&mut self.#optionized_field, other.#optionized_field); });
                quote! { ::optionize::PartialOptionized::optionize(subject.#original_field) }
            }
            (false, false) => {
                patch.extend(quote! { subject.#original_field = self.#optionized_field; });
                merge.extend(quote! { self.#optionized_field = other.#optionized_field; });
                quote! { subject.#original_field }
            }
        };

        optionize.extend(if named {
            quote! { #optionized_field: #optionize_value, }
        } else {
            quote! { #optionize_value, }
        });
    }

    let mut output = quote! {
        #original
        #optionized
    };

    let generics = optionized.generics.clone();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let original_ident = &original.ident;
    let optionized_ident = &optionized.ident;

    let subject = quote! { #original_ident #type_generics };

    let optionize = match &original.fields {
        syn::Fields::Named(_) => quote! { Self { #optionize } },
        syn::Fields::Unnamed(_) => quote! { Self ( #optionize ) },
        syn::Fields::Unit => quote! { Self },
    };

    output.extend(quote! {
        impl #impl_generics ::optionize::PartialOptionized for #optionized_ident #type_generics #where_clause {
            type Subject = #subject;
            fn optionize(subject: Self::Subject) -> Self { #optionize }
            fn patch(self, subject: &mut Self::Subject) { #patch }
            fn merge(&mut self, other: Self) { #merge }
        }
    });

    if !struct_args.partial || struct_args.upgradable {
        let destructure = if fields.is_empty() {
            quote! { let _ = self; }
        } else {
            let locals = fields.iter().map(|m| &m.local);
            if named {
                quote! { let Self { #(#locals),* } = self; }
            } else {
                quote! { let Self ( #(#locals),* ) = self; }
            }
        };

        let error_ident = Ident::new(
            &format!("{}UpgradeError", optionized_ident),
            optionized_ident.span(),
        );
        let mut error_builder = UpgradeErrorBuilder::empty();

        let mut past = Vec::new();

        let mut upgrade = quote! {};

        for (i, field) in fields.iter().enumerate() {
            let FieldMeta {
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
                    quote! { #error_ident::NestedError { field: #original_name, source: ::optionize::__private::alloc::boxed::Box::new(_e) as _ } },
                )
            } else {
                (
                    true,
                    quote! { #error_ident::MissingRenamedField { original: #original_name, optionized: #optionized_name } },
                    quote! { #error_ident::RenamedNestedError { original: #original_name, optionized: #optionized_name, source: ::optionize::__private::alloc::boxed::Box::new(_e) as _ } },
                )
            };

            error_builder.update(renamed, *nest);

            let build = |current: proc_macro2::TokenStream| {
                let past = &past;
                let future = fields[(i + 1)..].iter().map(|m| &m.local);
                if named {
                    quote! { Self { #(#past)* #local: #current #(, #future)* } }
                } else {
                    quote! { Self ( #(#past)* #current #(, #future)* ) }
                }
            };

            if *wrap {
                let this = build(quote! { ::core::option::Option::None });
                upgrade.extend(quote! {
                    let #local = match #local {
                        ::core::option::Option::Some(v) => v,
                        ::core::option::Option::None => return ::core::result::Result::Err((#missing_err, #this)),
                    };
                });
            }

            if *nest {
                let this = build(if *wrap {
                    quote! { ::core::option::Option::Some(#local) }
                } else {
                    quote! { #local }
                });
                upgrade.extend(quote! {
                    let #local = match ::optionize::Upgradable::upgrade(#local) {
                        ::core::result::Result::Ok(v) => v,
                        ::core::result::Result::Err((_e, #local)) => return ::core::result::Result::Err((#nest_err, #this)),
                    };
                });
            }

            let past_value = if *wrap {
                quote! { ::core::option::Option::Some(#local) }
            } else {
                quote! { #local }
            };
            past.push(if named {
                quote! { #local: #past_value, }
            } else {
                quote! { #past_value, }
            });
        }

        output.extend(error_builder.build(&original.vis, &error_ident));

        let original = {
            let mut fields = quote! {};
            for field in &meta.fields {
                let original_field = &field.original_field;
                if field.skip {
                    let upgrade = &field.upgrade;
                    if named {
                        fields.extend(quote! { #original_field: #upgrade, });
                    } else {
                        fields.extend(quote! { #upgrade, });
                    }
                    continue;
                }
                let original_value = &field.local;
                if named {
                    fields.extend(quote! { #original_field: #original_value, });
                } else {
                    fields.extend(quote! { #original_value, });
                }
            }

            match &original.fields {
                syn::Fields::Named(_) => quote! { #original_ident { #fields } },
                syn::Fields::Unnamed(_) => quote! { #original_ident ( #fields ) },
                syn::Fields::Unit => quote! { #original_ident },
            }
        };

        output.extend(quote! {
            impl #impl_generics ::optionize::Upgradable<#subject> for #optionized_ident #type_generics #where_clause {
                type Error = #error_ident;
                fn upgrade(self) -> ::core::result::Result<#subject, (Self::Error, Self)> {
                    #destructure
                    #upgrade
                    ::core::result::Result::Ok(#original)
                }
            }
        });
    }

    output.into()
}
