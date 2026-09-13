mod args;
mod codegen;
mod field;
mod utils;

use darling::ast::NestedMeta;
use darling::util::Override;
use darling::{Error, FromAttributes, FromMeta, Result};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, quote_spanned as qs};
use std::collections::HashSet;
use std::mem::take;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::token::{Brace, Paren};
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, FieldsUnnamed, Index, Lifetime, PathArguments,
    WherePredicate, parse_quote, parse_quote_spanned as pqs, parse2,
};

use args::{Crate, OptionizedArgs, StructArgs};
use codegen::{
    Merge, Optionize, Patch, Retain, Upgrade, UpgradeFieldValue, UpgradeSkip, Validate, View,
};
use field::{FieldIr, FieldStrategy};
use utils::{collect_idents, format, new_ident, span};

#[derive(Debug, Clone, Copy)]
enum StructStyle {
    Named,
    Unnamed,
    Unit,
}

fn parse(krate: Crate, input: TokenStream) -> Result<TokenStream> {
    let subject = parse2::<DeriveInput>(input)?;
    let _span = subject.span();
    span!(_span);

    macro_rules! construct {
        ($style:expr, $span:expr => [$($ty:tt)+] $($fields:tt)*) => {
            match $style {
                StructStyle::Unit => qs! { $span => $($ty)* },
                _ => qs! { $span => #[allow(clippy::init_numbered_fields)] $($ty)* { $($fields)* } },
            }
        };
    }

    let this = format_ident!("self", span = Span::mixed_site());
    let args = StructArgs::from_attributes(&subject.attrs)?;
    let source = q! { #[derive(#krate::__private::Optionize)] #subject };
    let reverse = args.subject.is_some();

    let (partial, upgradable, marked) = args
        .partial
        .map(|partial| {
            let span = partial.span();
            let (upgradable, marked) = match partial.into_inner() {
                Override::Explicit(p) => (
                    p.upgradable.is_present().then(|| p.upgradable.span()),
                    p.marked,
                ),
                _ => Default::default(),
            };
            (Some(span), upgradable, marked)
        })
        .unwrap_or_default();

    let mut object = subject;
    let (impl_generics, type_generics, where_clause) = object.generics.split_for_impl();

    let subject = &object.ident.clone();
    #[allow(non_snake_case)]
    let (Subject, subject_constructor) = if let Some(target) = args.subject {
        let path = target.format(subject)?;
        let ty = q! { #path };
        let mut constructor = path;
        for segment in &mut constructor.path.segments {
            if let PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                arguments.colon2_token.get_or_insert_with(Default::default);
            }
        }
        (ty, q! { #constructor })
    } else {
        (q! { #subject #type_generics }, q! { #subject })
    };

    let has_object = args.object.is_some();
    #[allow(non_snake_case)]
    let Object = if reverse {
        q! { #subject #type_generics }
    } else if let Some(object) = args.object {
        let path = object.format(subject)?;
        q! { #path }
    } else {
        object.ident = match &args.general.name {
            Some(name) => format(name, subject)?,
            None => format(&pqs! { subject.span() => "{}Optional"}, subject)?,
        };
        let name = &object.ident;
        q! { #name #type_generics }
    };

    if !has_object && !reverse {
        args.general.attrs.patch(&mut object.attrs);
    }

    let data = match &mut object.data {
        Data::Struct(data) => data,
        _ => {
            return Err(
                Error::custom("Optionize can only be derived for structs").with_span(&_span)
            );
        }
    };

    let subject_style = match &data.fields {
        Fields::Named(_) => StructStyle::Named,
        Fields::Unnamed(_) => StructStyle::Unnamed,
        Fields::Unit => StructStyle::Unit,
    };
    let object_style = if matches!(subject_style, StructStyle::Unit)
        && let Some(marked) = &marked
    {
        let span = marked.span();
        let punctuated = Default::default();
        if let Override::Explicit(marked) = marked.as_ref()
            && marked.name.is_some()
        {
            data.fields = Fields::Named(FieldsNamed {
                brace_token: Brace(span),
                named: punctuated,
            });
            StructStyle::Named
        } else {
            data.fields = Fields::Unnamed(FieldsUnnamed {
                paren_token: Paren(span),
                unnamed: punctuated,
            });
            StructStyle::Unnamed
        }
    } else {
        subject_style
    };

    let fields = match &mut data.fields {
        Fields::Named(fields) => &mut fields.named,
        Fields::Unnamed(fields) => &mut fields.unnamed,
        Fields::Unit => &mut Default::default(),
    };

    let originals = FieldIr::extract(fields, krate.clone(), partial.is_some(), reverse)?;
    let optionizeds = originals
        .iter()
        .filter(|f| matches!(f.strategy, FieldStrategy::Optionize { .. }))
        .collect::<Vec<_>>();

    let marker = if let Some(marked) = marked {
        let span = marked.span();
        let marked = marked.into_inner().unwrap_or_default();

        let mut attrs = vec![pqs! { span => #[doc(hidden)] }];
        marked.attrs.patch(&mut attrs);

        let ident = match (subject_style, marked.name) {
            (StructStyle::Named, None) => {
                let names = fields
                    .iter()
                    .filter_map(|f| f.ident.as_ref())
                    .map(|i| i.unraw().to_string())
                    .collect::<HashSet<_>>();
                let mut ident = "_marker".to_owned();
                while names.contains(&ident) {
                    ident.insert(0, '_');
                }
                Some(format_ident!("{}", ident, span = span))
            }
            (StructStyle::Unnamed, Some(name)) => {
                return Err(
                    Error::custom("`name` attribute cannot be used on unnamed structs")
                        .with_span(&name),
                );
            }
            (_, Some(name)) => Some(name),
            _ => None,
        };

        let (marker, field) = if let Some(ident) = ident {
            (
                qs! { ident.span() => #ident: ::core::marker::PhantomData, },
                pqs! { span =>
                    #(#attrs)*
                    pub #ident: ::core::marker::PhantomData<fn() -> *const #Subject>
                },
            )
        } else {
            let index = Index {
                index: fields.len() as u32,
                span,
            };
            (
                qs! { span => #index: ::core::marker::PhantomData, },
                pqs! { span =>
                    #(#attrs)*
                    pub ::core::marker::PhantomData<fn() -> *const #Subject>
                },
            )
        };

        fields.push(field);
        Some(marker)
    } else {
        None
    };

    let mut output = Vec::new();
    if reverse {
        output.push(q! { #[derive(#krate::__private::Optionize)] #object });
    } else {
        output.push(source);
    }
    if !has_object && !reverse {
        output.push(q! { #object });
    }
    let declarations = take(&mut output);

    let where_clause = {
        let mut where_clause = where_clause.cloned().unwrap_or_else(|| pq! { where });
        let mut predicates = where_clause
            .predicates
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        where_clause.predicates.extend(
            optionizeds
                .iter()
                .filter_map(|field| -> Option<[WherePredicate; 2]> {
                    let FieldIr {
                        ty, strategy, span, ..
                    } = field;
                    let FieldStrategy::Optionize {
                        nest: Some(nest), ..
                    } = strategy
                    else {
                        return None;
                    };
                    Some([
                        pqs! { *span => #nest: #krate::Schema<#ty> },
                        pqs! { *span => #nest: #krate::PartialOptionized<#ty> },
                    ])
                })
                .flatten()
                .filter(|predicate| predicates.insert(predicate.clone())),
        );
        where_clause
    };

    {
        let (view, view_lifetime, self_lifetime) = {
            let mut idents = HashSet::new();
            let fields = originals.iter().map(|field| {
                let ty = &field.ty;
                let nest = match &field.strategy {
                    FieldStrategy::Optionize { nest, .. } => nest.as_ref(),
                    FieldStrategy::Skip { .. } => None,
                };
                q! { #ty #nest }
            });
            collect_idents(
                q! { #Subject #Object #impl_generics #where_clause #(#fields)* },
                &mut idents,
            );
            let new_lifetime = |lifetime| {
                let ident = new_ident(lifetime, &idents);
                Lifetime::new(&format!("'{ident}"), ident.span())
            };
            let view = format::<Ident>(&pq! { "__{}OptionizeView" }, subject)?;
            (
                new_ident(&view.to_string(), &idents),
                new_lifetime("v"),
                new_lifetime("s"),
            )
        };
        let generics = {
            let mut generics = object.generics.clone();
            generics.params.insert(0, parse_quote! { #view_lifetime });
            let mut where_clause = where_clause.clone();
            where_clause
                .predicates
                .push(pq! { #Subject: #view_lifetime });
            where_clause
                .predicates
                .push(pq! { #Object: #view_lifetime });
            generics.where_clause = Some(where_clause);
            generics
        };
        let (_, type_generics, _) = generics.split_for_impl();

        {
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            let fields = originals.iter().map(|field| {
                let FieldIr {
                    ty,
                    visibility,
                    strategy,
                    local,
                    span,
                    ..
                } = field;
                let ty = if let FieldStrategy::Optionize {
                    nest: Some(nest), ..
                } = strategy
                {
                    qs! { *span => <#nest as #krate::Schema<#ty>>::View<#view_lifetime> }
                } else {
                    qs! { *span => &#view_lifetime #ty }
                };
                qs! { *span => #visibility #local: ::core::option::Option<#ty>, }
            });
            output.push(q! {
                // A nominal view keeps private field types out of public associated types.
                // The anonymous const keeps the helper out of the surrounding namespace.
                #[doc(hidden)]
                #[allow(private_bounds)]
                pub struct #view #impl_generics #where_clause {
                    #(#fields)*
                    #[allow(clippy::type_complexity)]
                    __marker: ::core::marker::PhantomData<fn() -> &#view_lifetime (#Subject, #Object)>,
                }
            });

            let fields = originals.iter().map(|field| {
                let local = &field.local;
                q! { #local: ::core::option::Option::None, }
            });
            output.push(q! {
                impl #impl_generics ::core::default::Default for #view #type_generics #where_clause {
                    fn default() -> Self {
                        Self { #(#fields)* __marker: ::core::marker::PhantomData }
                    }
                }
            });
        }

        {
            let fields = originals.iter().map(|field| View {
                field,
                subject: false,
            });
            output.push(q! {
                #[automatically_derived]
                impl #impl_generics #krate::Schema<#Subject> for #Object #where_clause {
                    type View<#view_lifetime> = #view #type_generics
                    where #Subject: #view_lifetime, Self: #view_lifetime;
                    #[inline]
                    fn view<#self_lifetime>(&#self_lifetime #this) -> <Self as #krate::Schema<#Subject>>::View<#self_lifetime>
                    where #Subject: #self_lifetime, Self: #self_lifetime {
                        #view { #(#fields)* __marker: ::core::marker::PhantomData }
                    }
                }
            });
        }

        {
            let mut where_clause = where_clause.clone();
            let mut predicates = where_clause
                .predicates
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            where_clause.predicates.extend(
                originals
                    .iter()
                    .filter_map(|field| -> Option<WherePredicate> {
                        let FieldIr { ty, strategy, index, span, .. } = field;
                        match strategy {
                            FieldStrategy::Skip { .. } => None,
                            FieldStrategy::Optionize { nest: None, .. } => {
                                Some(pqs! { *span => for<#view_lifetime> &#view_lifetime #ty: #krate::__private::Equal<#index> })
                            }
                            FieldStrategy::Optionize { nest: Some(nest), .. } => {
                                // The unused binder defers concrete comparison bounds until
                                // Retain is used, keeping other operations available without it.
                                Some(pqs! { *span => for<#view_lifetime> #nest: #krate::Retain<#ty> })
                            }
                        }
                    })
                    .filter(|predicate| predicates.insert(predicate.clone())),
            );
            let baseline = format_ident!("baseline", span = Span::mixed_site());
            let remains = format_ident!("remains", span = Span::mixed_site());
            let fields = originals.iter().map(|field| Retain {
                field,
                baseline: &baseline,
                remains: &remains,
            });
            output.push(q! {
                #[automatically_derived]
                impl #impl_generics #krate::Retain<#Subject, #Object> for #Object #where_clause {
                    #[inline]
                    fn retain_view<#view_lifetime>(&mut #this, #baseline: #view #type_generics) -> bool
                    where #Subject: #view_lifetime, #Object: #view_lifetime {
                        let mut #remains = false;
                        #(#fields)*
                        #remains
                    }
                }
            });
        }

        {
            // Only complete baselines need nested subject views. Defer these bounds so
            // concrete nested objects remain usable without subject implementations.
            let mut where_clause = where_clause.clone();
            let mut predicates = where_clause
                .predicates
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            where_clause.predicates.extend(
                optionizeds
                    .iter()
                    .filter_map(|field| -> Option<WherePredicate> {
                        let FieldIr {
                            ty, strategy, span, ..
                        } = field;
                        let FieldStrategy::Optionize {
                            nest: Some(nest), ..
                        } = strategy
                        else {
                            return None;
                        };
                        Some(pqs! { *span => for<#view_lifetime> #ty: #krate::Schema<#ty, #nest> })
                    })
                    .filter(|predicate| predicates.insert(predicate.clone())),
            );
            let fields = originals.iter().map(|field| View {
                field,
                subject: true,
            });
            output.push(q! {
                #[automatically_derived]
                impl #impl_generics #krate::Schema<#Subject, #Object> for #Subject #where_clause {
                    type View<#view_lifetime> = #view #type_generics
                    where #Subject: #view_lifetime, #Object: #view_lifetime;
                    #[inline]
                    fn view<#self_lifetime>(&#self_lifetime #this) -> <#Object as #krate::Schema<#Subject>>::View<#self_lifetime>
                    where #Subject: #self_lifetime, #Object: #self_lifetime {
                        #view { #(#fields)* __marker: ::core::marker::PhantomData }
                    }
                }
            });
        }
    }

    output.push(q! {
        #[automatically_derived]
        impl #impl_generics #krate::Optionizable<#Object> for #Subject #where_clause {}
    });

    {
        let subject = &format_ident!("subject", span = Span::mixed_site());

        let optionize = {
            let optionizes = optionizeds.iter().map(|field| Optionize { field, subject });

            construct!(object_style, _span => [Self] #(#optionizes)* #marker )
        };
        let patches = optionizeds.iter().map(|field| Patch { field, subject });
        let other = &format_ident!("other", span = Span::mixed_site());
        let merges = optionizeds.iter().map(|field| Merge { field, other });

        output.push(q! {
            #[automatically_derived]
            impl #impl_generics #krate::PartialOptionized<#Subject> for #Object #where_clause {
                #[inline]
                fn optionize(#subject: #Subject) -> Self { #optionize }
                #[inline]
                fn patch(#this, #subject: &mut #Subject) { #(#patches)* }
                #[inline]
                fn merge(&mut #this, #other: Self) { #(#merges)* }
            }
        });
    }

    let span = if partial.is_none() {
        Some(_span)
    } else {
        upgradable
    };

    if let Some(span) = span {
        let mut where_clause = where_clause;
        let mut predicates = where_clause
            .predicates
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        where_clause.predicates.extend(
            optionizeds
                .iter()
                .filter_map(|field| -> Option<[WherePredicate; 2]> {
                    let FieldIr {
                        ty, strategy, span, ..
                    } = field;
                    let FieldStrategy::Optionize {
                        nest: Some(nest), ..
                    } = strategy
                    else {
                        return None;
                    };
                    Some([
                        pqs! { *span => #nest: #krate::Optionized<#ty> },
                        pqs! { *span => <#nest as #krate::Optionized<#ty>>::Errors: 'static },
                    ])
                })
                .flatten()
                .filter(|predicate| predicates.insert(predicate.clone())),
        );

        let failed = &format_ident!("failed", span = Span::mixed_site());
        let errors = &format_ident!("errors", span = Span::mixed_site());

        let validates = optionizeds.iter().map(|field| Validate {
            field,
            subject: &Subject,
            object: &Object,
            failed,
            errors,
        });

        let skips = originals.iter().map(UpgradeSkip);
        let upgrades = optionizeds.iter().copied().map(Upgrade);
        let subject = {
            let fields = originals.iter().map(UpgradeFieldValue);
            construct!(subject_style, span => [#subject_constructor] #(#fields)*)
        };

        output.push(qs! { span =>
            #[automatically_derived]
            impl #impl_generics #krate::Optionized<#Subject> for #Object #where_clause {
                type Errors = #krate::ErrorCollection;
                #[inline]
                fn validate(&#this) -> ::core::result::Result<(), Self::Errors> {
                    let mut #failed = false;
                    let mut #errors = #krate::ErrorCollection::default();
                    #(#validates)*
                    if !#failed {
                        ::core::result::Result::Ok(())
                    } else {
                        ::core::result::Result::Err(#errors)
                    }
                }
                #[inline]
                unsafe fn upgrade_unchecked(#this) -> #Subject {
                    #(#skips)*
                    #(#upgrades)*
                    #subject
                }
            }
        });
    }

    Ok(q! { #(#declarations)* const _: () = { #(#output)* }; })
}

pub fn proc(args: TokenStream, input: &TokenStream) -> Result<TokenStream> {
    let args = OptionizedArgs::from_list(&NestedMeta::parse_meta_list(args)?)?;
    let krate = args.krate.unwrap_or_else(Crate::infer);
    Ok(parse(krate.clone(), input.clone()).unwrap_or_else(|error| {
        let error = error.write_errors();
        quote! {
            #[derive(#krate::__private::Optionize)]
            #input
            #error
        }
    }))
}

#[cfg(test)]
mod tests;
