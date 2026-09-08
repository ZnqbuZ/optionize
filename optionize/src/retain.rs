use crate::PartialOptionized;

/// Selects the common borrowed view of a subject and its partial representations.
///
/// The `optionized` macro implements this for its subject automatically. A local
/// descriptor can implement it for an external subject without changing that
/// subject's crate. Partial representations compared with one another must use
/// the same descriptor.
pub trait Schema<S> {
    /// The borrowed fields, including recursively constructed nested views.
    type View<'a>
    where
        S: 'a,
        Self: 'a;

    /// Borrows every field of a complete subject without cloning its values.
    fn full_view<'a>(subject: &'a S) -> Self::View<'a>
    where
        Self: 'a;
}

/// Selects the descriptor for a particular object-to-subject mapping.
#[doc(hidden)]
pub trait Mapping<S> {
    type Descriptor: Schema<S>;
}

impl<S: Schema<S>> Mapping<S> for S {
    type Descriptor = S;
}

/// Extracts the inner value of an optional field, including through type aliases.
#[doc(hidden)]
pub trait OptionField {
    type Value;
}

impl<T> OptionField for Option<T> {
    type Value = T;
}

/// Compares one borrowed field without overlapping another field's bounds.
///
/// The index keeps higher-ranked bounds distinct when field types normalize to
/// the same type or differ only in their lifetimes.
#[doc(hidden)]
pub trait Equal<const INDEX: usize> {
    fn equal(self, other: Self) -> bool;
}

impl<T: PartialEq + ?Sized, const INDEX: usize> Equal<INDEX> for &T {
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
pub trait Retain<S, D: Schema<S> = S>: PartialOptionized<S, D> {
    /// Retains changes relative to the shared borrowed field representation.
    #[doc(hidden)]
    fn retain_view<'a>(&mut self, baseline: D::View<'a>) -> bool
    where
        S: 'a,
        D: 'a;

    /// Removes redundant updates and reports whether any changes remain.
    fn retain<'a, B: PartialOptionized<S, D>>(&mut self, baseline: &'a B) -> bool
    where
        S: 'a,
        D: 'a,
    {
        self.retain_view(baseline.view())
    }
}

/// Compares a borrowed nested patch against its already constructed baseline view.
///
/// Generated implementations place a higher-ranked bound on the borrowed patch
/// so unavailable nested comparison does not restrict patching and upgrading.
#[doc(hidden)]
pub trait NestedRetain<S, D: Schema<S> = S, const INDEX: usize = 0> {
    fn retain_nested<'a>(self, baseline: D::View<'a>) -> bool
    where
        S: 'a,
        D: 'a;
}

impl<P, S, D, const INDEX: usize> NestedRetain<S, D, INDEX> for &mut P
where
    D: Schema<S>,
    P: Retain<S, D>,
{
    fn retain_nested<'a>(self, baseline: D::View<'a>) -> bool
    where
        S: 'a,
        D: 'a,
    {
        self.retain_view(baseline)
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::String;

    use super::*;

    struct Subject<T> {
        value: T,
    }

    struct Patch<T> {
        value: Option<T>,
    }

    struct View<'a, T> {
        value: Option<&'a T>,
    }

    impl<T> Schema<Subject<T>> for Subject<T> {
        type View<'a>
            = View<'a, T>
        where
            Self: 'a;

        fn full_view<'a>(subject: &'a Subject<T>) -> View<'a, T>
        where
            Self: 'a,
        {
            View {
                value: Some(&subject.value),
            }
        }
    }

    impl<T> PartialOptionized<Subject<T>> for Patch<T> {
        fn optionize(subject: Subject<T>) -> Self {
            Self {
                value: Some(subject.value),
            }
        }

        fn patch(self, subject: &mut Subject<T>) {
            if let Some(value) = self.value {
                subject.value = value;
            }
        }

        fn merge(&mut self, other: Self) {
            if other.value.is_some() {
                self.value = other.value;
            }
        }

        fn view<'a>(&'a self) -> <Subject<T> as Schema<Subject<T>>>::View<'a>
        where
            Subject<T>: 'a,
        {
            View {
                value: self.value.as_ref(),
            }
        }
    }

    impl<T> Retain<Subject<T>> for Patch<T>
    where
        for<'a> &'a T: Equal<0>,
    {
        fn retain_view<'a>(&mut self, baseline: View<'a, T>) -> bool
        where
            Subject<T>: 'a,
        {
            if let (Some(value), Some(baseline)) = (self.value.as_ref(), baseline.value)
                && Equal::<0>::equal(value, baseline)
            {
                self.value = None;
            }
            self.value.is_some()
        }
    }

    fn trim<P, B, S, D>(patch: &mut P, baseline: &B) -> bool
    where
        P: Retain<S, D>,
        B: PartialOptionized<S, D>,
        D: Schema<S>,
    {
        patch.retain(baseline)
    }

    #[test]
    fn generic_retain_accepts_borrowed_values_without_lifetime_bounds() {
        let owned = String::from("borrowed");
        let baseline = Subject {
            value: owned.as_str(),
        };
        let mut patch = Patch {
            value: Some(owned.as_str()),
        };
        assert!(!trim(&mut patch, &baseline));
        assert_eq!(patch.value, None);
        assert_eq!(baseline.value, owned.as_str());
    }

    #[test]
    fn nested_adapter_handles_full_and_partial_baselines_without_clone() {
        #[derive(PartialEq)]
        struct NoClone(String);

        let baseline = Subject {
            value: NoClone(String::from("same")),
        };
        let mut patch = Patch {
            value: Some(NoClone(String::from("same"))),
        };
        assert!(!NestedRetain::<Subject<NoClone>>::retain_nested(
            &mut patch,
            Subject::full_view(&baseline),
        ));
        assert!(patch.value.is_none());

        let baseline = Patch::<NoClone> { value: None };
        patch.value = Some(NoClone(String::from("changed")));
        assert!(NestedRetain::<Subject<NoClone>>::retain_nested(
            &mut patch,
            baseline.view(),
        ));
        assert!(patch.value.is_some());
    }

    #[test]
    fn clear_values_are_distinct_from_unknown_baseline_fields() {
        let mut patch = Patch { value: Some(None) };
        let unknown = Patch::<Option<u32>> { value: None };
        assert!(trim(&mut patch, &unknown));
        assert_eq!(patch.value, Some(None));

        let known_clear = Patch::<Option<u32>> { value: Some(None) };
        assert!(!trim(&mut patch, &known_clear));
        assert_eq!(patch.value, None);
    }

    #[test]
    fn equality_is_not_required_for_other_partial_operations() {
        struct NoEq;
        let mut subject = Subject { value: NoEq };
        let mut patch = Patch { value: None };
        patch.merge(Patch { value: Some(NoEq) });
        patch.patch(&mut subject);
    }

    #[test]
    fn indexed_equality_distinguishes_fields_with_different_lifetimes() {
        fn both_equal<'s, 't>(left: &(&'s str, &'t str), right: &(&'s str, &'t str)) -> bool
        where
            for<'a> &'a &'s str: Equal<0>,
            for<'a> &'a &'t str: Equal<1>,
        {
            Equal::<0>::equal(&left.0, &right.0) & Equal::<1>::equal(&left.1, &right.1)
        }

        let first = String::from("first");
        let second = String::from("second");
        let values = (first.as_str(), second.as_str());
        assert!(both_equal(&values, &values));
    }

    #[test]
    fn indexed_nested_bounds_distinguish_normalized_field_aliases() {
        fn trim_two<T>(
            first: &mut Patch<T>,
            second: &mut <Option<Patch<T>> as OptionField>::Value,
            baseline: &Subject<T>,
        ) -> bool
        where
            for<'a> &'a mut Patch<T>: NestedRetain<Subject<T>, Subject<T>, 0>,
            for<'a> &'a mut <Option<Patch<T>> as OptionField>::Value:
                NestedRetain<Subject<T>, Subject<T>, 1>,
        {
            let first = NestedRetain::<Subject<T>, Subject<T>, 0>::retain_nested(
                first,
                Subject::full_view(baseline),
            );
            let second = NestedRetain::<Subject<T>, Subject<T>, 1>::retain_nested(
                second,
                Subject::full_view(baseline),
            );
            first | second
        }

        let owned = String::from("borrowed");
        let baseline = Subject {
            value: owned.as_str(),
        };
        let mut first = Patch {
            value: Some(owned.as_str()),
        };
        let mut second = Patch {
            value: Some(owned.as_str()),
        };
        assert!(!trim_two(&mut first, &mut second, &baseline));
        assert!(first.value.is_none());
        assert!(second.value.is_none());
    }
}
