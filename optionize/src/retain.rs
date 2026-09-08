use core::marker::PhantomData;

use crate::PartialOptionized;

/// The common borrowed field representation selected by a schema.
#[doc(hidden)]
pub trait Layout {
    type Ref<'a>
    where
        Self: 'a;
}

/// A field whose complete value is either known or absent from the baseline.
#[doc(hidden)]
pub struct Field<T>(PhantomData<fn() -> T>);

impl<T> Layout for Field<T> {
    type Ref<'a>
        = Option<&'a T>
    where
        T: 'a;
}

impl<L: Layout, R: Layout> Layout for (L, R) {
    type Ref<'a>
        = (L::Ref<'a>, R::Ref<'a>)
    where
        Self: 'a;
}

impl Layout for () {
    type Ref<'a> = ();
}

/// Selects the common field layout for a subject and its partial representations.
///
/// The `optionized` macro implements this for its subject automatically. A local
/// descriptor can implement it for an external subject without changing that
/// subject's crate. Partial representations compared with one another must use
/// the same descriptor.
pub trait Schema<S> {
    #[doc(hidden)]
    type Layout: Layout;

    /// Borrows every field of a complete subject without cloning its values.
    fn full_view<'a>(subject: &'a S) -> <Self::Layout as Layout>::Ref<'a>
    where
        Self::Layout: 'a;
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

/// A complete nested value or another object's partial view of that value.
#[doc(hidden)]
pub enum NestedRef<'a, S: 'a, L: Layout + 'a> {
    Full(&'a S),
    Partial(L::Ref<'a>),
}

/// The explicit layout parameter makes its lifetime part of this type's bounds.
#[doc(hidden)]
#[allow(clippy::type_complexity)]
pub struct Branch<S, D: Schema<S>, L: Layout = <D as Schema<S>>::Layout>(
    PhantomData<fn() -> (S, D, L)>,
);

#[doc(hidden)]
pub type Nested<S, D = S> = Branch<S, D>;

impl<S, D, L> Layout for Branch<S, D, L>
where
    D: Schema<S, Layout = L>,
    L: Layout,
{
    type Ref<'a>
        = Option<NestedRef<'a, S, L>>
    where
        Self: 'a;
}

/// A strategy's mutable access to one layout or a group of layouts.
#[doc(hidden)]
pub trait Access<L: Layout> {
    type Mut<'a>
    where
        Self: 'a,
        L: 'a;
}

/// Removes redundant `Some` values from an ordinary optional field.
#[doc(hidden)]
pub struct Take;

/// Ignores a field omitted from this partial representation.
#[doc(hidden)]
pub struct Skip;

/// Keeps a flattened field and reports whether its value still changes baseline.
#[doc(hidden)]
pub struct Always;

/// Recursively reduces an optional nested patch.
#[doc(hidden)]
pub struct TakeNested<P>(PhantomData<fn() -> P>);

/// Recursively reduces a flattened nested patch.
#[doc(hidden)]
pub struct AlwaysNested<P>(PhantomData<fn() -> P>);

impl<T> Access<Field<T>> for Take {
    type Mut<'a>
        = &'a mut Option<T>
    where
        T: 'a;
}

impl<L: Layout> Access<L> for Skip {
    type Mut<'a>
        = ()
    where
        L: 'a;
}

impl<T> Access<Field<T>> for Always {
    type Mut<'a>
        = &'a mut T
    where
        T: 'a;
}

impl<S, D, L> Access<Branch<S, D, L>> for Take
where
    D: Schema<S, Layout = L>,
    L: Layout,
{
    type Mut<'a>
        = &'a mut Option<S>
    where
        Branch<S, D, L>: 'a;
}

impl<S, D, L> Access<Branch<S, D, L>> for Always
where
    D: Schema<S, Layout = L>,
    L: Layout,
{
    type Mut<'a>
        = &'a mut S
    where
        Branch<S, D, L>: 'a;
}

impl<P, S, D, L> Access<Branch<S, D, L>> for TakeNested<P>
where
    D: Schema<S, Layout = L>,
    L: Layout,
{
    type Mut<'a>
        = &'a mut Option<P>
    where
        Self: 'a,
        Branch<S, D, L>: 'a;
}

impl<P, S, D, L> Access<Branch<S, D, L>> for AlwaysNested<P>
where
    D: Schema<S, Layout = L>,
    L: Layout,
{
    type Mut<'a>
        = &'a mut P
    where
        Self: 'a,
        Branch<S, D, L>: 'a;
}

impl<L, R, A, B> Access<(L, R)> for (A, B)
where
    L: Layout,
    R: Layout,
    A: Access<L>,
    B: Access<R>,
{
    type Mut<'a>
        = (A::Mut<'a>, B::Mut<'a>)
    where
        Self: 'a,
        (L, R): 'a;
}

impl Access<()> for () {
    type Mut<'a> = ();
}

/// Borrows the fields that a generated object is able to update.
#[doc(hidden)]
pub trait FieldAccess<S, D: Schema<S> = S>: PartialOptionized<S, D> {
    type Access: Access<D::Layout>;

    fn fields_mut<'a>(&'a mut self) -> <Self::Access as Access<D::Layout>>::Mut<'a>
    where
        Self::Access: 'a,
        D::Layout: 'a;
}

/// Comparison is implemented on generic strategies, not concrete user structs.
#[doc(hidden)]
pub trait Comparable<L: Layout>: Access<L> {
    /// Returns whether updates remain after all fields have been visited.
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: L::Ref<'b>) -> bool
    where
        Self: 'a,
        L: 'a + 'b;
}

impl<T: PartialEq> Comparable<Field<T>> for Take {
    fn retain<'a, 'b>(fields: &'a mut Option<T>, baseline: Option<&'b T>) -> bool
    where
        T: 'a + 'b,
    {
        if let (Some(value), Some(baseline)) = (fields.as_ref(), baseline)
            && value == baseline
        {
            *fields = None;
        }
        fields.is_some()
    }
}

impl<L: Layout> Comparable<L> for Skip {
    fn retain<'a, 'b>(_: (), _: L::Ref<'b>) -> bool
    where
        L: 'a + 'b,
    {
        false
    }
}

impl<T: PartialEq> Comparable<Field<T>> for Always {
    fn retain<'a, 'b>(fields: &'a mut T, baseline: Option<&'b T>) -> bool
    where
        T: 'a + 'b,
    {
        baseline.is_none_or(|baseline| &*fields != baseline)
    }
}

impl<S, D, L> Comparable<Branch<S, D, L>> for Take
where
    S: PartialEq,
    D: Schema<S, Layout = L>,
    L: Layout,
{
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: Option<NestedRef<'b, S, L>>) -> bool
    where
        Branch<S, D, L>: 'a + 'b,
    {
        if let (Some(value), Some(NestedRef::Full(baseline))) = (fields.as_ref(), baseline)
            && value == baseline
        {
            *fields = None;
        }
        fields.is_some()
    }
}

impl<S, D, L> Comparable<Branch<S, D, L>> for Always
where
    S: PartialEq,
    D: Schema<S, Layout = L>,
    L: Layout,
{
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: Option<NestedRef<'b, S, L>>) -> bool
    where
        Branch<S, D, L>: 'a + 'b,
    {
        match baseline {
            Some(NestedRef::Full(baseline)) => &*fields != baseline,
            _ => true,
        }
    }
}

impl<P, S, D, L> Comparable<Branch<S, D, L>> for TakeNested<P>
where
    D: Schema<S, Layout = L>,
    L: Layout,
    P: FieldAccess<S, D>,
    P::Access: Comparable<L>,
{
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: Option<NestedRef<'b, S, L>>) -> bool
    where
        Self: 'a,
        Branch<S, D, L>: 'a + 'b,
    {
        let Some(patch) = fields.as_mut() else {
            return false;
        };
        let Some(baseline) = baseline else {
            // A present patch adds presence when merged into an absent field,
            // even when the nested patch contains no updates of its own.
            return true;
        };
        let baseline = match baseline {
            NestedRef::Full(subject) => D::full_view(subject),
            NestedRef::Partial(view) => view,
        };
        let remains = <P::Access as Comparable<L>>::retain(patch.fields_mut(), baseline);
        if !remains {
            *fields = None;
        }
        remains
    }
}

impl<P, S, D, L> Comparable<Branch<S, D, L>> for AlwaysNested<P>
where
    D: Schema<S, Layout = L>,
    L: Layout,
    P: FieldAccess<S, D>,
    P::Access: Comparable<L>,
{
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: Option<NestedRef<'b, S, L>>) -> bool
    where
        Self: 'a,
        Branch<S, D, L>: 'a + 'b,
    {
        let Some(baseline) = baseline else {
            return true;
        };
        let baseline = match baseline {
            NestedRef::Full(subject) => D::full_view(subject),
            NestedRef::Partial(view) => view,
        };
        <P::Access as Comparable<L>>::retain(fields.fields_mut(), baseline)
    }
}

impl<L, R, A, B> Comparable<(L, R)> for (A, B)
where
    L: Layout,
    R: Layout,
    A: Comparable<L>,
    B: Comparable<R>,
{
    fn retain<'a, 'b>(fields: Self::Mut<'a>, baseline: <(L, R) as Layout>::Ref<'b>) -> bool
    where
        Self: 'a,
        (L, R): 'a + 'b,
    {
        let left = A::retain(fields.0, baseline.0);
        let right = B::retain(fields.1, baseline.1);
        left | right
    }
}

impl Comparable<()> for () {
    fn retain<'a, 'b>(_: (), _: ()) -> bool
    where
        Self: 'a,
        (): 'a + 'b,
    {
        false
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
/// types implement `PartialEq`. Other operations remain available without those
/// comparison bounds.
pub trait Retain<S, D: Schema<S> = S>: FieldAccess<S, D> {
    fn retain<'a, B: PartialOptionized<S, D>>(&'a mut self, baseline: &'a B) -> bool
    where
        Self::Access: 'a,
        D::Layout: 'a;
}

impl<P, S, D> Retain<S, D> for P
where
    D: Schema<S>,
    P: FieldAccess<S, D>,
    P::Access: Comparable<D::Layout>,
{
    fn retain<'a, B: PartialOptionized<S, D>>(&'a mut self, baseline: &'a B) -> bool
    where
        Self::Access: 'a,
        D::Layout: 'a,
    {
        <P::Access as Comparable<D::Layout>>::retain(self.fields_mut(), baseline.view())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_fields_are_processed_when_an_earlier_change_remains() {
        let mut first = Some(1);
        let mut second = Some(2);
        let remains = <(Take, Take) as Comparable<(Field<i32>, Field<i32>)>>::retain(
            (&mut first, &mut second),
            (Some(&3), Some(&2)),
        );
        assert!(remains);
        assert_eq!(first, Some(1));
        assert_eq!(second, None);
    }

    #[test]
    fn a_skipped_field_does_not_need_partial_eq() {
        struct NoEq;
        assert!(!<Skip as Comparable<Field<NoEq>>>::retain((), Some(&NoEq)));
    }

    #[test]
    fn optional_values_distinguish_unknown_clear_and_set() {
        let mut clear = Some(None::<u32>);
        assert!(<Take as Comparable<Field<Option<u32>>>>::retain(
            &mut clear, None,
        ));
        assert_eq!(clear, Some(None));
        assert!(<Take as Comparable<Field<Option<u32>>>>::retain(
            &mut clear,
            Some(&Some(7)),
        ));
        assert!(!<Take as Comparable<Field<Option<u32>>>>::retain(
            &mut clear,
            Some(&None),
        ));
        assert_eq!(clear, None);
    }

    #[test]
    fn flatten_keeps_values_while_reporting_changes() {
        let mut value = None::<u32>;
        assert!(!<Always as Comparable<Field<Option<u32>>>>::retain(
            &mut value,
            Some(&None),
        ));
        assert_eq!(value, None);
        assert!(<Always as Comparable<Field<Option<u32>>>>::retain(
            &mut value,
            Some(&Some(7)),
        ));
    }

    struct Inner<'s> {
        optional: &'s str,
        flattened: u32,
    }

    struct InnerPatch<'s> {
        optional: Option<&'s str>,
        flattened: u32,
    }

    impl<'s> Schema<Inner<'s>> for Inner<'s> {
        type Layout = (Field<&'s str>, Field<u32>);

        fn full_view<'a>(subject: &'a Inner<'s>) -> <Self::Layout as Layout>::Ref<'a>
        where
            Self::Layout: 'a,
        {
            (Some(&subject.optional), Some(&subject.flattened))
        }
    }

    impl<'s> PartialOptionized<Inner<'s>> for InnerPatch<'s> {
        fn optionize(subject: Inner<'s>) -> Self {
            Self {
                optional: Some(subject.optional),
                flattened: subject.flattened,
            }
        }

        fn patch(self, subject: &mut Inner<'s>) {
            if let Some(value) = self.optional {
                subject.optional = value;
            }
            subject.flattened = self.flattened;
        }

        fn merge(&mut self, other: Self) {
            if other.optional.is_some() {
                self.optional = other.optional;
            }
            self.flattened = other.flattened;
        }

        fn view<'a>(&'a self) -> <<Inner<'s> as Schema<Inner<'s>>>::Layout as Layout>::Ref<'a>
        where
            <Inner<'s> as Schema<Inner<'s>>>::Layout: 'a,
        {
            (self.optional.as_ref(), Some(&self.flattened))
        }
    }

    impl<'s> FieldAccess<Inner<'s>> for InnerPatch<'s> {
        type Access = (Take, Always);

        fn fields_mut<'a>(
            &'a mut self,
        ) -> <Self::Access as Access<<Inner<'s> as Schema<Inner<'s>>>::Layout>>::Mut<'a>
        where
            Self::Access: 'a,
            <Inner<'s> as Schema<Inner<'s>>>::Layout: 'a,
        {
            (&mut self.optional, &mut self.flattened)
        }
    }

    #[test]
    fn nested_borrowed_values_and_flattened_fields_can_clear_the_parent() {
        extern crate alloc;
        let owned = alloc::string::String::from("borrowed");
        let baseline = Inner {
            optional: owned.as_str(),
            flattened: 7,
        };
        let mut patch = Some(InnerPatch {
            optional: Some(owned.as_str()),
            flattened: 7,
        });
        assert!(!<TakeNested<InnerPatch<'_>> as Comparable<
            Nested<Inner<'_>>,
        >>::retain(
            &mut patch, Some(NestedRef::Full(&baseline))
        ));
        assert!(patch.is_none());
    }

    #[test]
    fn nested_partial_baselines_preserve_unknown_values() {
        let baseline = InnerPatch {
            optional: None,
            flattened: 7,
        };
        let mut patch = Some(InnerPatch {
            optional: Some("new"),
            flattened: 7,
        });
        assert!(<TakeNested<InnerPatch<'_>> as Comparable<
            Nested<Inner<'_>>,
        >>::retain(
            &mut patch, Some(NestedRef::Partial(baseline.view())),
        ));
        assert_eq!(patch.as_ref().unwrap().optional, Some("new"));

        patch.as_mut().unwrap().optional = None;
        assert!(!<TakeNested<InnerPatch<'_>> as Comparable<
            Nested<Inner<'_>>,
        >>::retain(
            &mut patch, Some(NestedRef::Partial(baseline.view())),
        ));
        assert!(patch.is_none());
    }

    #[test]
    fn nested_unknown_baselines_keep_presence_and_flattened_updates() {
        let mut optional = Some(InnerPatch {
            optional: None,
            flattened: 0,
        });
        assert!(<TakeNested<InnerPatch<'_>> as Comparable<
            Nested<Inner<'_>>,
        >>::retain(&mut optional, None));
        assert!(optional.is_some());

        let mut flattened = InnerPatch {
            optional: None,
            flattened: 0,
        };
        assert!(<AlwaysNested<InnerPatch<'_>> as Comparable<
            Nested<Inner<'_>>,
        >>::retain(&mut flattened, None));
    }

    #[test]
    fn public_retain_compares_a_partial_baseline() {
        let baseline = InnerPatch {
            optional: Some("same"),
            flattened: 7,
        };
        let mut patch = InnerPatch {
            optional: Some("same"),
            flattened: 7,
        };
        assert!(!patch.retain(&baseline));
        assert_eq!(patch.optional, None);
        assert_eq!(patch.flattened, 7);
    }

    #[test]
    fn generic_retain_accepts_borrowed_values_without_lifetime_bounds() {
        fn trim<P, B, S, D>(patch: &mut P, baseline: &B) -> bool
        where
            P: Retain<S, D>,
            B: PartialOptionized<S, D>,
            D: Schema<S>,
        {
            patch.retain(baseline)
        }

        extern crate alloc;
        let owned = alloc::string::String::from("borrowed");
        let baseline = InnerPatch {
            optional: Some(owned.as_str()),
            flattened: 7,
        };
        let mut patch = InnerPatch {
            optional: Some(owned.as_str()),
            flattened: 7,
        };

        assert!(!trim(&mut patch, &baseline));
        assert_eq!(patch.optional, None);
        assert_eq!(baseline.optional, Some(owned.as_str()));
    }

    struct Empty;
    struct EmptyPatch;

    impl Schema<Empty> for Empty {
        type Layout = ();

        fn full_view<'a>(_: &'a Empty) -> <Self::Layout as Layout>::Ref<'a>
        where
            Self::Layout: 'a,
        {
        }
    }

    impl PartialOptionized<Empty> for EmptyPatch {
        fn optionize(_: Empty) -> Self {
            Self
        }

        fn patch(self, _: &mut Empty) {}

        fn merge(&mut self, _: Self) {}

        fn view<'a>(&'a self) -> <<Empty as Schema<Empty>>::Layout as Layout>::Ref<'a>
        where
            <Empty as Schema<Empty>>::Layout: 'a,
        {
        }
    }

    impl FieldAccess<Empty> for EmptyPatch {
        type Access = ();

        fn fields_mut<'a>(
            &'a mut self,
        ) -> <Self::Access as Access<<Empty as Schema<Empty>>::Layout>>::Mut<'a>
        where
            Self::Access: 'a,
            <Empty as Schema<Empty>>::Layout: 'a,
        {
        }
    }

    #[test]
    fn empty_nested_patch_preserves_presence_only_when_unknown() {
        let mut patch = Some(EmptyPatch);
        assert!(<TakeNested<EmptyPatch> as Comparable<Nested<Empty>>>::retain(&mut patch, None,));
        assert!(patch.is_some());
        assert!(
            !<TakeNested<EmptyPatch> as Comparable<Nested<Empty>>>::retain(
                &mut patch,
                Some(NestedRef::Partial(())),
            )
        );
        assert!(patch.is_none());
    }
}
