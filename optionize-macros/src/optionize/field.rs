use darling::{Error, FromAttributes, Result};
use proc_macro2::{Ident, Span};
use quote::format_ident;
use std::mem::take;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Expr, Field, Index, Member, Type, Visibility, parse_quote};

use super::args::{Crate, FieldArgs, TypeArg};
use super::utils::{format, is_optionize, member_to_string, span};

#[derive(Debug)]
pub(super) enum FieldStrategy {
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

pub(super) struct FieldIr {
    pub(super) krate: Crate,
    pub(super) ty: Type,
    pub(super) visibility: Visibility,
    pub(super) index: usize,
    pub(super) span: Span,
    pub(super) original: Member,
    pub(super) optionized: Member,
    pub(super) strategy: FieldStrategy,
    pub(super) local: Ident,
}

impl Default for FieldIr {
    fn default() -> Self {
        Self {
            krate: Default::default(),
            ty: parse_quote!(()),
            visibility: Visibility::Inherited,
            index: 0,
            span: Span::call_site(),
            original: format_ident!("_").into(),
            optionized: format_ident!("_").into(),
            strategy: Default::default(),
            local: format_ident!("_"),
        }
    }
}

impl FieldIr {
    pub(super) fn extract(
        fields: &mut Punctuated<Field, Comma>,
        krate: Crate,
        partial: bool,
        reverse: bool,
    ) -> Result<Vec<Self>> {
        let mut result = Vec::new();
        let mut errors = Error::accumulator();

        for (index, mut field) in take(fields).into_iter().enumerate() {
            let Some(args) = errors.handle(FieldArgs::from_attributes(&field.attrs)) else {
                continue;
            };
            let span = {
                let span = field.ty.span();
                let span = field.ident.as_ref().map_or(span, |ident| {
                    span.join(ident.span()).unwrap_or(ident.span())
                });
                field
                    .attrs
                    .iter()
                    .filter(|attr| is_optionize(attr))
                    .map(|attr| attr.bracket_token.span.span())
                    .reduce(|span, other| span.join(other).unwrap_or(span))
                    .unwrap_or(span)
            };
            span!(span);

            let (original, optionized) = {
                let member = |index| {
                    field.ident.clone().map(Into::into).unwrap_or_else(|| {
                        Index {
                            index: index as u32,
                            span,
                        }
                        .into()
                    })
                };
                let mut original = member(index);
                let mut optionized = member(fields.len());
                if let Some(name) = args.name.as_ref() {
                    let Some(ident) = field.ident.as_ref() else {
                        errors.push(
                            Error::custom("`name` attribute cannot be used on unnamed fields")
                                .with_span(name),
                        );
                        continue;
                    };
                    let Some(ident) = errors.handle(format::<Ident>(name, ident)) else {
                        continue;
                    };
                    if reverse {
                        original = ident.into();
                    } else {
                        optionized = ident.clone().into();
                        field.ident = Some(ident);
                    }
                }
                (original, optionized)
            };
            let mut ir = Self {
                krate: krate.clone(),
                ty: field.ty.clone(),
                visibility: field.vis.clone(),
                index,
                span,
                local: format_ident!(
                    "v_{}",
                    member_to_string(&original),
                    span = Span::mixed_site()
                ),
                original,
                optionized,
                ..Default::default()
            };

            if let Some(skip) = args.skip {
                if !partial {
                    errors.push(
                        Error::custom(
                            "`skip` attribute is only allowed when `partial` is specified",
                        )
                        .with_span(&skip.span()),
                    );
                    continue;
                }
                let ty = &ir.ty;
                let upgrade = skip
                    .into_inner()
                    .explicit()
                    .and_then(|skip| skip.upgrade)
                    .unwrap_or_else(|| pq! { <#ty as ::core::default::Default>::default() });
                ir.strategy = FieldStrategy::Skip { upgrade };
                result.push(ir);
                continue;
            }

            let wrap = !args.flatten.is_present();
            let Some(nest) = errors.handle(args.nest.map(TypeArg::parse).transpose()) else {
                continue;
            };
            let nest = nest.map(Type::Path);
            let nest = if reverse {
                let ty = &field.ty;
                let ty: Type = if wrap {
                    pq! { <#ty as #krate::__private::OptionField>::Value }
                } else {
                    ty.clone()
                };
                if let Some(nest) = nest {
                    ir.ty = nest;
                    Some(ty)
                } else {
                    ir.ty = ty;
                    None
                }
            } else {
                let ty = nest.as_ref().unwrap_or(&field.ty);
                field.ty = if wrap {
                    pq! { ::core::option::Option<#ty> }
                } else {
                    ty.clone()
                };
                nest
            };
            ir.strategy = FieldStrategy::Optionize { wrap, nest };
            args.general.attrs.patch(&mut field.attrs);
            result.push(ir);
            fields.push(field);
        }

        errors.finish_with(result)
    }
}
