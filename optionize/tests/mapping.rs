use core::time::Duration;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};
use optionize_test_models as models;

/// The object representation of a [`Duration`] owned by the mapping crate.
#[derive(Debug, PartialEq)]
struct Seconds(u64);

impl From<Duration> for Seconds {
    fn from(value: Duration) -> Self {
        Self(value.as_secs())
    }
}

impl From<Seconds> for Duration {
    fn from(value: Seconds) -> Self {
        Self::from_secs(value.0)
    }
}

#[optionized]
#[derive(Debug, PartialEq)]
struct Config {
    #[optionize(as = Seconds)]
    timeout: Duration,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct Flattened {
    #[optionize(flatten, as = Seconds)]
    interval: Duration,
}

#[optionized]
#[optionize(subject = models::DurationSubject)]
#[derive(Debug, PartialEq)]
struct ConvertedPatch {
    #[optionize(as = Duration)]
    timeout: Option<Seconds>,
}

#[optionized]
#[derive(Debug, PartialEq)]
struct Defaulted {
    #[optionize(as = Seconds, default = |_| Duration::from_secs(1))]
    timeout: Duration,
}

// The subject type deliberately implements neither `Clone` nor `PartialEq`.
struct Payload(String);

impl From<Payload> for String {
    fn from(value: Payload) -> Self {
        value.0
    }
}

impl From<String> for Payload {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[optionized]
struct Wrapped {
    #[optionize(as = String)]
    payload: Payload,
}

#[test]
fn converted_fields_map_between_subject_and_object_types() {
    let mut patch = Config {
        timeout: Duration::from_secs(30),
    }
    .downgrade();
    assert_eq!(
        patch,
        ConfigOptional {
            timeout: Some(Seconds(30))
        }
    );

    let mut subject = Config {
        timeout: Duration::ZERO,
    };
    subject.load(ConfigOptional {
        timeout: Some(Seconds(7)),
    });
    assert_eq!(subject.timeout, Duration::from_secs(7));

    patch.merge(ConfigOptional { timeout: None });
    assert_eq!(patch.timeout, Some(Seconds(30)));
    patch.merge(ConfigOptional {
        timeout: Some(Seconds(5)),
    });
    assert_eq!(patch.timeout, Some(Seconds(5)));

    assert_eq!(
        ConfigOptional {
            timeout: Some(Seconds(3))
        }
        .upgrade()
        .unwrap(),
        Config {
            timeout: Duration::from_secs(3)
        }
    );
}

#[test]
fn missing_converted_fields_fail_validation() {
    let errors = ConfigOptional { timeout: None }.validate().unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors.to_string().contains("`timeout`"));
}

#[test]
fn converted_fields_accept_upgrade_defaults() {
    // The default produces the subject's type, and only supplied values convert.
    let subject: Defaulted = DefaultedOptional { timeout: None }.upgrade().unwrap();
    assert_eq!(subject.timeout, Duration::from_secs(1));

    let subject: Defaulted = DefaultedOptional {
        timeout: Some(Seconds(9)),
    }
    .upgrade()
    .unwrap();
    assert_eq!(subject.timeout, Duration::from_secs(9));
}

#[test]
fn flattened_converted_fields_always_patch() {
    let mut subject = Flattened {
        interval: Duration::from_secs(1),
    };
    subject.load(FlattenedOptional {
        interval: Seconds(4),
    });
    assert_eq!(subject.interval, Duration::from_secs(4));

    assert_eq!(
        Flattened {
            interval: Duration::from_secs(2)
        }
        .downgrade(),
        FlattenedOptional {
            interval: Seconds(2)
        }
    );
    assert_eq!(
        FlattenedOptional {
            interval: Seconds(6)
        }
        .upgrade()
        .unwrap(),
        Flattened {
            interval: Duration::from_secs(6)
        }
    );
}

#[test]
fn retain_keeps_converted_updates() {
    let mut patch = ConfigOptional {
        timeout: Some(Seconds(5)),
    };
    assert!(patch.retain(&Config {
        timeout: Duration::from_secs(5)
    }));
    assert_eq!(patch.timeout, Some(Seconds(5)));

    let mut flattened = FlattenedOptional {
        interval: Seconds(5),
    };
    assert!(flattened.retain(&Flattened {
        interval: Duration::from_secs(5)
    }));
    assert_eq!(flattened.interval, Seconds(5));

    let mut empty = ConfigOptional { timeout: None };
    assert!(!empty.retain(&Config {
        timeout: Duration::ZERO
    }));
}

#[test]
fn converted_fields_do_not_require_subject_comparison() {
    let mut patch = WrappedOptional {
        payload: Some(String::from("node")),
    };
    assert!(patch.retain(&Wrapped {
        payload: Payload(String::from("node")),
    }));
    assert_eq!(patch.payload.as_deref(), Some("node"));

    let subject: Wrapped = patch.upgrade().unwrap();
    assert_eq!(subject.payload.0, "node");
}

#[test]
fn subject_mappings_convert_the_declared_object_type() {
    let mut subject = models::DurationSubject {
        timeout: Duration::from_secs(1),
    };
    subject.load(ConvertedPatch {
        timeout: Some(Seconds(4)),
    });
    assert_eq!(subject.timeout, Duration::from_secs(4));

    let patch: ConvertedPatch = subject.downgrade();
    assert_eq!(patch.timeout, Some(Seconds(4)));
    assert_eq!(
        patch.upgrade().unwrap(),
        models::DurationSubject {
            timeout: Duration::from_secs(4)
        }
    );
}
