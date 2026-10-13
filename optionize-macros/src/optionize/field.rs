use darling::util::Override;
use darling::{Error, FromAttributes, Result};
use proc_macro2::{Ident, Span};
use quote::format_ident;
use std::mem::take;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Expr, Field, Index, Member, Type, TypePath, Visibility};

use super::args::{Crate, FieldArgs, TypeArg};
use super::utils::{format, is_optionize, member_to_string, span};

#[derive(Debug)]
pub(super) enum FieldStrategy {
    Skip,
    Optionize {
        wrap: bool,
        nest: Option<Box<Type>>,
        convert: Option<Box<Type>>,
    },
}

pub(super) struct FieldIr {
    pub(super) krate: Crate,
    pub(super) subject_ty: Type,
    pub(super) object_ty: Type,
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
        optional: Option<&TypePath>,
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

            // StructArgs checks the struct's `optional`; this checks field overrides.
            if reverse && let Some(optional) = &args.optional {
                errors.push(
                    Error::custom("`optional` cannot be used with `subject`; declare the field wrapper directly")
                        .with_span(optional),
                );
                continue;
            }

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
            let (subject_ty, strategy) = if args.skip.is_present() {
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
                let Some(convert) = errors.handle(args.r#as.map(TypeArg::parse).transpose()) else {
                    continue;
                };
                let convert = convert.map(Type::Path);
                // Resolve the subject's type in both directions. `nest` and `convert`
                // carry the object's inner type when it differs from the subject's.
                let (subject_ty, nest, convert) = if reverse {
                    let ty = if wrap {
                        pq! { <#ty as #krate::Optional>::Value }
                    } else {
                        ty
                    };
                    match (nest, convert) {
                        (Some(nest), _) => (nest, Some(ty), None),
                        (None, Some(convert)) => (convert, None, Some(ty)),
                        (None, None) => (ty, None, None),
                    }
                } else {
                    field.ty = {
                        let ty = convert.as_ref().or(nest.as_ref()).unwrap_or(&ty);
                        if wrap {
                            let optional = match args.optional {
                                Some(optional) => {
                                    let Some(optional) = errors.handle(optional.parse()) else {
                                        continue;
                                    };
                                    optional
                                }
                                None => optional
                                    .cloned()
                                    .unwrap_or_else(|| pq! { ::core::option::Option }),
                            };
                            let Some(ty) = errors.handle(super::args::wrap(optional, ty)) else {
                                continue;
                            };
                            ty
                        } else {
                            ty.clone()
                        }
                    };
                    (ty, nest, convert)
                };
                args.general.attrs.patch(&mut field.attrs);
                (
                    subject_ty,
                    FieldStrategy::Optionize {
                        wrap,
                        nest: nest.map(Box::new),
                        convert: convert.map(Box::new),
                    },
                )
            };
            let default = args
                .default
                .map(|default| default.into_inner())
                .or_else(|| matches!(strategy, FieldStrategy::Skip).then_some(Default::default()));
            let ir = Self {
                krate: krate.clone(),
                subject_ty,
                object_ty: field.ty.clone(),
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

    pub(super) fn ty(&self) -> &Type {
        match &self.strategy {
            FieldStrategy::Optionize { nest, convert, .. } => convert
                .as_deref()
                .or(nest.as_deref())
                .unwrap_or(&self.subject_ty),
            FieldStrategy::Skip => &self.subject_ty,
        }
    }
}
