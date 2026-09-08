use optionize::{Optionizable, Optionized, PartialOptionized, optionized};
use optionize_test_proto as proto;

#[optionized]
#[optionize(subject = proto::Subject, partial(upgradable))]
struct PartialSubjectPatch {
    enabled: Option<bool>,
    count: Option<u32>,
    #[optionize(skip(upgrade = "String::from(\"default\")"))]
    label: String,
    #[optionize(skip)]
    optional: Option<u32>,
}

#[test]
fn reverse_skips_declare_unmanaged_subject_fields_and_upgrade_defaults() {
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
#[optionize(subject = proto::Subject)]
#[derive(Debug)]
struct SubjectPatch {
    enabled: Option<bool>,
    count: Option<u32>,
    label: Option<String>,
    optional: Option<Option<u32>>,
}

#[optionized]
#[optionize(subject = "proto::Subject")]
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
#[optionize(subject = proto::GenericSubject::<Value>)]
#[derive(Debug)]
struct GenericPatch<Value> {
    value: Option<Value>,
}

type Maybe<Value> = Option<Value>;

#[optionized]
#[optionize(subject = "proto::GenericSubject<Value>")]
#[derive(Debug)]
struct AliasedPatch<Value> {
    value: Maybe<Value>,
}

#[optionized]
#[optionize(subject = proto::BorrowedSubject::<'v, Value, N>)]
struct BorrowedPatch<'v, Value, const N: usize> {
    values: Option<&'v [Value; N]>,
}

#[optionized]
#[optionize(subject = proto::TupleSubject)]
#[derive(Debug)]
struct TuplePatch(Option<u32>, Option<String>);

#[optionized]
#[optionize(subject = proto::UnitSubject)]
#[derive(Debug)]
struct UnitPatch;

#[optionized]
#[optionize(subject = proto::NestedSubject)]
#[derive(Debug)]
struct NestedPatch {
    value: Option<u32>,
}

#[optionized]
#[optionize(subject = proto::ContainerSubject)]
#[derive(Debug)]
struct ContainerPatch {
    #[optionize(nest = proto::NestedSubject)]
    nested: Option<NestedPatch>,
    #[optionize(flatten, nest = proto::NestedSubject)]
    flattened: NestedPatch,
}

#[test]
fn foreign_subject_supports_patch_merge_downgrade_and_upgrade() {
    let mut subject = proto::Subject {
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
fn reverse_field_names_and_flattening_map_to_subject_fields() {
    let mut subject = proto::Subject {
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
fn reverse_validation_keeps_required_and_nullable_fields_distinct() {
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
fn generic_subject_paths_infer_parameters_and_preserve_owned_values() {
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
fn option_aliases_work_in_reverse_declarations() {
    let mut subject = proto::GenericSubject { value: 1_u32 };
    subject.load(AliasedPatch { value: Some(0) });
    assert_eq!(subject.value, 0);

    let patch: AliasedPatch<u32> = subject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, 0);
}

#[test]
fn reverse_paths_preserve_lifetime_and_const_arguments() {
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
fn reverse_tuple_and_unit_paths_support_upgrading() {
    let patch = TuplePatch(Some(0), Some(String::new()));
    patch.validate().unwrap();
    assert_eq!(
        patch.upgrade().unwrap(),
        proto::TupleSubject(0, String::new())
    );

    let patch: UnitPatch = proto::UnitSubject.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap(), proto::UnitSubject);
}

#[test]
fn reverse_nested_subject_types_preserve_nested_patch_types() {
    let mut subject = proto::ContainerSubject {
        nested: proto::NestedSubject { value: 1 },
        flattened: proto::NestedSubject { value: 2 },
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
