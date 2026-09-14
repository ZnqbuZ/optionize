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
