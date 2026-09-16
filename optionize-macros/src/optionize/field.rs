use darling::util::Override;
use darling::{Error, FromAttributes, Result};
use proc_macro2::{Ident, Span};
use quote::format_ident;
use std::mem::take;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Expr, Field, Index, Member, Type, Visibility};

use super::args::{Crate, FieldArgs, TypeArg};
use super::utils::{format, is_optionize, member_to_string, span};

#[derive(Debug)]
pub(super) enum FieldStrategy {
    Skip,
    Optionize { wrap: bool, nest: Option<Box<Type>> },
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
    pub(super) default: Option<Override<Expr>>,
    pub(super) local: Ident,
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
            let ty = field.ty.clone();
            let (ty, strategy) = if args.skip.is_present() {
                if !partial {
                    errors.push(
                        Error::custom(
                            "`skip` attribute is only allowed when `partial` is specified",
                        )
                        .with_span(&args.skip.span()),
                    );
                    continue;
                }
                (ty, FieldStrategy::Skip)
            } else {
                let wrap = !args.flatten.is_present();
                let Some(nest) = errors.handle(args.nest.map(TypeArg::parse).transpose()) else {
                    continue;
                };
                let nest = nest.map(Type::Path);
                let (ty, nest) = if reverse {
                    let ty = if wrap {
                        pq! { <#ty as #krate::__private::OptionField>::Value }
                    } else {
                        ty
                    };
                    match nest {
                        Some(nest) => (nest, Some(ty)),
                        None => (ty, None),
                    }
                } else {
                    field.ty = {
                        let ty = nest.as_ref().unwrap_or(&ty);
                        if wrap {
                            pq! { Option<#ty> }
                        } else {
                            ty.clone()
                        }
                    };
                    (ty, nest)
                };
                args.general.attrs.patch(&mut field.attrs);
                (
                    ty,
                    FieldStrategy::Optionize {
                        wrap,
                        nest: nest.map(Box::new),
                    },
                )
            };
            let default = args
                .default
                .map(|default| default.into_inner())
                .or_else(|| matches!(strategy, FieldStrategy::Skip).then_some(Default::default()));
            let ir = Self {
                krate: krate.clone(),
                ty,
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
                strategy,
                default,
            };
            if matches!(ir.strategy, FieldStrategy::Optionize { .. }) {
                fields.push(field);
            }
            result.push(ir);
        }

        errors.finish_with(result)
    }
}
