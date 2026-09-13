use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, Schema, optionized};
use optionize_test_models as models;

#[optionized]
#[optionize(object = models::Single)]
#[derive(Debug, PartialEq)]
struct SingleSubject {
    enabled: bool,
    label: String,
}

#[optionized]
#[optionize(object = models::Shared)]
#[derive(Debug, PartialEq)]
struct RequiredSubject {
    value: u32,
}

#[optionized]
#[optionize(object = "models::Shared")]
#[derive(Debug, PartialEq)]
struct NullableSubject {
    #[optionize(flatten)]
    value: Option<u32>,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct RequiredContainer {
    #[optionize(nest = "models::Shared")]
    nested: RequiredSubject,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct NullableContainer {
    #[optionize(nest = "models::Shared")]
    nested: NullableSubject,
}

#[optionized]
#[optionize(object = models::Generic::<Value>)]
#[derive(Debug, PartialEq)]
struct GenericSubject<Value> {
    value: Value,
}

#[optionized]
#[optionize(partial(marked, upgradable))]
#[derive(Debug, PartialEq)]
struct GenericNested<Subject, Patch> {
    #[optionize(nest = "Patch")]
    nested: Subject,
    #[optionize(skip)]
    marker: PhantomData<Patch>,
}

#[optionized]
#[optionize(object = "models::ErasedGeneric", partial(upgradable))]
#[derive(Debug, PartialEq)]
struct SubjectOnlyGeneric<Value> {
    value: u32,
    #[optionize(skip)]
    marker: PhantomData<Value>,
}

fn convert<Patch, Subject>(patch: Patch) -> Result<Subject, Patch::Errors>
where
    Patch: Optionized<Subject>,
{
    patch.upgrade()
}

#[test]
fn external_objects_infer_the_only_subject() {
    let mut patch = models::Single {
        enabled: Some(false),
        label: Some(String::new()),
    };

    // An unknown baseline also infers the mapping from the external Object alone.
    assert!(patch.retain(&models::Single::default()));
    patch.validate().unwrap();
    let full = patch.upgrade().unwrap();
    assert!(!full.enabled);
    assert!(full.label.is_empty());

    let missing = models::Single::default();
    assert!(missing.validate().is_err());
    assert!(missing.upgrade().is_err());
}

#[test]
fn partial_operations_support_external_objects() {
    let mut full = SingleSubject {
        enabled: true,
        label: "old".into(),
    };
    full.load(models::Single {
        enabled: None,
        label: Some("new".into()),
    });

    let patch: models::Single = full.downgrade();
    assert_eq!(patch.enabled, Some(true));
    assert_eq!(patch.label.as_deref(), Some("new"));
}

#[test]
fn optionized_selects_the_explicit_subject_for_shared_objects() {
    let patch = models::Shared::default();

    assert!(Optionized::<RequiredSubject>::validate(&patch).is_err());
    Optionized::<NullableSubject>::validate(&patch).unwrap();

    let nullable = Optionized::<NullableSubject>::upgrade(patch).unwrap();
    assert_eq!(nullable.value, None);
    assert!(Optionized::<RequiredSubject>::upgrade(models::Shared::default()).is_err());
}

#[test]
fn upgrade_infers_shared_object_mappings_from_the_return_type() {
    let required: RequiredSubject = models::Shared { value: Some(7) }.upgrade().unwrap();
    let nullable: NullableSubject = models::Shared::default().upgrade().unwrap();

    assert_eq!(required.value, 7);
    assert_eq!(nullable.value, None);
}

#[test]
fn retain_infers_shared_object_mappings_from_the_baseline() {
    fn trim<Subject, Descriptor, Patch, Baseline>(patch: &mut Patch, baseline: &Baseline) -> bool
    where
        Descriptor: Schema<Subject>,
        Patch: Retain<Subject, Descriptor>,
        Baseline: Schema<Subject, Descriptor>,
    {
        patch.retain(baseline)
    }

    let mut patch = models::Shared { value: Some(7) };
    assert!(!patch.retain(&RequiredSubject { value: 7 }));
    assert_eq!(patch.value, None);

    patch.value = Some(7);
    assert!(!trim(&mut patch, &NullableSubject { value: Some(7) }));
    // This mapping flattens the field, so retaining keeps its stored value.
    assert_eq!(patch.value, Some(7));

    let baseline = models::Shared { value: Some(7) };
    assert!(!Retain::<RequiredSubject>::retain(&mut patch, &baseline));
    assert_eq!(patch.value, None);
}

#[test]
fn upgrade_exposes_associated_errors_to_generic_helpers() {
    let required: RequiredSubject = convert(models::Shared { value: Some(9) }).unwrap();
    let nullable: NullableSubject = convert(models::Shared::default()).unwrap();

    assert_eq!(required.value, 9);
    assert_eq!(nullable.value, None);
    assert!(convert::<_, RequiredSubject>(models::Shared::default()).is_err());
}

#[test]
fn optionized_uses_the_declared_nested_subject_for_shared_objects() {
    let missing_required = RequiredContainerOptional {
        nested: Some(models::Shared::default()),
    };
    assert!(missing_required.validate().is_err());
    assert!(missing_required.upgrade().is_err());

    let nullable = NullableContainerOptional {
        nested: Some(models::Shared::default()),
    };
    nullable.validate().unwrap();
    assert_eq!(nullable.upgrade().unwrap().nested.value, None);

    let required = RequiredContainerOptional {
        nested: Some(models::Shared { value: Some(11) }),
    };
    required.validate().unwrap();
    assert_eq!(required.upgrade().unwrap().nested.value, 11);
}

#[test]
fn optionized_infers_subject_parameters_from_external_objects() {
    let patch = models::Generic {
        value: Some(String::from("owned")),
    };

    patch.validate().unwrap();
    let full = patch.upgrade().unwrap();
    assert_eq!(full.value, "owned");
}

#[test]
fn nested_operations_propagate_generic_trait_bounds() {
    let full = GenericNested::<RequiredSubject, models::Shared> {
        nested: RequiredSubject { value: 12 },
        marker: PhantomData,
    };

    let mut patch = full.downgrade();
    patch.merge(GenericNestedOptional {
        nested: Some(models::Shared { value: Some(13) }),
        _marker: PhantomData,
    });

    let mut baseline = GenericNested::<RequiredSubject, models::Shared> {
        nested: RequiredSubject { value: 12 },
        marker: PhantomData,
    };
    assert!(patch.retain(&baseline));
    baseline.load(patch);
    assert_eq!(baseline.nested.value, 13);

    let patch = baseline.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().nested.value, 13);
}

#[test]
fn optionized_accepts_explicit_subject_only_parameters() {
    let patch = models::ErasedGeneric { value: Some(13) };

    Optionized::<SubjectOnlyGeneric<u8>>::validate(&patch).unwrap();
    let full: SubjectOnlyGeneric<u8> = patch.upgrade().unwrap();
    assert_eq!(full.value, 13);
}
