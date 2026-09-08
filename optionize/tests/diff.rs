use core::marker::PhantomData;

use optionize::{Diff, Optionizable, Optionized, PartialOptionized, optionized};
use optionize_test_proto as proto;

#[optionized]
#[optionize(diff)]
#[derive(Debug)]
struct Values {
    enabled: bool,
    count: u64,
    text: String,
    items: Vec<u32>,
    optional: Option<String>,
}

#[derive(Debug, PartialEq)]
struct NoClone(u32);

#[derive(Debug)]
struct NoEq(u32);

#[optionized]
#[optionize(diff)]
#[derive(Debug)]
struct OwnedValue {
    value: NoClone,
}

#[optionized]
#[optionize(diff, partial)]
#[derive(Debug)]
struct Exceptional {
    managed: u32,
    #[optionize(flatten)]
    flattened: NoEq,
    #[optionize(skip)]
    skipped: NoEq,
}

#[optionized]
#[derive(Debug)]
struct WithoutDiff {
    value: NoEq,
}

#[optionized]
#[optionize(diff)]
#[derive(Debug)]
struct Generic<T> {
    value: T,
}

#[optionized]
#[optionize(diff)]
#[derive(Debug, PartialEq)]
struct Inner {
    count: u32,
    label: String,
}

#[optionized]
#[optionize(diff)]
#[derive(Debug)]
struct Outer {
    #[optionize(nest = "InnerOptional")]
    nested: Inner,
    #[optionize(flatten, nest = "InnerOptional")]
    flattened: Inner,
}

#[optionized]
#[optionize(diff, partial)]
#[derive(Debug, PartialEq)]
struct PartialInner {
    managed: u32,
    #[optionize(skip)]
    skipped: u32,
}

#[optionized]
#[optionize(diff, partial)]
#[derive(Debug)]
struct PartialOuter {
    #[optionize(nest = "PartialInnerOptional")]
    nested: PartialInner,
    #[optionize(flatten, nest = "ExceptionalOptional")]
    flattened: Exceptional,
}

#[optionized]
#[optionize(diff, partial)]
#[derive(Debug)]
struct Tuple(
    #[optionize(skip)] NoEq,
    NoClone,
    #[optionize(flatten)] NoEq,
    Option<u32>,
);

#[optionized]
#[optionize(diff, partial(marked))]
#[derive(Debug)]
struct Marked<T> {
    value: u32,
    #[optionize(skip)]
    marker: PhantomData<T>,
}

#[optionized]
#[optionize(diff)]
#[derive(Debug)]
struct Unit;

#[optionized]
#[optionize(diff, partial(marked))]
#[derive(Debug)]
struct MarkedUnit;

#[optionized]
#[optionize(diff, partial)]
#[derive(Debug)]
struct OnlySkipped {
    #[optionize(skip)]
    value: NoEq,
}

#[optionized]
#[optionize(diff, object = "proto::Shared")]
#[derive(Debug)]
struct ForeignTarget {
    value: u32,
}

#[optionized]
#[optionize(diff, object = "proto::Generic<T>")]
#[derive(Debug)]
struct ForeignGeneric<T> {
    value: T,
}

#[test]
fn changed_fields_preserve_false_zero_empty_values_and_clearing() {
    let mut base = Values {
        enabled: true,
        count: 7,
        text: "before".into(),
        items: vec![1, 2],
        optional: Some("present".into()),
    };
    let next = Values {
        enabled: false,
        count: 0,
        text: String::new(),
        items: Vec::new(),
        optional: None,
    };

    let patch = ValuesOptional::diff(&base, next);
    assert_eq!(patch.enabled, Some(false));
    assert_eq!(patch.count, Some(0));
    assert_eq!(patch.text.as_deref(), Some(""));
    assert_eq!(patch.items.as_deref(), Some([].as_slice()));
    assert_eq!(patch.optional, Some(None));

    base.load(patch);
    assert!(!base.enabled);
    assert_eq!(base.count, 0);
    assert!(base.text.is_empty());
    assert!(base.items.is_empty());
    assert_eq!(base.optional, None);
}

#[test]
fn equal_fields_are_omitted_and_optional_values_can_be_set() {
    let base = Values {
        enabled: false,
        count: 0,
        text: String::new(),
        items: Vec::new(),
        optional: None,
    };
    let equal = Values {
        enabled: false,
        count: 0,
        text: String::new(),
        items: Vec::new(),
        optional: None,
    };

    let patch = ValuesOptional::diff(&base, equal);
    assert!(patch.enabled.is_none());
    assert!(patch.count.is_none());
    assert!(patch.text.is_none());
    assert!(patch.items.is_none());
    assert!(patch.optional.is_none());

    let set = Values {
        enabled: false,
        count: 0,
        text: String::new(),
        items: Vec::new(),
        optional: Some(String::new()),
    };
    let patch = ValuesOptional::diff(&base, set);
    assert_eq!(patch.optional, Some(Some(String::new())));
}

#[test]
fn diff_moves_fields_without_clone_or_whole_subject_equality() {
    let mut base = OwnedValue { value: NoClone(1) };
    let next = OwnedValue { value: NoClone(2) };

    let patch = OwnedValueOptional::diff(&base, next);
    assert_eq!(patch.value, Some(NoClone(2)));
    patch.patch(&mut base);
    assert_eq!(base.value, NoClone(2));
}

#[test]
fn flattened_and_skipped_fields_do_not_require_equality() {
    let mut base = Exceptional {
        managed: 1,
        flattened: NoEq(2),
        skipped: NoEq(3),
    };
    let next = Exceptional {
        managed: 1,
        flattened: NoEq(4),
        skipped: NoEq(5),
    };

    let patch = ExceptionalOptional::diff(&base, next);
    assert_eq!(patch.managed, None);
    assert_eq!(patch.flattened.0, 4);
    base.load(patch);
    assert_eq!(base.managed, 1);
    assert_eq!(base.flattened.0, 4);
    assert_eq!(base.skipped.0, 3);
}

#[test]
fn types_without_diff_keep_supporting_non_equality_fields() {
    let full = WithoutDiff { value: NoEq(6) };
    let patch = full.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value.0, 6);
}

#[test]
fn generic_diff_bounds_do_not_restrict_loading_or_upgrading() {
    let full = Generic { value: NoEq(7) };
    let patch = full.downgrade();
    patch.validate().unwrap();
    let mut full = patch.upgrade().unwrap();
    full.load(GenericOptional {
        value: Some(NoEq(8)),
    });
    assert_eq!(full.value.0, 8);

    let base = Generic { value: NoClone(9) };
    let next = Generic { value: NoClone(10) };
    let patch = GenericOptional::diff(&base, next);
    assert_eq!(patch.value, Some(NoClone(10)));
}

#[test]
fn nested_diffs_recurse_and_equal_nested_values_are_omitted() {
    let mut base = Outer {
        nested: Inner {
            count: 1,
            label: "old".into(),
        },
        flattened: Inner {
            count: 2,
            label: "same".into(),
        },
    };
    let next = Outer {
        nested: Inner {
            count: 1,
            label: "new".into(),
        },
        flattened: Inner {
            count: 0,
            label: "same".into(),
        },
    };

    let patch = OuterOptional::diff(&base, next);
    let nested = patch.nested.as_ref().unwrap();
    assert_eq!(nested.count, None);
    assert_eq!(nested.label.as_deref(), Some("new"));
    assert_eq!(patch.flattened.count, Some(0));
    assert_eq!(patch.flattened.label, None);
    base.load(patch);
    assert_eq!(base.nested.count, 1);
    assert_eq!(base.nested.label, "new");
    assert_eq!(base.flattened.count, 0);

    let equal = Outer {
        nested: Inner {
            count: 1,
            label: "new".into(),
        },
        flattened: Inner {
            count: 0,
            label: "same".into(),
        },
    };
    let patch = OuterOptional::diff(&base, equal);
    assert!(patch.nested.is_none());
    assert_eq!(patch.flattened.count, None);
    assert_eq!(patch.flattened.label, None);
}

#[test]
fn nested_partial_diffs_do_not_require_upgrade_support() {
    let mut base = PartialOuter {
        nested: PartialInner {
            managed: 1,
            skipped: 2,
        },
        flattened: Exceptional {
            managed: 3,
            flattened: NoEq(4),
            skipped: NoEq(5),
        },
    };
    let next = PartialOuter {
        nested: PartialInner {
            managed: 0,
            skipped: 9,
        },
        flattened: Exceptional {
            managed: 3,
            flattened: NoEq(6),
            skipped: NoEq(9),
        },
    };

    let patch = PartialOuterOptional::diff(&base, next);
    assert_eq!(patch.nested.as_ref().unwrap().managed, Some(0));
    assert_eq!(patch.flattened.managed, None);
    base.load(patch);
    assert_eq!(base.nested.managed, 0);
    assert_eq!(base.nested.skipped, 2);
    assert_eq!(base.flattened.flattened.0, 6);
    assert_eq!(base.flattened.skipped.0, 5);
}

#[test]
fn tuple_diffs_use_field_indices_after_skipping() {
    let mut base = Tuple(NoEq(1), NoClone(2), NoEq(3), Some(4));
    let next = Tuple(NoEq(9), NoClone(5), NoEq(6), None);

    let patch = TupleOptional::diff(&base, next);
    assert_eq!(patch.0, Some(NoClone(5)));
    assert_eq!(patch.1.0, 6);
    assert_eq!(patch.2, Some(None));
    base.load(patch);
    assert_eq!(base.0.0, 1);
    assert_eq!(base.1, NoClone(5));
    assert_eq!(base.2.0, 6);
    assert_eq!(base.3, None);
}

#[test]
fn markers_units_and_all_skipped_structs_can_be_diffed() {
    let mut base = Marked::<NoEq> {
        value: 1,
        marker: PhantomData,
    };
    let next = Marked::<NoEq> {
        value: 2,
        marker: PhantomData,
    };
    let patch = MarkedOptional::diff(&base, next);
    assert_eq!(patch.value, Some(2));
    base.load(patch);
    assert_eq!(base.value, 2);

    let mut unit = Unit;
    UnitOptional::diff(&unit, Unit).patch(&mut unit);
    let mut marked_unit = MarkedUnit;
    MarkedUnitOptional::diff(&marked_unit, MarkedUnit).patch(&mut marked_unit);

    let mut skipped = OnlySkipped { value: NoEq(3) };
    let next = OnlySkipped { value: NoEq(4) };
    OnlySkippedOptional::diff(&skipped, next).patch(&mut skipped);
    assert_eq!(skipped.value.0, 3);
}

#[test]
fn external_objects_can_implement_diff_for_local_subjects() {
    let mut base = ForeignTarget { value: 5 };
    let next = ForeignTarget { value: 0 };

    let patch = proto::Shared::diff(&base, next);
    assert_eq!(patch.value, Some(0));
    base.load(patch);
    assert_eq!(base.value, 0);
}

#[test]
fn external_generic_object_paths_support_owned_diffs() {
    let mut base = ForeignGeneric { value: NoClone(6) };
    let next = ForeignGeneric { value: NoClone(7) };

    let patch = proto::Generic::diff(&base, next);
    assert_eq!(patch.value, Some(NoClone(7)));
    base.load(patch);
    assert_eq!(base.value, NoClone(7));
}
