mod args;
mod attrs;
mod codegen;
mod field;
mod utils;

use darling::ast::NestedMeta;
use darling::util::Override;
use darling::{Error, FromAttributes, FromMeta, Result};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, quote_spanned as qs};
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::token::{Brace, Paren};
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, FieldsUnnamed, Index, Lifetime, PathArguments,
    parse_quote, parse_quote_spanned as pqs, parse2,
};

use args::{Crate, OptionizedArgs, StructArgs};
use codegen::{Merge, Optionize, Patch, Retain, Upgrade, UpgradeDefault, Validate, View};
use field::{FieldIr, FieldStrategy};
use utils::{collect_idents, extend_where_clause, format, new_ident, span};

fn parse(krate: Crate, input: TokenStream) -> Result<TokenStream> {
    let mut object = parse2::<DeriveInput>(input)?;
    let span = object.span();
    span!(span);

    let args = StructArgs::from_attributes(&object.attrs)?;
    let reverse = args.subject.is_some();
    let generated = !reverse && args.object.is_none();
    let mut declarations = if reverse {
        TokenStream::new()
    } else {
        q! { #[derive(#krate::__private::Optionize)] #object }
    };

    let (partial, upgradable, marked) = match args.partial {
        Some(partial) => {
            let partial = partial.into_inner().unwrap_or_default();
            (
                true,
                partial
                    .upgradable
                    .is_present()
                    .then(|| partial.upgradable.span()),
                partial.marked,
            )
        }
        None => (false, Some(span), None),
    };

    let ident = object.ident.clone();
    #[allow(non_snake_case)]
    let (Subject, subject_constructor) = if let Some(subject) = args.subject {
        let mut path = subject.format(&ident)?;
        let ty = q! { #path };
        for segment in &mut path.path.segments {
            if let PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                arguments.colon2_token.get_or_insert_with(Default::default);
            }
        }
        (ty, q! { #path })
    } else {
        let (_, type_generics, _) = object.generics.split_for_impl();
        (q! { #ident #type_generics }, q! { #ident })
    };

    #[allow(non_snake_case)]
    let Object = if reverse {
        let (_, type_generics, _) = object.generics.split_for_impl();
        q! { #ident #type_generics }
    } else if let Some(object) = args.object {
        let path = object.format(&ident)?;
        q! { #path }
    } else {
        args.general.attrs.patch(&mut object.attrs);
        object.ident = match &args.general.name {
            Some(name) => format(name, &ident)?,
            None => format(&pqs! { ident.span() => "{}Optional"}, &ident)?,
        };
        let name = &object.ident;
        let (_, type_generics, _) = object.generics.split_for_impl();
        q! { #name #type_generics }
    };

    let (originals, marker) = {
        let data = match &mut object.data {
            Data::Struct(data) => data,
            _ => {
                return Err(
                    Error::custom("Optionize can only be derived for structs").with_span(&span)
                );
            }
        };

        let originals = {
            let fields = match &mut data.fields {
                Fields::Named(fields) => &mut fields.named,
                Fields::Unnamed(fields) => &mut fields.unnamed,
                Fields::Unit => &mut Default::default(),
            };
            FieldIr::extract(fields, krate.clone(), partial, reverse)?
        };

        let marker = if let Some(marked) = marked {
            let span = marked.span();
            let marked = marked.into_inner().unwrap_or_default();

            let mut attrs = vec![pqs! { span => #[doc(hidden)] }];
            marked.attrs.patch(&mut attrs);

            let ident = match (&data.fields, marked.name) {
                (Fields::Named(fields), None) => {
                    let names = fields
                        .named
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
                (Fields::Unnamed(_), Some(name)) => {
                    return Err(Error::custom(
                        "`name` attribute cannot be used on unnamed structs",
                    )
                    .with_span(&name));
                }
                (_, name) => name,
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
                    index: data.fields.len() as u32,
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

            match &mut data.fields {
                Fields::Named(fields) => fields.named.push(field),
                Fields::Unnamed(fields) => fields.unnamed.push(field),
                Fields::Unit => {
                    data.fields = if field.ident.is_some() {
                        Fields::Named(FieldsNamed {
                            brace_token: Brace(span),
                            named: [field].into_iter().collect(),
                        })
                    } else {
                        Fields::Unnamed(FieldsUnnamed {
                            paren_token: Paren(span),
                            unnamed: [field].into_iter().collect(),
                        })
                    };
                }
            }
            Some(marker)
        } else {
            None
        };

        (originals, marker)
    };

    if reverse {
        declarations.extend(q! { #[derive(#krate::__private::Optionize)] #object });
    } else if generated {
        declarations.extend(q! { #object });
    }

    let generics = object.generics;
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let where_clause = {
        let mut where_clause = where_clause.cloned().unwrap_or_else(|| pq! { where });
        extend_where_clause(
            &mut where_clause,
            originals
                .iter()
                .filter_map(|field| {
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
                .flatten(),
        );
        where_clause
    };
    let mut output = TokenStream::new();
    let this = format_ident!("self", span = Span::mixed_site());

    {
        let (view, view_lifetime, self_lifetime) = {
            let mut idents = HashSet::new();
            let fields = originals.iter().map(|field| {
                let ty = &field.ty;
                let nest = match &field.strategy {
                    FieldStrategy::Optionize { nest, .. } => nest.as_ref(),
                    FieldStrategy::Skip => None,
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
            let view = format::<Ident>(&pq! { "__{}OptionizeView" }, &ident)?;
            (
                new_ident(&view.to_string(), &idents),
                new_lifetime("v"),
                new_lifetime("s"),
            )
        };
        let generics = {
            let mut generics = generics.clone();
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
            output.extend(q! {
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
            output.extend(q! {
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
            output.extend(q! {
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
            let where_clause = {
                let mut where_clause = where_clause.clone();
                extend_where_clause(
                    &mut where_clause,
                    originals
                        .iter()
                        .filter_map(|field| {
                            let FieldIr { ty, strategy, index, span, .. } = field;
                            match strategy {
                                FieldStrategy::Skip => None,
                                FieldStrategy::Optionize { nest: None, .. } => {
                                    Some(pqs! { *span => for<#view_lifetime> &#view_lifetime #ty: #krate::__private::Equal<#index> })
                                }
                                FieldStrategy::Optionize { nest: Some(nest), .. } => {
                                    // The unused binder defers concrete comparison bounds until
                                    // Retain is used, keeping other operations available without it.
                                    Some(pqs! { *span => for<#view_lifetime> #nest: #krate::Retain<#ty> })
                                }
                            }
                        }),
                );
                where_clause
            };
            let baseline = format_ident!("baseline", span = Span::mixed_site());
            let remains = format_ident!("remains", span = Span::mixed_site());
            let fields = originals.iter().map(|field| Retain {
                field,
                baseline: &baseline,
                remains: &remains,
            });
            output.extend(q! {
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
            let where_clause = {
                let mut where_clause = where_clause.clone();
                extend_where_clause(
                    &mut where_clause,
                    originals.iter().filter_map(|field| {
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
                    }),
                );
                where_clause
            };
            let fields = originals.iter().map(|field| View {
                field,
                subject: true,
            });
            output.extend(q! {
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

    output.extend(q! {
        #[automatically_derived]
        impl #impl_generics #krate::Optionizable<#Object> for #Subject #where_clause {}
    });

    {
        let subject = format_ident!("subject", span = Span::mixed_site());

        let optionize = {
            let fields = originals.iter().map(|field| Optionize {
                field,
                subject: &subject,
            });
            q! {
                #[inline]
                fn optionize(#subject: #Subject) -> Self {
                    #[allow(clippy::init_numbered_fields)]
                    Self { #(#fields)* #marker }
                }
            }
        };
        let patch = {
            let fields = originals.iter().map(|field| Patch {
                field,
                subject: &subject,
            });
            q! {
                #[inline]
                fn patch(#this, #subject: &mut #Subject) { #(#fields)* }
            }
        };
        let merge = {
            let other = format_ident!("other", span = Span::mixed_site());
            let fields = originals.iter().map(|field| Merge {
                field,
                other: &other,
            });
            q! {
                #[inline]
                fn merge(&mut #this, #other: Self) { #(#fields)* }
            }
        };

        output.extend(q! {
            #[automatically_derived]
            impl #impl_generics #krate::PartialOptionized<#Subject> for #Object #where_clause {
                #optionize
                #patch
                #merge
            }
        });
    }

    if let Some(span) = upgradable {
        let where_clause = {
            let mut where_clause = where_clause;
            extend_where_clause(
                &mut where_clause,
                originals
                    .iter()
                    .filter_map(|field| {
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
                            // Only the yielded errors are boxed; their collection may borrow.
                            pqs! { *span => <<#nest as #krate::Optionized<#ty>>::Errors as ::core::iter::IntoIterator>::Item: 'static },
                        ])
                    })
                    .flatten(),
            );
            extend_where_clause(
                &mut where_clause,
                originals.iter().filter_map(|field| {
                    let FieldIr {
                        ty, default, span, ..
                    } = field;
                    matches!(default, Some(Override::Inherit)).then(|| {
                        pqs! { *span => #ty: ::core::default::Default }
                    })
                }),
            );
            where_clause
        };
        let validate = {
            let info = {
                let subject = Subject.to_string();
                let object = Object.to_string();
                qs! { span => #krate::TypeInfo { subject: #subject, object: #object } }
            };
            let failed = format_ident!("failed", span = Span::mixed_site());
            let errors = format_ident!("errors", span = Span::mixed_site());
            let fields = originals.iter().map(|field| Validate {
                field,
                info: &info,
                failed: &failed,
                errors: &errors,
            });
            qs! { span =>
                #[inline]
                fn validate(&#this) -> ::core::result::Result<(), Self::Errors> {
                    let mut #failed = false;
                    let mut #errors = #krate::ErrorCollection::default();
                    #(#fields)*
                    if !#failed {
                        ::core::result::Result::Ok(())
                    } else {
                        ::core::result::Result::Err(#errors)
                    }
                }
            }
        };
        let upgrade = {
            let defaults = originals.iter().map(UpgradeDefault);
            let upgrades = originals.iter().map(Upgrade);
            let fields = originals.iter().map(|field| {
                let FieldIr {
                    original,
                    local,
                    span,
                    ..
                } = field;
                qs! { *span => #original: #local, }
            });
            qs! { span =>
                #[inline]
                unsafe fn upgrade_unchecked(#this) -> #Subject {
                    #(#defaults)*
                    #(#upgrades)*
                    #[allow(clippy::init_numbered_fields)]
                    #subject_constructor { #(#fields)* }
                }
            }
        };

        output.extend(qs! { span =>
            #[automatically_derived]
            impl #impl_generics #krate::Optionized<#Subject> for #Object #where_clause {
                type Errors = #krate::ErrorCollection;
                #validate
                #upgrade
            }
        });
    }

    Ok(q! {
        #declarations
        #[allow(deprecated)]
        const _: () = { #output };
    })
}

pub fn proc(args: TokenStream, input: &TokenStream) -> Result<TokenStream> {
    let args = OptionizedArgs::from_list(&NestedMeta::parse_meta_list(args)?)?;
    let krate = args.krate.unwrap_or_else(Crate::infer);
    Ok(quote! {
        #[derive(#krate::__private::Prepare)]
        #[#krate::__private::discard]
        #[#krate::__private::expand(crate = #krate)]
        #input
    })
}

pub fn expand(args: TokenStream, input: &TokenStream) -> Result<TokenStream> {
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
