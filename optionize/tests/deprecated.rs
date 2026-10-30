#![deny(deprecated)]

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};

#[optionized]
#[derive(Debug, Default, PartialEq)]
struct Config {
    #[deprecated(note = "use enabled")]
    old: bool,
    enabled: bool,
}

#[optionized]
#[derive(Debug, Default, PartialEq)]
struct Tuple(#[deprecated] bool);

// Only the generated object deprecates this field.
#[optionized]
#[derive(Debug, Default, PartialEq)]
struct Renamed {
    #[optionize(attrs(deprecated = "use enabled"))]
    old: bool,
}

#[deprecated]
#[derive(Debug, Default, PartialEq)]
struct Legacy;

// The struct allows its deprecated field type, so the generated code does too.
#[optionized]
#[allow(deprecated)]
#[derive(Debug, Default, PartialEq)]
struct Holder {
    legacy: Legacy,
}

// Nothing here is deprecated, so the generated code needs no `allow(deprecated)`,
// which `forbid` would reject.
#[forbid(deprecated)]
mod forbidden {
    use optionize::optionized;

    #[optionized]
    #[derive(Debug, Default, PartialEq)]
    pub(super) struct Config {
        pub(super) value: u32,
    }
}

#[test]
fn generated_operations_can_access_deprecated_fields() {
    let mut subject = Config::default();
    let mut object = subject.downgrade();
    object.merge(ConfigOptional::default());
    assert!(!object.retain(&Config::default()));
    subject = Config::default();
    subject.load(object);
    assert_eq!(subject.downgrade().upgrade().unwrap(), Config::default());
    assert_eq!(
        Tuple::default().downgrade().upgrade().unwrap(),
        Tuple::default()
    );
}

#[test]
fn generated_operations_follow_deprecations_on_the_object_and_allowed_types() {
    assert_eq!(
        Renamed::default().downgrade().upgrade().unwrap(),
        Renamed::default()
    );
    let mut object = Holder::default().downgrade();
    assert!(!object.retain(&Holder::default()));
    object.merge(Holder::default().downgrade());
    assert_eq!(object.upgrade().unwrap(), Holder::default());
}

#[test]
fn mappings_without_deprecations_compile_where_deprecated_is_forbidden() {
    let config = forbidden::ConfigOptional { value: Some(1) }
        .upgrade()
        .unwrap();
    assert_eq!(config, forbidden::Config { value: 1 });
}
