#![deny(unused_lifetimes)]

use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, Schema, optionized};
use optionize_test_models as models;

#[optionized]
#[derive(Debug)]
struct Values {
    enabled: bool,
    count: u32,
    label: String,
    items: Vec<u32>,
    optional: Option<u32>,
}

#[derive(Debug, PartialEq)]
struct NoClone(u32);

#[derive(Debug)]
struct NoEq(u32);

#[optionized]
#[derive(Debug)]
struct Uncomparable {
    value: NoEq,
}

#[optionized]
#[derive(Debug)]
struct Generic<Value> {
    value: Value,
}

// Neither nested subject implements PartialEq.
#[optionized]
#[derive(Debug)]
struct Inner {
    value: u32,
    label: String,
}

#[optionized]
#[derive(Debug)]
struct Outer {
    #[optionize(nest = InnerOptional)]
    nested: Inner,
    #[optionize(flatten, nest = InnerOptional)]
    flattened: Inner,
}

#[optionized]
#[derive(Debug)]
struct Flattened {
    #[optionize(flatten)]
    value: u32,
    #[optionize(flatten)]
    optional: Option<u32>,
}

#[optionized]
#[optionize(partial(marked))]
#[derive(Debug)]
struct Marked<Value> {
    value: u32,
    #[optionize(skip)]
    ignored: Value,
}

#[optionized]
#[optionize(partial)]
#[derive(Debug)]
struct Tuple(#[optionize(skip)] NoEq, u32, Option<u32>);

#[optionized]
#[derive(Debug)]
struct Unit;

#[optionized]
#[optionize(subject = models::Subject)]
#[derive(Debug)]
struct ForeignSubjectPatch {
    #[optionize(name = "enabled")]
    active: Option<bool>,
    count: Option<u32>,
    label: Option<String>,
    optional: Option<Option<u32>>,
}

#[optionized]
#[optionize(subject = models::NestedSubject)]
#[derive(Debug)]
struct ForeignNestedPatch {
    value: Option<u32>,
}

#[optionized]
#[optionize(subject = models::ContainerSubject)]
#[derive(Debug)]
struct ForeignContainerPatch {
    #[optionize(nest = models::NestedSubject)]
    nested: Option<ForeignNestedPatch>,
    #[optionize(flatten, nest = models::NestedSubject)]
    flattened: ForeignNestedPatch,
}

#[optionized]
#[derive(Debug)]
struct Borrowed<'s> {
    value: &'s str,
}

#[optionized]
#[derive(Debug)]
struct BorrowedContainer<'s> {
    #[optionize(nest = BorrowedOptional::<'s>)]
    nested: Borrowed<'s>,
}

#[optionized]
#[derive(Debug)]
struct UncomparableContainer {
    #[optionize(nest = UncomparableOptional)]
    nested: Uncomparable,
    #[optionize(flatten, nest = UncomparableOptional)]
    flattened: Uncomparable,
}

// This independent representation knows only two renamed fields. It supplies
// the shared view without implementing PartialOptionized or Retain.
struct RenamedBaseline {
    active: Option<bool>,
    title: Option<String>,
}

impl Schema<Values, ValuesOptional> for RenamedBaseline {
    type View<'v> = <ValuesOptional as Schema<Values>>::View<'v>;
    // The generated view is named through its associated type.
    #[allow(clippy::field_reassign_with_default)]
    fn view<'s>(&'s self) -> <ValuesOptional as Schema<Values>>::View<'s>
    where
        Values: 's,
    {
        let mut view: <ValuesOptional as Schema<Values>>::View<'s> = Default::default();
        view.v_enabled = self.active.as_ref();
        view.v_label = self.title.as_ref();
        view
    }
}

fn retain_generic<Subject, Descriptor, Patch, Baseline>(
    patch: &mut Patch,
    baseline: &Baseline,
) -> bool
where
    Descriptor: optionize::Schema<Subject>,
    Patch: Retain<Subject, Descriptor>,
    Baseline: Schema<Subject, Descriptor>,
{
    patch.retain(baseline)
}

fn empty_inner() -> InnerOptional {
    InnerOptional {
        value: None,
        label: None,
    }
}

#[test]
fn retain_removes_equal_false_zero_empty_and_clear_values() {
    let baseline = Values {
        enabled: false,
        count: 0,
        label: String::new(),
        items: Vec::new(),
        optional: None,
    };
    let mut patch = ValuesOptional {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::new()),
        items: Some(Vec::new()),
        optional: Some(None),
    };

    assert!(!patch.retain(&baseline));
    assert!(patch.enabled.is_none());
    assert!(patch.count.is_none());
    assert!(patch.label.is_none());
    assert!(patch.items.is_none());
    assert!(patch.optional.is_none());
}

#[test]
fn retain_preserves_changed_false_zero_empty_and_clear_values() {
    let baseline = Values {
        enabled: true,
        count: 1,
        label: String::from("old"),
        items: vec![1],
        optional: Some(7),
    };
    let mut patch = ValuesOptional {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::new()),
        items: Some(Vec::new()),
        optional: Some(None),
    };

    assert!(patch.retain(&baseline));
    assert_eq!(patch.enabled, Some(false));
    assert_eq!(patch.count, Some(0));
    assert_eq!(patch.label.as_deref(), Some(""));
    assert_eq!(patch.items.as_deref(), Some([].as_slice()));
    assert_eq!(patch.optional, Some(None));
}

#[test]
fn retain_preserves_unknown_fields_and_explicit_clears() {
    let baseline = ValuesOptional {
        enabled: Some(false),
        count: None,
        label: Some(String::new()),
        items: None,
        optional: None,
    };
    let mut patch = ValuesOptional {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::new()),
        items: None,
        optional: Some(None),
    };

    assert!(patch.retain(&baseline));
    assert_eq!(patch.enabled, None);
    assert_eq!(patch.count, Some(0));
    assert_eq!(patch.label, None);
    assert_eq!(patch.items, None);
    assert_eq!(patch.optional, Some(None));

    let known_clear = ValuesOptional {
        enabled: None,
        count: Some(0),
        label: None,
        items: None,
        optional: Some(None),
    };
    assert!(!patch.retain(&known_clear));
    assert_eq!(patch.count, None);
    assert_eq!(patch.optional, None);
}

#[test]
fn partial_operations_accept_concrete_and_generic_non_equality_types() {
    let patch = Uncomparable { value: NoEq(1) }.downgrade();
    patch.validate().unwrap();
    let mut subject = patch.upgrade().unwrap();
    subject.load(UncomparableOptional {
        value: Some(NoEq(2)),
    });
    assert_eq!(subject.value.0, 2);

    let patch = Generic { value: NoEq(3) }.downgrade();
    patch.validate().unwrap();
    let mut subject = patch.upgrade().unwrap();
    subject.load(GenericOptional {
        value: Some(NoEq(4)),
    });
    assert_eq!(subject.value.0, 4);
}

#[test]
fn retain_accepts_full_and_partial_baselines_without_clone() {
    let baseline = Generic { value: NoClone(5) };
    let mut patch = GenericOptional {
        value: Some(NoClone(5)),
    };
    assert!(!retain_generic(&mut patch, &baseline));
    assert_eq!(patch.value, None);
    assert_eq!(baseline.value, NoClone(5));

    let baseline = GenericOptional { value: None };
    let mut patch = GenericOptional {
        value: Some(NoClone(0)),
    };
    assert!(retain_generic(&mut patch, &baseline));
    assert_eq!(patch.value, Some(NoClone(0)));
}

#[test]
fn schema_includes_optional_and_flattened_descendants() {
    #[optionized]
    struct Root {
        #[optionize(nest = OuterOptional)]
        outer: Outer,
    }

    let full = Root {
        outer: Outer {
            nested: Inner {
                value: 1,
                label: String::from("nested"),
            },
            flattened: Inner {
                value: 2,
                label: String::from("flattened"),
            },
        },
    };
    let outer = full.view().v_outer.unwrap();
    let nested = outer.v_nested.unwrap();
    let flattened = outer.v_flattened.unwrap();
    assert_eq!(nested.v_value, Some(&1));
    assert_eq!(nested.v_label.map(String::as_str), Some("nested"));
    assert_eq!(flattened.v_value, Some(&2));
    assert_eq!(flattened.v_label.map(String::as_str), Some("flattened"));

    let mut partial = RootOptional {
        outer: Some(OuterOptional {
            nested: None,
            flattened: InnerOptional {
                value: Some(3),
                label: None,
            },
        }),
    };
    let outer = partial.view().v_outer.unwrap();
    assert!(outer.v_nested.is_none());
    let flattened = outer.v_flattened.unwrap();
    assert_eq!(flattened.v_value, Some(&3));
    assert_eq!(flattened.v_label, None);

    partial.outer.as_mut().unwrap().nested = Some(empty_inner());
    let nested = partial.view().v_outer.unwrap().v_nested.unwrap();
    assert_eq!(nested.v_value, None);
    assert_eq!(nested.v_label, None);

    partial.outer = None;
    assert!(partial.view().v_outer.is_none());
}

#[test]
fn retain_recurses_without_whole_subject_equality() {
    let baseline = Outer {
        nested: Inner {
            value: 1,
            label: String::from("same"),
        },
        flattened: Inner {
            value: 2,
            label: String::from("same"),
        },
    };
    let mut patch = OuterOptional {
        nested: Some(InnerOptional {
            value: Some(1),
            label: None,
        }),
        flattened: InnerOptional {
            value: Some(2),
            label: Some(String::from("same")),
        },
    };

    assert!(!patch.retain(&baseline));
    assert!(patch.nested.is_none());
    assert_eq!(patch.flattened.value, None);
    assert_eq!(patch.flattened.label, None);
}

#[test]
fn retain_preserves_empty_nested_patches_against_unknown_baselines() {
    let baseline = OuterOptional {
        nested: None,
        flattened: empty_inner(),
    };
    let mut patch = OuterOptional {
        nested: Some(empty_inner()),
        flattened: empty_inner(),
    };

    assert!(patch.retain(&baseline));
    let nested = patch.nested.as_ref().unwrap();
    assert_eq!(nested.value, None);
    assert_eq!(nested.label, None);

    let known = OuterOptional {
        nested: Some(empty_inner()),
        flattened: empty_inner(),
    };
    assert!(!patch.retain(&known));
    assert!(patch.nested.is_none());
}

#[test]
fn retain_removes_only_known_equal_fields_in_nested_patches() {
    let baseline = OuterOptional {
        nested: Some(InnerOptional {
            value: Some(1),
            label: None,
        }),
        flattened: InnerOptional {
            value: None,
            label: Some(String::from("same")),
        },
    };
    let mut patch = OuterOptional {
        nested: Some(InnerOptional {
            value: Some(1),
            label: Some(String::new()),
        }),
        flattened: InnerOptional {
            value: Some(0),
            label: Some(String::from("same")),
        },
    };

    assert!(patch.retain(&baseline));
    let nested = patch.nested.as_ref().unwrap();
    assert_eq!(nested.value, None);
    assert_eq!(nested.label.as_deref(), Some(""));
    assert_eq!(patch.flattened.value, Some(0));
    assert_eq!(patch.flattened.label, None);
}

#[test]
fn retain_preserves_flattened_values_and_reports_changes() {
    let baseline = Flattened {
        value: 0,
        optional: None,
    };
    let mut equal = FlattenedOptional {
        value: 0,
        optional: None,
    };
    assert!(!equal.retain(&baseline));
    assert_eq!(equal.value, 0);
    assert_eq!(equal.optional, None);

    let mut changed = FlattenedOptional {
        value: 1,
        optional: None,
    };
    assert!(changed.retain(&baseline));
    assert_eq!(changed.value, 1);
}

#[test]
fn retain_preserves_flattened_fields_missing_from_independent_baselines() {
    #[optionized]
    struct Config {
        #[optionize(flatten)]
        count: u32,
        #[optionize(flatten, nest = InnerOptional)]
        nested: Inner,
    }

    struct Baseline {
        count: Option<u32>,
        nested: Option<Inner>,
    }

    impl Schema<Config, ConfigOptional> for Baseline {
        type View<'v> = <ConfigOptional as Schema<Config>>::View<'v>;

        #[allow(clippy::field_reassign_with_default)]
        fn view<'s>(&'s self) -> Self::View<'s>
        where
            Config: 's,
        {
            let mut view: Self::View<'s> = Default::default();
            view.v_count = self.count.as_ref();
            view.v_nested = self.nested.as_ref().map(Schema::view);
            view
        }
    }

    // Unknown flattened values must count as changes even when they look empty.
    let mut patch = ConfigOptional {
        count: 0,
        nested: empty_inner(),
    };
    assert!(patch.retain(&Baseline {
        count: Some(0),
        nested: None,
    }));
    assert_eq!(patch.count, 0);
    assert_eq!(patch.nested.value, None);
    assert_eq!(patch.nested.label, None);

    patch.nested.value = Some(0);
    patch.nested.label = Some(String::new());
    assert!(patch.retain(&Baseline {
        count: None,
        nested: Some(Inner {
            value: 0,
            label: String::new(),
        }),
    }));
    assert_eq!(patch.count, 0);
    assert_eq!(patch.nested.value, None);
    assert_eq!(patch.nested.label, None);
}

#[test]
fn retain_uses_partial_equality_for_non_reflexive_values() {
    #[optionized]
    struct Measurement {
        value: f64,
        #[optionize(flatten)]
        flattened: f64,
    }

    let baseline = Measurement {
        value: f64::NAN,
        flattened: 0.0,
    };
    let mut patch = MeasurementOptional {
        value: Some(f64::NAN),
        flattened: -0.0,
    };
    assert!(patch.retain(&baseline));
    assert!(patch.value.unwrap().is_nan());
    assert!(patch.flattened.is_sign_negative());

    patch.value = None;
    assert!(!patch.retain(&baseline));
    patch.flattened = f64::NAN;
    assert!(patch.retain(&baseline));
    assert!(patch.flattened.is_nan());
}

#[test]
fn retain_ignores_skipped_fields_markers_and_empty_shapes() {
    let baseline = Marked {
        value: 1,
        ignored: NoEq(2),
    };
    let mut patch = MarkedOptional::<NoEq> {
        value: Some(1),
        _marker: PhantomData,
    };
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, None);
    assert_eq!(baseline.ignored.0, 2);

    let baseline = Tuple(NoEq(3), 0, None);
    let mut patch = TupleOptional(Some(0), Some(None));
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.0, None);
    assert_eq!(patch.1, None);
    assert_eq!(baseline.0.0, 3);

    let mut patch = UnitOptional;
    assert!(!patch.retain(&Unit));
    assert!(!patch.retain(&UnitOptional));
}

#[test]
fn retain_accepts_external_subjects_without_wrappers() {
    let baseline = models::Subject {
        enabled: false,
        count: 0,
        label: String::new(),
        optional: None,
    };
    let mut patch = ForeignSubjectPatch {
        active: Some(false),
        count: Some(0),
        label: Some(String::new()),
        optional: Some(None),
    };

    assert!(!retain_generic(&mut patch, &baseline));
    assert_eq!(patch.active, None);
    assert_eq!(patch.count, None);
    assert_eq!(patch.label, None);
    assert_eq!(patch.optional, None);

    let baseline = ForeignSubjectPatch {
        active: None,
        count: None,
        label: None,
        optional: None,
    };
    patch.optional = Some(None);
    assert!(patch.retain(&baseline));
    assert_eq!(patch.optional, Some(None));
}

#[test]
fn retain_infers_local_descriptors_for_external_nested_subjects() {
    let baseline = models::ContainerSubject {
        nested: models::NestedSubject { value: 1 },
        flattened: models::NestedSubject { value: 2 },
    };
    let mut patch = ForeignContainerPatch {
        nested: Some(ForeignNestedPatch { value: Some(1) }),
        flattened: ForeignNestedPatch { value: Some(2) },
    };

    assert!(!patch.retain(&baseline));
    assert!(patch.nested.is_none());
    assert_eq!(patch.flattened.value, None);
}

#[test]
fn schema_shares_borrowed_views_between_subject_and_object() {
    use optionize::Schema;

    let text = String::from("borrowed through the object schema");
    let baseline = Borrowed { value: &text };
    let view: <BorrowedOptional<'_> as Schema<Borrowed<'_>>>::View<'_> = baseline.view();

    assert_eq!(view.v_value.copied(), Some(text.as_str()));
    let mut patch = BorrowedOptional { value: Some(&text) };
    assert!(!patch.retain_view(view));
    assert!(patch.value.is_none());
}

#[test]
fn partial_operations_accept_nested_objects_without_subject_schemas() {
    struct Child {
        value: u32,
    }

    struct ChildPatch {
        value: Option<u32>,
    }

    impl optionize::Schema<Child> for ChildPatch {
        type View<'v> = Option<&'v u32>;

        fn view<'s>(&'s self) -> <Self as optionize::Schema<Child>>::View<'s>
        where
            Child: 's,
            Self: 's,
        {
            self.value.as_ref()
        }
    }

    impl PartialOptionized<Child> for ChildPatch {
        fn optionize(subject: Child) -> Self {
            Self {
                value: Some(subject.value),
            }
        }

        fn patch(self, subject: &mut Child) {
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

    impl Retain<Child, ChildPatch> for ChildPatch {
        fn retain_view<'v>(
            &mut self,
            baseline: <Self as optionize::Schema<Child>>::View<'v>,
        ) -> bool
        where
            Child: 'v,
            Self: 'v,
        {
            if self.value.as_ref() == baseline {
                self.value = None;
            }
            self.value.is_some()
        }
    }

    // Child deliberately has no Schema implementation.
    #[optionized]
    #[optionize(partial)]
    struct Parent {
        #[optionize(nest = ChildPatch)]
        child: Child,
    }

    let mut patch = ParentOptional {
        child: Some(ChildPatch { value: Some(1) }),
    };
    assert_eq!(patch.view().v_child.unwrap().copied(), Some(1));
    patch.merge(ParentOptional {
        child: Some(ChildPatch { value: Some(2) }),
    });

    let mut subject = Parent {
        child: Child { value: 0 },
    };
    patch.patch(&mut subject);
    assert_eq!(subject.child.value, 2);
    let mut patch = subject.downgrade();
    assert_eq!(patch.view().v_child.unwrap().copied(), Some(2));
    let baseline = ParentOptional {
        child: Some(ChildPatch { value: Some(2) }),
    };
    assert!(!patch.retain(&baseline));
    assert!(patch.child.is_none());
}

#[test]
fn retain_accepts_independent_baselines_with_renamed_and_omitted_fields() {
    let baseline = RenamedBaseline {
        active: Some(false),
        title: Some(String::from("same")),
    };
    let mut patch = ValuesOptional {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::from("same")),
        items: Some(Vec::new()),
        optional: Some(None),
    };

    assert!(retain_generic(&mut patch, &baseline));
    assert_eq!(patch.enabled, None);
    assert_eq!(patch.label, None);
    assert_eq!(patch.count, Some(0));
    assert_eq!(patch.items.as_deref(), Some([].as_slice()));
    assert_eq!(patch.optional, Some(None));
    assert_eq!(baseline.active, Some(false));
    assert_eq!(baseline.title.as_deref(), Some("same"));
}

#[test]
fn retain_is_idempotent_and_preserves_sequential_application() {
    fn subject(count: u32, optional: Option<u32>) -> Values {
        Values {
            enabled: true,
            count,
            label: String::from("untouched"),
            items: vec![7],
            optional,
        }
    }

    fn patch(count: Option<u32>, optional: Option<Option<u32>>) -> ValuesOptional {
        ValuesOptional {
            enabled: None,
            count,
            label: None,
            items: None,
            optional,
        }
    }

    // None, clear, and set are distinct, including when a partial baseline
    // contains no knowledge of the complete subject's existing value.
    let optional_updates = [None, Some(None), Some(Some(0)), Some(Some(1))];
    for initial_count in [0, 1] {
        for initial_optional in [None, Some(0), Some(1)] {
            for baseline_count in [None, Some(0), Some(1)] {
                for baseline_optional in optional_updates {
                    for next_count in [None, Some(0), Some(1)] {
                        for next_optional in optional_updates {
                            let baseline = patch(baseline_count, baseline_optional);
                            let mut reduced = patch(next_count, next_optional);
                            let remains = reduced.retain(&baseline);
                            let retained_fields = (reduced.count, reduced.optional);
                            assert_eq!(reduced.retain(&baseline), remains);
                            assert_eq!((reduced.count, reduced.optional), retained_fields);

                            let mut expected = subject(initial_count, initial_optional);
                            patch(baseline_count, baseline_optional).patch(&mut expected);
                            patch(next_count, next_optional).patch(&mut expected);

                            let mut actual = subject(initial_count, initial_optional);
                            baseline.patch(&mut actual);
                            reduced.patch(&mut actual);
                            assert_eq!(actual.count, expected.count);
                            assert_eq!(actual.optional, expected.optional);
                            assert_eq!(actual.enabled, expected.enabled);
                            assert_eq!(actual.label, expected.label);
                            assert_eq!(actual.items, expected.items);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn retain_preserves_nested_application_when_redundant_patches_are_omitted() {
    #[optionized]
    #[derive(Debug, PartialEq)]
    struct Leaf {
        value: u32,
        #[optionize(flatten)]
        enabled: bool,
    }

    #[optionized]
    #[derive(Debug, PartialEq)]
    struct Branch {
        #[optionize(nest = LeafOptional)]
        nested: Leaf,
        #[optionize(flatten, nest = LeafOptional)]
        flattened: Leaf,
    }

    fn subject(value: u32, enabled: bool) -> Branch {
        Branch {
            nested: Leaf { value, enabled },
            flattened: Leaf { value, enabled },
        }
    }

    fn patch(
        nested: Option<(Option<u32>, bool)>,
        flattened: (Option<u32>, bool),
    ) -> BranchOptional {
        BranchOptional {
            nested: nested.map(|(value, enabled)| LeafOptional { value, enabled }),
            flattened: LeafOptional {
                value: flattened.0,
                enabled: flattened.1,
            },
        }
    }

    let fields = [
        (None, false),
        (None, true),
        (Some(0), false),
        (Some(0), true),
        (Some(1), false),
        (Some(1), true),
    ];
    let nested = || core::iter::once(None).chain(fields.into_iter().map(Some));

    for initial_value in [0, 1] {
        for initial_enabled in [false, true] {
            for baseline_nested in nested() {
                for baseline_flattened in fields {
                    for next_nested in nested() {
                        for next_flattened in fields {
                            let baseline = patch(baseline_nested, baseline_flattened);
                            let mut retained = patch(next_nested, next_flattened);
                            let remains = retained.retain(&baseline);
                            let fields = (
                                retained
                                    .nested
                                    .as_ref()
                                    .map(|leaf| (leaf.value, leaf.enabled)),
                                (retained.flattened.value, retained.flattened.enabled),
                            );
                            assert_eq!(retained.retain(&baseline), remains);
                            assert_eq!(retained, patch(fields.0, fields.1));

                            let mut expected = subject(initial_value, initial_enabled);
                            patch(baseline_nested, baseline_flattened).patch(&mut expected);
                            patch(next_nested, next_flattened).patch(&mut expected);

                            let mut actual = subject(initial_value, initial_enabled);
                            baseline.patch(&mut actual);
                            if remains {
                                retained.patch(&mut actual);
                            }
                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn retain_accepts_nested_borrowed_fields_without_static_lifetimes() {
    let text = String::from("nested local borrow");
    let baseline = BorrowedContainer {
        nested: Borrowed { value: &text },
    };
    let mut patch = BorrowedContainerOptional {
        nested: Some(BorrowedOptional { value: Some(&text) }),
    };
    assert!(!patch.retain(&baseline));
    assert!(patch.nested.is_none());

    let other = String::from("different nested borrow");
    patch.nested = Some(BorrowedOptional {
        value: Some(&other),
    });
    assert!(patch.retain(&baseline));
    assert_eq!(
        patch.nested.as_ref().unwrap().value,
        Some("different nested borrow")
    );
}

#[test]
fn partial_operations_accept_nested_non_equality_types() {
    let subject = UncomparableContainer {
        nested: Uncomparable { value: NoEq(1) },
        flattened: Uncomparable { value: NoEq(2) },
    };
    let patch = subject.downgrade();
    patch.validate().unwrap();
    let mut subject = patch.upgrade().unwrap();
    subject.load(UncomparableContainerOptional {
        nested: Some(UncomparableOptional {
            value: Some(NoEq(3)),
        }),
        flattened: UncomparableOptional {
            value: Some(NoEq(4)),
        },
    });
    assert_eq!(subject.nested.value.0, 3);
    assert_eq!(subject.flattened.value.0, 4);
}

macro_rules! many_fields {
    ($($field:ident),+ $(,)?) => {
        #[optionized]
        struct ManyFields {
            $($field: u32,)+
        }

        #[test]
        fn retain_accepts_sixty_four_fields_with_the_default_recursion_limit() {
            let baseline = ManyFields { $($field: 0,)+ };
            let mut equal = ManyFieldsOptional { $($field: Some(0),)+ };
            assert!(!equal.retain(&baseline));
            $(assert!(equal.$field.is_none());)+

            let mut changed = ManyFieldsOptional { $($field: Some(0),)+ };
            changed.f00 = Some(1);
            assert!(changed.retain(&baseline));
            assert_eq!(changed.f00, Some(1));
            assert_eq!(changed.f01, None);
            assert_eq!(changed.f63, None);
            assert!(changed.retain(&baseline));
        }
    };
}

many_fields! {
    f00, f01, f02, f03, f04, f05, f06, f07,
    f08, f09, f10, f11, f12, f13, f14, f15,
    f16, f17, f18, f19, f20, f21, f22, f23,
    f24, f25, f26, f27, f28, f29, f30, f31,
    f32, f33, f34, f35, f36, f37, f38, f39,
    f40, f41, f42, f43, f44, f45, f46, f47,
    f48, f49, f50, f51, f52, f53, f54, f55,
    f56, f57, f58, f59, f60, f61, f62, f63,
}
