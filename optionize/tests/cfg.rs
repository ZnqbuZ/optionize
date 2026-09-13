use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};
use optionize_test_models as models;

#[optionized]
#[optionize(attrs(derive(Default)))]
struct Config {
    #[cfg(any())]
    #[optionize(unknown)]
    unavailable: MissingType,
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
    nested: OtherMissingType,
    #[cfg_attr(all(), optionize(name = "active", attrs()))]
    enabled: bool,
}

#[test]
fn cfg_filters_fields_before_parsing_attributes_and_generating_operations() {
    let mut subject = Config { enabled: true };
    let mut patch = ConfigOptional::default();
    assert!(patch.validate().is_err());
    patch.merge(ConfigOptional { active: Some(true) });
    assert!(!patch.retain(&subject));
    patch.active = Some(false);
    subject.load(patch);
    assert!(!subject.enabled);
    assert!(!subject.downgrade().upgrade().unwrap().enabled);
}

#[optionized]
#[optionize(partial(upgradable))]
struct Tuple(
    #[cfg(any())] MissingType,
    bool,
    #[optionize(skip)] u32,
    #[cfg_attr(all(), cfg(any()))] OtherMissingType,
    String,
);

#[test]
fn cfg_and_skip_preserve_effective_tuple_indices() {
    let mut subject = Tuple(true, 7, String::from("old"));
    let patch = TupleOptional(Some(false), Some(String::from("new")));
    subject.load(patch);
    assert_eq!(
        (subject.0, subject.1, subject.2.as_str()),
        (false, 7, "new")
    );
    let mut patch = subject.downgrade();
    assert!(!patch.retain(&Tuple(false, 9, String::from("new"))));
    patch.0 = Some(true);
    patch.1 = Some(String::from("restored"));
    let subject = patch.upgrade().unwrap();
    assert_eq!(
        (subject.0, subject.1, subject.2.as_str()),
        (true, 0, "restored")
    );
}

#[optionized]
#[optionize(subject = models::Subject, partial(upgradable))]
struct SubjectPatch {
    enabled: Option<bool>,
    count: Option<u32>,
    label: Option<String>,
    #[optionize(skip)]
    optional: Option<u32>,
    #[cfg(any())]
    nonexistent: MissingType,
}

#[optionized]
#[optionize(object = models::Single)]
struct Subject {
    enabled: bool,
    label: String,
    #[cfg(any())]
    nonexistent: MissingType,
}

#[test]
fn cfg_supports_external_subjects_and_objects() {
    let subject = SubjectPatch {
        enabled: Some(true),
        count: Some(7),
        label: Some(String::from("subject")),
    }
    .upgrade()
    .unwrap();
    assert_eq!(subject.optional, None);
    assert_eq!(subject.count, 7);

    let patch = models::Single {
        enabled: Some(false),
        label: Some(String::from("object")),
    };
    let subject = patch.upgrade().unwrap();
    assert!(!subject.enabled);
    assert_eq!(subject.label, "object");
}

#[optionized]
struct Empty(#[cfg(any())] MissingType);

#[optionized]
#[cfg(any())]
struct Removed {
    field: MissingType,
}

#[cfg(any())]
#[optionized]
struct RemovedFirst {
    field: MissingType,
}

#[test]
fn cfg_can_remove_all_fields_or_the_entire_type() {
    let mut patch = Empty().downgrade();
    assert!(!patch.retain(&Empty()));
    let Empty() = patch.upgrade().unwrap();
}

mod reexport {
    pub use optionize::*;
}

#[reexport::optionized(crate = reexport)]
struct Reexported {
    #[cfg(any())]
    field: MissingType,
    value: u32,
}

#[test]
fn cfg_preparation_preserves_explicit_crate_paths() {
    let subject = ReexportedOptional { value: Some(12) }.upgrade().unwrap();
    assert_eq!(subject.value, 12);
}
