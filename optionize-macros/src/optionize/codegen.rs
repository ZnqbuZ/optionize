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
                q! { ::core::option::Option::Some(<#ty as #krate::Schema<#ty, #nest>>::view(&#this.#original)) }
            } else {
                q! { ::core::option::Option::Some(&#this.#original) }
            }
        } else if let FieldStrategy::Optionize { wrap, nest } = strategy {
            let view = if *wrap {
                q! { #this.#optionized.as_ref() }
            } else {
                q! { ::core::option::Option::Some(&#this.#optionized) }
            };
            if let Some(nest) = nest {
                q! { #view.map(<#nest as #krate::Schema<#ty>>::view) }
            } else {
                view
            }
        } else {
            q! { ::core::option::Option::None }
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
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let baseline = self.baseline;
        let remains = self.remains;
        let retain = match (wrap, nest) {
            (true, None) => q! {
                if let (::core::option::Option::Some(value), ::core::option::Option::Some(baseline)) =
                    (#this.#optionized.as_ref(), #baseline.#local)
                    && #krate::__private::Equal::<#index>::equal(value, baseline)
                {
                    #this.#optionized = ::core::option::Option::None;
                }
                #remains |= #this.#optionized.is_some();
            },
            (false, None) => q! {
                #remains |= #baseline.#local.is_none_or(|baseline| {
                    !#krate::__private::Equal::<#index>::equal(&#this.#optionized, baseline)
                });
            },
            (true, Some(nest)) => q! {
                if let (::core::option::Option::Some(value), ::core::option::Option::Some(baseline)) =
                    (#this.#optionized.as_mut(), #baseline.#local)
                    && !<#nest as #krate::Retain<#ty>>::retain_view(value, baseline)
                {
                    #this.#optionized = ::core::option::Option::None;
                }
                #remains |= #this.#optionized.is_some();
            },
            (false, Some(nest)) => q! {
                #remains |= #baseline.#local.is_none_or(|baseline| {
                    <#nest as #krate::Retain<#ty>>::retain_view(&mut #this.#optionized, baseline)
                });
            },
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

        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let subject = self.subject;
        let optionize = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty>>::optionize(#subject.#original) }
        } else {
            q! { #subject.#original }
        };
        let optionize = if *wrap {
            q! { ::core::option::Option::Some(#optionize) }
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
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let subject = self.subject;
        let patch = if *wrap {
            q! { v }
        } else {
            q! { #this.#optionized }
        };
        let patch = if let Some(nest) = nest {
            q! { <#nest as #krate::PartialOptionized<#ty>>::patch(#patch, &mut #subject.#original); }
        } else {
            q! { #subject.#original = #patch; }
        };
        let patch = if *wrap {
            q! {
                if let ::core::option::Option::Some(v) = #this.#optionized {
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
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        let other = self.other;
        let merge = match (wrap, nest) {
            (true, Some(nest)) => q! {
                match (&mut #this.#optionized, #other.#optionized) {
                    (::core::option::Option::Some(this), ::core::option::Option::Some(other)) => <#nest as #krate::PartialOptionized<#ty>>::merge(this, other),
                    (::core::option::Option::None, ::core::option::Option::Some(other)) => #this.#optionized = ::core::option::Option::Some(other),
                    _ => {}
                }
            },
            (true, None) => q! {
                if ::core::option::Option::is_some(&#other.#optionized) {
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
            }
        }
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };
        if !*wrap && nest.is_none() {
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
            q! {
                if let ::core::option::Option::Some(#local) = &#this.#optionized {
                    #validate
                } else {
                    #failed = true;
                    #errors.push(#krate::Error::Missing {
                        ty: #info,
                        field: #field,
                    });
                }
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
        expand! {
            self.0 => {
                krate,
                ty,
                optionized,
                strategy,
                local,
            }
        }
        let FieldStrategy::Optionize { wrap, nest } = strategy else {
            return;
        };

        let this = format_ident!("self", span = Span::mixed_site());
        tokens.extend(q! { let #local = #this.#optionized; });
        if *wrap {
            tokens.extend(
                q! { let #local = unsafe { ::core::option::Option::unwrap_unchecked(#local) }; },
            );
        }
        if let Some(nest) = nest {
            tokens.extend(q! {
                let #local = unsafe { <#nest as #krate::Optionized<#ty>>::upgrade_unchecked(#local) };
            })
        }
    }
}

pub(super) struct UpgradeSkip<'f>(pub(super) &'f FieldIr);

impl ToTokens for UpgradeSkip<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! {
            self.0 => {
                ty,
                strategy,
                local,
            }
        }

        if let FieldStrategy::Skip { upgrade } = strategy {
            tokens.extend(q! { let #local: #ty = { #upgrade }; });
        }
    }
}

pub(super) struct UpgradeFieldValue<'f>(pub(super) &'f FieldIr);

impl ToTokens for UpgradeFieldValue<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        expand! { self.0 => { original, local } }
        tokens.extend(q! { #original: #local, });
    }
}
