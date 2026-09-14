use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};
use optionize_test_models as models;

#[optionized]
#[optionize(subject = models::Subject, partial(upgradable))]
struct PartialSubjectPatch {
    enabled: Option<bool>,
    count: Option<u32>,
    #[optionize(skip, default = "|_| String::from(\"default\")")]
    label: String,
    #[optionize(skip)]
    optional: Option<u32>,
}

#[test]
fn subject_skips_restore_defaults_when_upgrading() {
    let patch = PartialSubjectPatch {
        enabled: Some(true),
        count: Some(7),
    };
    let full = patch.upgrade().unwrap();
    assert!(full.enabled);
    assert_eq!(full.count, 7);
    assert_eq!(full.label, "default");
    assert_eq!(full.optional, None);
    let patch: PartialSubjectPatch = full.downgrade();
    assert_eq!(patch.count, Some(7));
}

#[optionized]
#[optionize(subject = models::Subject)]
#[derive(Debug)]
struct SubjectPatch {
    enabled: Option<bool>,
    count: Option<u32>,
    label: Option<String>,
    optional: Option<Option<u32>>,
}

#[optionized]
#[optionize(subject = "models::Subject")]
#[derive(Debug)]
struct RenamedSubjectPatch {
    #[optionize(name = "enabled")]
    active: Option<bool>,
    count: Option<u32>,
    #[optionize(name = "label", flatten)]
    text: String,
    optional: Option<Option<u32>>,
}

#[optionized]
#[optionize(subject = models::GenericSubject::<Value>)]
#[derive(Debug)]
struct GenericPatch<Value> {
    value: Option<Value>,
}

type Maybe<Value> = Option<Value>;

#[optionized]
#[optionize(subject = "models::GenericSubject<Value>")]
#[derive(Debug)]
struct AliasedPatch<Value> {
    value: Maybe<Value>,
}

#[optionized]
#[optionize(subject = models::BorrowedSubject::<'v, Value, N>)]
struct BorrowedPatch<'v, Value, const N: usize> {
    values: Option<&'v [Value; N]>,
}

#[optionized]
#[optionize(subject = models::TupleSubject)]
#[derive(Debug)]
struct TuplePatch(Option<u32>, Option<String>);

#[optionized]
#[optionize(subject = models::UnitSubject)]
#[derive(Debug)]
struct UnitPatch;

#[optionized]
#[optionize(subject = models::NestedSubject)]
#[derive(Debug)]
struct NestedPatch {
    value: Option<u32>,
}

#[optionized]
#[optionize(subject = models::ContainerSubject)]
#[derive(Debug)]
struct ContainerPatch {
    #[optionize(nest = models::NestedSubject)]
    nested: Option<NestedPatch>,
    #[optionize(flatten, nest = models::NestedSubject)]
    flattened: NestedPatch,
}

#[test]
fn subject_mapping_supports_partial_operations_and_upgrading() {
    let mut subject = models::Subject {
        enabled: true,
        count: 5,
        label: String::from("old"),
        optional: Some(7),
    };
    let mut patch = SubjectPatch {
        enabled: Some(false),
        count: None,
        label: None,
        optional: Some(None),
    };
    patch.merge(SubjectPatch {
        enabled: None,
        count: Some(0),
        label: Some(String::new()),
        optional: None,
    });
    subject.load(patch);
    assert!(!subject.enabled);
    assert_eq!(subject.count, 0);
    assert!(subject.label.is_empty());
    assert_eq!(subject.optional, None);

    let patch: SubjectPatch = subject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.enabled, Some(false));
    assert_eq!(patch.optional, Some(None));
    let subject = patch.upgrade().unwrap();
    assert!(!subject.enabled);
    assert_eq!(subject.optional, None);
}

#[test]
fn subject_mapping_renames_and_flattens_fields() {
    let mut subject = models::Subject {
        enabled: true,
        count: 5,
        label: String::from("old"),
        optional: Some(7),
    };
    subject.load(RenamedSubjectPatch {
        active: Some(false),
        count: None,
        text: String::new(),
        optional: None,
    });
    assert!(!subject.enabled);
    assert_eq!(subject.count, 5);
    assert!(subject.label.is_empty());
    assert_eq!(subject.optional, Some(7));

    let patch: RenamedSubjectPatch = subject.downgrade();
    assert_eq!(patch.active, Some(false));
    assert!(patch.text.is_empty());
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.optional, Some(7));
}

#[test]
fn validate_requires_explicit_nullable_subject_fields() {
    let patch = SubjectPatch {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::new()),
        optional: None,
    };
    assert!(patch.validate().is_err());
    assert!(patch.upgrade().is_err());

    SubjectPatch {
        enabled: Some(false),
        count: Some(0),
        label: Some(String::new()),
        optional: Some(None),
    }
    .validate()
    .unwrap();
}

#[test]
fn subject_paths_infer_generic_parameters_from_owned_fields() {
    let patch = GenericPatch {
        value: Some(String::from("owned")),
    };
    patch.validate().unwrap();
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.value, "owned");

    let patch: GenericPatch<String> = subject.downgrade();
    assert_eq!(patch.value.as_deref(), Some("owned"));
}

#[test]
fn subject_mapping_accepts_option_aliases() {
    let mut subject = models::GenericSubject { value: 1_u32 };
    subject.load(AliasedPatch { value: Some(0) });
    assert_eq!(subject.value, 0);

    let patch: AliasedPatch<u32> = subject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, 0);
}

#[test]
fn subject_paths_preserve_lifetime_and_const_arguments() {
    let values = [String::from("borrowed"), String::from("locally")];
    let patch = BorrowedPatch {
        values: Some(&values),
    };
    patch.validate().unwrap();
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.values, &values);

    let patch: BorrowedPatch<'_, String, 2> = subject.downgrade();
    assert_eq!(patch.values, Some(&values));
}

#[test]
fn subject_paths_support_tuple_and_unit_structs() {
    let patch = TuplePatch(Some(0), Some(String::new()));
    patch.validate().unwrap();
    assert_eq!(
        patch.upgrade().unwrap(),
        models::TupleSubject(0, String::new())
    );

    let patch: UnitPatch = models::UnitSubject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap(), models::UnitSubject);
}

#[test]
fn subject_skips_preserve_tuple_indices_and_compact_object_fields() {
    use optionize::{Retain, Schema};

    #[optionized]
    #[optionize(subject = models::TupleSubject, partial(upgradable))]
    struct PartialTuplePatch(#[optionize(skip, default = |_| 7)] u32, Option<String>);

    let mut subject = models::TupleSubject(9, String::from("old"));
    let mut patch = PartialTuplePatch(Some(String::from("new")));
    let view = patch.view();
    assert!(view.v_0.is_none());
    assert_eq!(view.v_1.map(String::as_str), Some("new"));
    assert!(patch.retain(&subject));
    subject.load(patch);
    assert_eq!(subject.0, 9);
    assert_eq!(subject.1, "new");

    let patch: PartialTuplePatch = subject.downgrade();
    assert_eq!(patch.0.as_deref(), Some("new"));
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.0, 7);
    assert_eq!(subject.1, "new");
}

#[test]
fn subject_mapping_preserves_nested_patch_types() {
    let mut subject = models::ContainerSubject {
        nested: models::NestedSubject { value: 1 },
        flattened: models::NestedSubject { value: 2 },
    };
    subject.load(ContainerPatch {
        nested: Some(NestedPatch { value: Some(0) }),
        flattened: NestedPatch { value: None },
    });
    assert_eq!(subject.nested.value, 0);
    assert_eq!(subject.flattened.value, 2);

    let patch: ContainerPatch = subject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.nested.as_ref().unwrap().value, Some(0));
    assert_eq!(patch.flattened.value, Some(2));
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.nested.value, 0);
    assert_eq!(subject.flattened.value, 2);
}

#[test]
fn subject_markers_preserve_generic_parameters_when_fields_are_skipped() {
    #[optionized]
    #[optionize(subject = models::GenericSubject::<Value>, partial(marked, upgradable))]
    struct MarkedPatch<Value> {
        value: Option<Value>,
    }

    #[optionized]
    #[optionize(subject = models::GenericSubject::<Value>, partial(marked, upgradable))]
    struct SkippedPatch<Value: Default> {
        #[optionize(skip)]
        value: Value,
    }

    let baseline = models::GenericSubject { value: 8_u32 };
    let mut patch = MarkedPatch {
        value: Some(8),
        _marker: PhantomData,
    };
    assert!(!patch.retain(&baseline));

    let patch: MarkedPatch<u32> = baseline.downgrade();
    assert_eq!(patch.upgrade().unwrap().value, 8);

    let baseline = models::GenericSubject { value: 9_u32 };
    let mut patch: SkippedPatch<u32> = models::GenericSubject { value: 1_u32 }.downgrade();
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.upgrade().unwrap().value, 0);
}

#[test]
fn subject_markers_support_tuple_and_unit_objects() {
    #[optionized]
    #[optionize(subject = models::TupleSubject, partial(marked, upgradable))]
    struct MarkedTuplePatch(#[optionize(skip, default = |_| 7)] u32, Option<String>);

    #[optionized]
    #[optionize(subject = models::UnitSubject, partial(marked, upgradable))]
    struct MarkedUnitPatch;

    let baseline = models::TupleSubject(9, String::from("same"));
    let mut patch = MarkedTuplePatch(Some(String::from("same")), PhantomData);
    assert!(!patch.retain(&baseline));
    assert!(patch.0.is_none());

    let patch: MarkedTuplePatch = baseline.downgrade();
    assert_eq!(
        patch.upgrade().unwrap(),
        models::TupleSubject(7, String::from("same"))
    );

    let mut patch: MarkedUnitPatch = models::UnitSubject.downgrade();
    assert!(!patch.retain(&models::UnitSubject));
    assert_eq!(patch.upgrade().unwrap(), models::UnitSubject);
}

#[test]
fn subject_arguments_survive_expression_macro_forwarding() {
    macro_rules! patches {
        ($($name:ident => $subject:expr),+ $(,)?) => {
            $(
                #[optionized]
                #[optionize(subject = $subject)]
                struct $name<Value> {
                    value: Option<Value>,
                }
            )+
        };
    }

    patches! {
        GenericSubject => "models::{}<Value>",
        BarePatch => models::GenericSubject::<Value>,
    }

    let template = GenericSubject {
        value: Some(String::from("forwarded template")),
    };
    assert_eq!(template.upgrade().unwrap().value, "forwarded template");

    let bare = BarePatch {
        value: Some(String::from("forwarded path")),
    };
    assert_eq!(bare.upgrade().unwrap().value, "forwarded path");
}
