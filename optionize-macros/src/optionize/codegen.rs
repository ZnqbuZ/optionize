use darling::util::Override;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident};

use super::field::{FieldIr, FieldStrategy};
use super::utils::{member_to_string, span};

macro_rules! expand {
    ($target:expr => { $($field:ident $(: $bind:pat)?),* $(,)? }) => {
        let FieldIr {
            #[allow(unused_variables)]
            span,
            $(
                $field $(: $bind)?,
            )*
            ..
        } = $target;

        span!(*span);
    };
}

pub(super) struct View<'f> {
    pub(super) field: &'f FieldIr,
    pub(super) subject: bool,
}

impl ToTokens for View<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.field => { krate, ty, original, optionized, strategy, local } }
        let this = format_ident!("self", span = Span::mixed_site());
        let view = if self.subject {
            if let FieldStrategy::Optionize {
                nest: Some(nest), ..
            } = strategy
            {
                q! { Option::Some(<#ty as #krate::Schema<#ty, #nest>>::view(&#this.#original)) }
            } else {
                q! { Option::Some(&#this.#original) }
            }
        } else if matches!(
            strategy,
            FieldStrategy::Optionize {
                convert: Some(_),
                ..
            }
        ) {
            // The two sides hold different types, so the shared view cannot borrow
            // this field. The object reports an unknown baseline field.
            q! { Option::None }
        } else if let FieldStrategy::Optionize { wrap, nest, .. } = strategy {
            let view = if *wrap {
                q! { #this.#optionized.as_ref() }
            } else {
                q! { Option::Some(&#this.#optionized) }
            };
            if let Some(nest) = nest {
                q! { #view.map(<#nest as #krate::Schema<#ty>>::view) }
            } else {
                view
            }
        } else {
            q! { Option::None }
        };
        tokens.extend(q! { #local: #view, });
    }
}

pub(super) struct Retain<'f, 'i> {
    pub(super) field: &'f FieldIr,
    pub(super) baseline: &'i Ident,
    pub(super) remains: &'i Ident,
}

impl ToTokens for Retain<'_, '_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.field => { krate, ty, optionized, strategy, index, local } }
        let FieldStrategy::Optionize {
            wrap,
            nest,
            convert,
        } = strategy
        else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let baseline = self.baseline;
        let remains = self.remains;
        // A converted field has no comparable view entry; any stored update is kept.
        let retain = if convert.is_some() {
            if *wrap {
                q! { #remains |= #this.#optionized.is_some(); }
            } else {
                q! { #remains = true; }
            }
        } else {
            match (wrap, nest) {
                (true, None) => q! {
                    if let (Option::Some(value), Option::Some(baseline)) =
                        (#this.#optionized.as_ref(), #baseline.#local)
                        && #krate::__private::Equal::<#index>::equal(value, baseline)
                    {
                        #this.#optionized = Option::None;
                    }
                    #remains |= #this.#optionized.is_some();
                },
                (false, None) => q! {
                    #remains |= #baseline.#local.is_none_or(|baseline| {
                        !#krate::__private::Equal::<#index>::equal(&#this.#optionized, baseline)
                    });
                },
                (true, Some(nest)) => q! {
                    if let (Option::Some(value), Option::Some(baseline)) =
                        (#this.#optionized.as_mut(), #baseline.#local)
                        && !<#nest as #krate::Retain<#ty>>::retain_view(value, baseline)
                    {
                        #this.#optionized = Option::None;
                    }
                    #remains |= #this.#optionized.is_some();
                },
                (false, Some(nest)) => q! {
                    #remains |= #baseline.#local.is_none_or(|baseline| {
                        <#nest as #krate::Retain<#ty>>::retain_view(&mut #this.#optionized, baseline)
                    });
                },
            }
        };
        tokens.extend(retain);
    }
}

pub(super) struct Optionize<'f, 'i> {
    pub(super) field: &'f FieldIr,
    pub(super) subject: &'i Ident,
}

impl ToTokens for Optionize<'_, '_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
            }
        }

        let FieldStrategy::Optionize {
            wrap,
            nest,
            convert,
        } = strategy
        else {
            return;
        };

        let subject = self.subject;
        let optionize = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty>>::optionize(#subject.#original) }
        } else if convert.is_some() {
            q! { ::core::convert::Into::into(#subject.#original) }
        } else {
            q! { #subject.#original }
        };
        let optionize = if *wrap {
            q! { Option::Some(#optionize) }
        } else {
            optionize
        };

        tokens.extend(q! { #optionized: #optionize, });
    }
}

pub(super) struct Patch<'f, 'i> {
    pub(super) field: &'f FieldIr,
    pub(super) subject: &'i Ident,
}

impl ToTokens for Patch<'_, '_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
            }
        }
        let FieldStrategy::Optionize {
            wrap,
            nest,
            convert,
        } = strategy
        else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let subject = self.subject;
        let patch = if *wrap {
            q! { v }
        } else {
            q! { #this.#optionized }
        };
        let patch = if convert.is_some() {
            q! { ::core::convert::Into::into(#patch) }
        } else {
            patch
        };
        let patch = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty>>::patch(#patch, &mut #subject.#original); }
        } else {
            q! { #subject.#original = #patch; }
        };
        let patch = if *wrap {
            q! {
                if let Option::Some(v) = #this.#optionized {
                    #patch
                }
            }
        } else {
            patch
        };

        tokens.extend(patch);
    }
}

pub(super) struct Merge<'f, 'i> {
    pub(super) field: &'f FieldIr,
    pub(super) other: &'i Ident,
}

impl ToTokens for Merge<'_, '_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                optionized,
                strategy,
            }
        }
        let FieldStrategy::Optionize { wrap, nest, .. } = strategy else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let other = self.other;
        let merge = match (wrap, nest) {
            (true, Some(nest)) => q! {
                match (&mut #this.#optionized, #other.#optionized) {
                    (Option::Some(this), Option::Some(other)) => <#nest as #krate::PartialOptionized<#ty>>::merge(this, other),
                    (Option::None, Option::Some(other)) => #this.#optionized = Option::Some(other),
                    _ => {}
                }
            },
            (true, None) => q! {
                if Option::is_some(&#other.#optionized) {
                    #this.#optionized = #other.#optionized;
                }
            },
            (false, Some(nest)) => q! {
                <#nest as #krate::PartialOptionized<#ty>>::merge(&mut #this.#optionized, #other.#optionized);
            },
            (false, None) => q! {
                #this.#optionized = #other.#optionized;
            },
        };

        tokens.extend(merge);
    }
}

pub(super) struct Validate<'f, 't, 'i> {
    pub(super) field: &'f FieldIr,
    pub(super) info: &'t TokenStream,
    pub(super) failed: &'i Ident,
    pub(super) errors: &'i Ident,
}

impl ToTokens for Validate<'_, '_, '_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.field => {
                krate,
                ty,
                original,
                optionized,
                strategy,
                local,
                default,
            }
        }
        let FieldStrategy::Optionize { wrap, nest, .. } = strategy else {
            return;
        };
        if (!*wrap || default.is_some()) && nest.is_none() {
            return;
        }

        let field = {
            let original = member_to_string(original);
            let optionized = member_to_string(optionized);

            if original == optionized {
                q! { #krate::FieldInfo::Identical(#original) }
            } else {
                q! { #krate::FieldInfo::Renamed { original: #original, optionized: #optionized } }
            }
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let info = self.info;
        let failed = self.failed;
        let errors = self.errors;
        let validate = nest.as_ref().map(|nest| {
            let value = if *wrap {
                q! { #local }
            } else {
                q! { &#this.#optionized }
            };
            q! {
                if let ::core::result::Result::Err(e) = <#nest as #krate::Optionized<#ty>>::validate(#value) {
                    #failed = true;
                    #errors.extend(::core::iter::IntoIterator::into_iter(e).map(|e| #krate::Error::Nested {
                        ty: #info,
                        field: #field,
                        source: #krate::__private::alloc::boxed::Box::new(e) as _,
                    }));
                }
            }
        });

        let validate = if *wrap {
            let missing = default.is_none().then(|| {
                q! {
                    else {
                        #failed = true;
                        #errors.push(#krate::Error::Missing {
                            ty: #info,
                            field: #field,
                        });
                    }
                }
            });
            q! {
                if let Option::Some(#local) = &#this.#optionized {
                    #validate
                }
                #missing
            }
        } else {
            q! { #validate }
        };

        tokens.extend(validate);
    }
}

pub(super) struct Upgrade<'f>(pub(super) &'f FieldIr);

impl ToTokens for Upgrade<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.0 => { krate, ty, optionized, strategy, local, default } }
        let FieldStrategy::Optionize {
            wrap,
            nest,
            convert,
        } = strategy
        else {
            return;
        };
        let this = format_ident!("self", span = Span::mixed_site());
        let value = if *wrap {
            q! { value }
        } else {
            q! { #this.#optionized }
        };
        let value = if let Some(nest) = nest {
            // A default callback can change a sibling's shared interior state.
            // Check each child when consuming it, after all parent defaults ran.
            q! {
                <#nest as #krate::Optionized<#ty>>::upgrade(#value)
                    .unwrap_or_else(|_| panic!("nested object became invalid during upgrading"))
            }
        } else {
            value
        };
        let value = if convert.is_some() {
            q! { ::core::convert::Into::into(#value) }
        } else {
            value
        };
        let value = if *wrap {
            let missing = if default.is_some() {
                // Defaults were prepared before moving any field from self.
                q! { #local.expect("missing field default was prepared") }
            } else {
                q! { unreachable!("validated field is missing") }
            };
            q! {
                match #this.#optionized {
                    Option::Some(value) => #value,
                    Option::None => #missing,
                }
            }
        } else {
            value
        };
        tokens.extend(q! { let #local = #value; });
    }
}

pub(super) struct UpgradeDefault<'f>(pub(super) &'f FieldIr);

impl ToTokens for UpgradeDefault<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.0 => { ty, optionized, strategy, local, default } }
        let Some(default) = default else {
            return;
        };
        let default = match default {
            Override::Explicit(default) => q! { #default },
            Override::Inherit => q! { |_| <#ty as ::core::default::Default>::default() },
        };
        let this = format_ident!("self", span = Span::mixed_site());
        let value = q! {
            {
                let default: fn(&Self) -> #ty = #default;
                default(&#this)
            }
        };
        let value = if matches!(strategy, FieldStrategy::Skip) {
            value
        } else {
            q! { #this.#optionized.is_none().then(|| #value) }
        };
        tokens.extend(q! { let #local = #value; });
    }
}
