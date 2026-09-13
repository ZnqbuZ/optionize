use crate::PartialOptionized;

/// Reads borrowed fields from complete subjects and partial representations.
///
/// `Descriptor` selects the shared view and defaults to `Self`. The `optionized` macro
/// implements `Schema<Subject>` for the object and `Schema<Subject, Object>` for
/// the subject, including subjects from another crate. Other representations
/// can implement the same schema to act as baselines without patch operations.
pub trait Schema<Subject, Descriptor = Self>: Sized {
    /// The borrowed field representation. Generated subject schemas reuse the
    /// object's view type, including recursively constructed nested views.
    type View<'v>
    where
        Subject: 'v,
        Descriptor: 'v,
        Self: 'v;

    /// Borrows the known fields using the descriptor's shared view.
    fn view<'s>(&'s self) -> <Descriptor as Schema<Subject>>::View<'s>
    where
        Subject: 's,
        Descriptor: Schema<Subject> + 's;
}

/// Extracts the inner value of an optional field, including through type aliases.
#[doc(hidden)]
pub trait OptionField {
    type Value;
}

impl<Value> OptionField for Option<Value> {
    type Value = Value;
}

/// Compares one borrowed field without overlapping another field's bounds.
///
/// The index keeps higher-ranked bounds distinct when field types normalize to
/// the same type or differ only in their lifetimes.
#[doc(hidden)]
pub trait Equal<const INDEX: usize> {
    fn equal(self, other: Self) -> bool;
}

impl<Value: PartialEq + ?Sized, const INDEX: usize> Equal<INDEX> for &Value {
    fn equal(self, other: Self) -> bool {
        PartialEq::eq(self, other)
    }
}

/// Removes updates already represented by a borrowed baseline.
///
/// The baseline may be the complete subject or any partial representation using
/// the same schema. Missing baseline fields are unknown, so updates to them are
/// retained. Returns `true` when updates remain and `false` when the entire patch
/// can be omitted relative to this baseline.
///
/// Equal ordinary fields become `None`. Flattened fields keep their values and
/// contribute to the result; a containing optional nested patch can still be
/// removed when all its updates are redundant. Skipped fields are ignored.
/// Neither the baseline nor the patch's remaining values are cloned.
///
/// Generated objects gain this trait automatically when their compared field
/// types support equality. Other operations remain available without those
/// comparison bounds.
pub trait Retain<Subject, Descriptor: Schema<Subject> = Self>:
    PartialOptionized<Subject> + Schema<Subject, Descriptor>
{
    /// Retains changes relative to the shared borrowed field representation.
    #[doc(hidden)]
    fn retain_view<'v>(&mut self, baseline: Descriptor::View<'v>) -> bool
    where
        Subject: 'v,
        Descriptor: 'v;

    /// Removes redundant updates and reports whether any changes remain.
    fn retain<'b, Baseline: Schema<Subject, Descriptor>>(&mut self, baseline: &'b Baseline) -> bool
    where
        Subject: 'b,
        Descriptor: 'b,
    {
        self.retain_view(baseline.view())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::String;

    use super::*;

    struct Config<Value> {
        value: Value,
    }

    struct ConfigPatch<Value> {
        value: Option<Value>,
    }

    struct View<'v, Value> {
        value: Option<&'v Value>,
    }

    impl<Value> Schema<Config<Value>> for ConfigPatch<Value> {
        type View<'v>
            = View<'v, Value>
        where
            Self: 'v;
        fn view<'s>(&'s self) -> <Self as Schema<Config<Value>>>::View<'s>
        where
            Config<Value>: 's,
        {
            View {
                value: self.value.as_ref(),
            }
        }
    }

    impl<Value> Schema<Config<Value>, ConfigPatch<Value>> for Config<Value> {
        type View<'v>
            = View<'v, Value>
        where
            Self: 'v;
        fn view<'s>(&'s self) -> <ConfigPatch<Value> as Schema<Config<Value>>>::View<'s>
        where
            Config<Value>: 's,
        {
            View {
                value: Some(&self.value),
            }
        }
    }

    impl<Value> PartialOptionized<Config<Value>> for ConfigPatch<Value> {
        fn optionize(subject: Config<Value>) -> Self {
            Self {
                value: Some(subject.value),
            }
        }

        fn patch(self, subject: &mut Config<Value>) {
            if let Some(value) = self.value {
                subject.value = value;
            }
        }

        fn merge(&mut self, other: Self) {
            if other.value.is_some() {
                self.value = other.value;
            }
        }
    }

    impl<Value> Retain<Config<Value>> for ConfigPatch<Value>
    where
        for<'v> &'v Value: Equal<0>,
    {
        fn retain_view<'v>(&mut self, baseline: View<'v, Value>) -> bool
        where
            Config<Value>: 'v,
        {
            if let (Some(value), Some(baseline)) = (self.value.as_ref(), baseline.value)
                && Equal::<0>::equal(value, baseline)
            {
                self.value = None;
            }
            self.value.is_some()
        }
    }

    fn trim<Patch, Baseline, Subject, Descriptor>(patch: &mut Patch, baseline: &Baseline) -> bool
    where
        Patch: Retain<Subject, Descriptor>,
        Baseline: Schema<Subject, Descriptor>,
        Descriptor: Schema<Subject>,
    {
        patch.retain(baseline)
    }

    #[test]
    fn retain_accepts_borrowed_non_clone_values_through_generic_bounds() {
        #[derive(Debug, PartialEq)]
        struct Value<'s>(&'s str);

        let owned = String::from("borrowed");
        let baseline = Config {
            value: Value(&owned),
        };
        let mut patch = ConfigPatch {
            value: Some(Value(&owned)),
        };
        assert!(!trim(&mut patch, &baseline));
        assert_eq!(patch.value, None);
        assert_eq!(baseline.value, Value(&owned));

        let baseline = ConfigPatch { value: None };
        patch.value = Some(Value(&owned));
        assert!(trim(&mut patch, &baseline));
        assert_eq!(patch.value, Some(Value(&owned)));
    }

    #[test]
    fn retain_bound_includes_the_objects_own_schema() {
        fn trim_same<Subject, Patch: Retain<Subject>>(patch: &mut Patch, baseline: &Patch) -> bool {
            patch.retain(baseline)
        }

        fn trim_shared<Subject, Descriptor: Schema<Subject>, Patch: Retain<Subject, Descriptor>>(
            patch: &mut Patch,
            baseline: &Patch,
        ) -> bool {
            patch.retain(baseline)
        }

        let owned = String::from("borrowed");
        let baseline = ConfigPatch {
            value: Some(owned.as_str()),
        };
        let mut patch = ConfigPatch {
            value: Some(owned.as_str()),
        };
        assert!(!trim_same(&mut patch, &baseline));
        assert!(patch.value.is_none());

        patch.value = Some(owned.as_str());
        assert!(!trim_shared(&mut patch, &baseline));
        assert!(patch.value.is_none());
    }

    #[test]
    fn equal_distinguishes_fields_with_different_lifetimes() {
        fn both_equal<'s, 's_>(left: &(&'s str, &'s_ str), right: &(&'s str, &'s_ str)) -> bool
        where
            for<'s__> &'s__ &'s str: Equal<0>,
            for<'s__> &'s__ &'s_ str: Equal<1>,
        {
            Equal::<0>::equal(&left.0, &right.0) & Equal::<1>::equal(&left.1, &right.1)
        }

        let first = String::from("first");
        let second = String::from("second");
        let values = (first.as_str(), second.as_str());
        assert!(both_equal(&values, &values));
    }

    #[test]
    fn retain_accepts_nested_bounds_with_normalized_field_aliases() {
        // Exercise the same higher-ranked bounds as the generated impl.
        #[allow(unused_lifetimes)]
        fn trim_two<Value>(
            first: &mut ConfigPatch<Value>,
            second: &mut <Option<ConfigPatch<Value>> as OptionField>::Value,
            baseline: &Config<Value>,
        ) -> bool
        where
            for<'c> ConfigPatch<Value>: Retain<Config<Value>>,
            for<'c> <Option<ConfigPatch<Value>> as OptionField>::Value: Retain<Config<Value>>,
        {
            let first = Retain::<Config<Value>>::retain_view(first, baseline.view());
            let second = Retain::<Config<Value>>::retain_view(second, baseline.view());
            first | second
        }

        let owned = String::from("borrowed");
        let baseline = Config {
            value: owned.as_str(),
        };
        let mut first = ConfigPatch {
            value: Some(owned.as_str()),
        };
        let mut second = ConfigPatch {
            value: Some(owned.as_str()),
        };
        assert!(!trim_two(&mut first, &mut second, &baseline));
        assert!(first.value.is_none());
        assert!(second.value.is_none());
    }
}
