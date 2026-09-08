use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};
use optionize_test_proto as proto;

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
struct Owned {
    value: NoClone,
}

#[optionized]
#[derive(Debug)]
struct Uncomparable {
    value: NoEq,
}

#[optionized]
#[derive(Debug)]
struct Generic<T> {
    value: T,
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
struct Marked<T> {
    value: u32,
    #[optionize(skip)]
    ignored: T,
}

#[optionized]
#[optionize(partial)]
#[derive(Debug)]
struct Tuple(#[optionize(skip)] NoEq, u32, Option<u32>);

#[optionized]
#[derive(Debug)]
struct Unit;

#[optionized]
#[optionize(object = proto::Shared)]
#[derive(Debug)]
struct ForeignObjectSubject {
    value: u32,
}

#[optionized]
#[optionize(subject = proto::Subject)]
#[derive(Debug)]
struct ForeignSubjectPatch {
    #[optionize(name = "enabled")]
    active: Option<bool>,
    count: Option<u32>,
    label: Option<String>,
    optional: Option<Option<u32>>,
}

#[optionized]
#[optionize(subject = proto::NestedSubject)]
#[derive(Debug)]
struct ForeignNestedPatch {
    value: Option<u32>,
}

#[optionized]
#[optionize(subject = proto::ContainerSubject)]
#[derive(Debug)]
struct ForeignContainerPatch {
    #[optionize(nest = proto::NestedSubject)]
    nested: Option<ForeignNestedPatch>,
    #[optionize(flatten, nest = proto::NestedSubject)]
    flattened: ForeignNestedPatch,
}

#[optionized]
#[derive(Debug)]
struct Borrowed<'a> {
    value: &'a str,
}

#[optionized]
#[derive(Debug)]
struct BorrowedContainer<'a> {
    #[optionize(nest = BorrowedOptional::<'a>)]
    nested: Borrowed<'a>,
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
// the shared view, but does not implement Retain itself.
struct RenamedBaseline {
    active: Option<bool>,
    title: Option<String>,
}

impl PartialOptionized<Values> for RenamedBaseline {
    fn optionize(subject: Values) -> Self {
        Self {
            active: Some(subject.enabled),
            title: Some(subject.label),
        }
    }

    fn patch(self, subject: &mut Values) {
        if let Some(active) = self.active {
            subject.enabled = active;
        }
        if let Some(title) = self.title {
            subject.label = title;
        }
    }

    fn merge(&mut self, other: Self) {
        if other.active.is_some() {
            self.active = other.active;
        }
        if other.title.is_some() {
            self.title = other.title;
        }
    }

    // The generated view is named through its associated type.
    #[allow(clippy::field_reassign_with_default)]
    fn view<'a>(&'a self) -> <Values as optionize::Schema<Values>>::View<'a>
    where
        Values: 'a,
    {
        let mut view: <Values as optionize::Schema<Values>>::View<'a> = Default::default();
        view.v_enabled = self.active.as_ref();
        view.v_label = self.title.as_ref();
        view
    }
}

fn retain_generic<S, D, P, B>(patch: &mut P, baseline: &B) -> bool
where
    D: optionize::Schema<S>,
    P: Retain<S, D>,
    B: PartialOptionized<S, D>,
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
fn equal_false_zero_empty_and_clear_values_are_removed() {
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
fn changed_false_zero_empty_and_clear_values_are_preserved() {
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
fn partial_baseline_preserves_unknown_fields_and_explicit_clears() {
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
fn retaining_only_borrows_non_clone_values() {
    let baseline = Owned { value: NoClone(2) };
    let mut equal = OwnedOptional {
        value: Some(NoClone(2)),
    };
    assert!(!equal.retain(&baseline));
    assert_eq!(equal.value, None);
    assert_eq!(baseline.value, NoClone(2));

    let mut changed = OwnedOptional {
        value: Some(NoClone(3)),
    };
    assert!(changed.retain(&baseline));
    assert_eq!(changed.value, Some(NoClone(3)));
    assert_eq!(baseline.value, NoClone(2));
}

#[test]
fn concrete_and_generic_non_equality_types_keep_normal_operations() {
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
fn generic_retain_supports_full_and_partial_baselines() {
    let baseline = Generic { value: NoClone(5) };
    let mut patch = GenericOptional {
        value: Some(NoClone(5)),
    };
    assert!(!retain_generic(&mut patch, &baseline));

    let baseline = GenericOptional { value: None };
    let mut patch = GenericOptional {
        value: Some(NoClone(0)),
    };
    assert!(retain_generic(&mut patch, &baseline));
    assert_eq!(patch.value, Some(NoClone(0)));
}

#[test]
fn nested_views_include_optional_and_flattened_descendants() {
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
fn nested_full_baselines_recurse_without_whole_subject_equality() {
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
fn nested_unknown_baseline_preserves_even_an_explicit_empty_patch() {
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
fn nested_partial_baselines_only_remove_known_equal_fields() {
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
fn flattened_values_stay_present_but_report_whether_they_change() {
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
fn skipped_fields_markers_tuples_and_units_do_not_create_changes() {
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
fn external_objects_support_full_and_partial_baselines() {
    let baseline = ForeignObjectSubject { value: 0 };
    let mut patch = proto::Shared { value: Some(0) };
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, None);

    let baseline = proto::Shared { value: None };
    let mut patch = proto::Shared { value: Some(0) };
    assert!(patch.retain(&baseline));
    assert_eq!(patch.value, Some(0));
}

#[test]
fn external_subjects_supply_identity_baselines_without_local_wrappers() {
    let baseline = proto::Subject {
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
fn reverse_nested_mappings_infer_local_descriptors() {
    let baseline = proto::ContainerSubject {
        nested: proto::NestedSubject { value: 1 },
        flattened: proto::NestedSubject { value: 2 },
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
fn borrowed_fields_do_not_require_static_lifetimes() {
    let text = String::from("borrowed locally");
    let baseline = Borrowed { value: &text };
    let mut patch = BorrowedOptional { value: Some(&text) };

    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, None);
    assert_eq!(baseline.value, "borrowed locally");
}

#[test]
fn independent_baseline_types_can_rename_and_omit_fields() {
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
fn nested_borrowed_fields_retain_without_static_lifetimes() {
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
fn nested_non_equality_fields_keep_loading_and_upgrading() {
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
        fn sixty_four_fields_work_with_the_default_recursion_limit() {
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
