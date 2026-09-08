use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, Retain, Schema, optionized};
use optionize_test_proto as proto;

#[optionized]
#[optionize(object = proto::Single)]
#[derive(Debug, PartialEq)]
struct SingleTarget {
    enabled: bool,
    label: String,
}

#[optionized]
#[optionize(object = proto::Shared)]
#[derive(Debug, PartialEq)]
struct RequiredTarget {
    value: u32,
}

#[optionized]
#[optionize(object = "proto::Shared")]
#[derive(Debug, PartialEq)]
struct NullableTarget {
    #[optionize(flatten)]
    value: Option<u32>,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct RequiredContainer {
    #[optionize(nest = "proto::Shared")]
    nested: RequiredTarget,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct NullableContainer {
    #[optionize(nest = "proto::Shared")]
    nested: NullableTarget,
}

#[optionized]
#[optionize(object = proto::Generic::<T>)]
#[derive(Debug, PartialEq)]
struct GenericTarget<T> {
    value: T,
}

#[optionized]
#[optionize(partial(marked, upgradable))]
#[derive(Debug, PartialEq)]
struct GenericNested<T, P> {
    #[optionize(nest = "P")]
    nested: T,
    #[optionize(skip)]
    marker: PhantomData<P>,
}

#[optionized]
#[optionize(object = "proto::ErasedGeneric", partial(upgradable))]
#[derive(Debug, PartialEq)]
struct TargetOnlyGeneric<T> {
    value: u32,
    #[optionize(skip)]
    marker: PhantomData<T>,
}

fn convert<P, S>(patch: P) -> Result<S, P::Errors>
where
    P: Optionized<S>,
{
    patch.upgrade()
}

#[test]
fn single_concrete_target_infers_validation_and_upgrade() {
    let patch = proto::Single {
        enabled: Some(false),
        label: Some(String::new()),
    };

    patch.validate().unwrap();
    let full = patch.upgrade().unwrap();
    assert!(!full.enabled);
    assert!(full.label.is_empty());

    let missing = proto::Single::default();
    assert!(missing.validate().is_err());
    assert!(missing.upgrade().is_err());
}

#[test]
fn external_object_still_supports_downgrade_and_load() {
    let mut full = SingleTarget {
        enabled: true,
        label: "old".into(),
    };
    full.load(proto::Single {
        enabled: None,
        label: Some("new".into()),
    });

    let patch: proto::Single = full.downgrade();
    assert_eq!(patch.enabled, Some(true));
    assert_eq!(patch.label.as_deref(), Some("new"));
}

#[test]
fn shared_object_validation_selects_the_target() {
    let patch = proto::Shared::default();

    assert!(Optionized::<RequiredTarget>::validate(&patch).is_err());
    Optionized::<NullableTarget>::validate(&patch).unwrap();

    let nullable = Optionized::<NullableTarget>::upgrade(patch).unwrap();
    assert_eq!(nullable.value, None);
    assert!(Optionized::<RequiredTarget>::upgrade(proto::Shared::default()).is_err());
}

#[test]
fn shared_object_upgrade_infers_from_the_return_type() {
    let required: RequiredTarget = proto::Shared { value: Some(7) }.upgrade().unwrap();
    let nullable: NullableTarget = proto::Shared::default().upgrade().unwrap();

    assert_eq!(required.value, 7);
    assert_eq!(nullable.value, None);
}

#[test]
fn shared_object_retain_infers_the_mapping_from_the_baseline() {
    fn trim<S, D, P, B>(patch: &mut P, baseline: &B) -> bool
    where
        D: Schema<S>,
        P: Retain<S, D>,
        B: Schema<S, D>,
    {
        patch.retain(baseline)
    }

    let mut patch = proto::Shared { value: Some(7) };
    assert!(!patch.retain(&RequiredTarget { value: 7 }));
    assert_eq!(patch.value, None);

    patch.value = Some(7);
    assert!(!trim(&mut patch, &NullableTarget { value: Some(7) }));
    // This mapping flattens the field, so retaining keeps its stored value.
    assert_eq!(patch.value, Some(7));

    let baseline = proto::Shared { value: Some(7) };
    assert!(!Retain::<RequiredTarget>::retain(&mut patch, &baseline));
    assert_eq!(patch.value, None);
}

#[test]
fn generic_helpers_can_name_associated_errors() {
    let required: RequiredTarget = convert(proto::Shared { value: Some(9) }).unwrap();
    let nullable: NullableTarget = convert(proto::Shared::default()).unwrap();

    assert_eq!(required.value, 9);
    assert_eq!(nullable.value, None);
    assert!(convert::<_, RequiredTarget>(proto::Shared::default()).is_err());
}

#[test]
fn nested_validation_and_upgrade_use_the_declared_target() {
    let missing_required = RequiredContainerOptional {
        nested: Some(proto::Shared::default()),
    };
    assert!(missing_required.validate().is_err());
    assert!(missing_required.upgrade().is_err());

    let nullable = NullableContainerOptional {
        nested: Some(proto::Shared::default()),
    };
    nullable.validate().unwrap();
    assert_eq!(nullable.upgrade().unwrap().nested.value, None);

    let required = RequiredContainerOptional {
        nested: Some(proto::Shared { value: Some(11) }),
    };
    required.validate().unwrap();
    assert_eq!(required.upgrade().unwrap().nested.value, 11);
}

#[test]
fn generic_target_infers_type_parameters_from_the_object() {
    let patch = proto::Generic {
        value: Some(String::from("owned")),
    };

    patch.validate().unwrap();
    let full = patch.upgrade().unwrap();
    assert_eq!(full.value, "owned");
}

#[test]
fn generic_nested_bounds_are_added_to_each_generated_impl() {
    let full = GenericNested::<RequiredTarget, proto::Shared> {
        nested: RequiredTarget { value: 12 },
        marker: PhantomData,
    };

    let patch = full.downgrade();
    patch.validate().unwrap();
    let full = patch.upgrade().unwrap();
    assert_eq!(full.nested.value, 12);
}

#[test]
fn target_only_type_parameters_can_be_specified_explicitly() {
    let patch = proto::ErasedGeneric { value: Some(13) };

    Optionized::<TargetOnlyGeneric<u8>>::validate(&patch).unwrap();
    let full: TargetOnlyGeneric<u8> = patch.upgrade().unwrap();
    assert_eq!(full.value, 13);
}
