use core::time::Duration;

use optionize::{
    Optional, Optionizable, Optionized, PartialOptionized, Retain, Schema, optionized,
};
use optionize_test_models as models;

// Deliberately an enum, with no inherent as_ref/as_mut methods. Optional supplies
// its value type; the standard conversions supply borrowed and owned views.
#[derive(Debug, PartialEq)]
enum Update<T> {
    Missing,
    Present(T),
}

impl<T> Default for Update<T> {
    fn default() -> Self {
        Self::Missing
    }
}

impl<T> From<Update<T>> for Option<T> {
    fn from(value: Update<T>) -> Self {
        match value {
            Update::Missing => None,
            Update::Present(value) => Some(value),
        }
    }
}

impl<'a, T> From<&'a Update<T>> for Option<&'a T> {
    fn from(value: &'a Update<T>) -> Self {
        match value {
            Update::Missing => None,
            Update::Present(value) => Some(value),
        }
    }
}

impl<'a, T> From<&'a mut Update<T>> for Option<&'a mut T> {
    fn from(value: &'a mut Update<T>) -> Self {
        match value {
            Update::Missing => None,
            Update::Present(value) => Some(value),
        }
    }
}

impl<T> Optional for Update<T> {
    type Value = T;

    const NONE: Self = Self::Missing;

    fn some(value: T) -> Self {
        Self::Present(value)
    }
}

#[optionized]
#[optionize(optional = Update, attrs(.., derive(Default)))]
#[derive(Debug, PartialEq)]
struct Values {
    enabled: bool,
    count: u32,
    label: Option<String>,
    #[optionize(optional = Option)]
    plain: u32,
    #[optionize(flatten)]
    mode: u8,
}

#[test]
fn custom_wrappers_preserve_omissions_false_zero_and_explicit_clear() {
    let mut subject = Values {
        enabled: true,
        count: 7,
        label: Some(String::from("old")),
        plain: 9,
        mode: 1,
    };
    subject.load(ValuesOptional {
        enabled: Update::Present(false),
        count: Update::Present(0),
        label: Update::Present(None),
        plain: None,
        mode: 2,
    });
    assert_eq!(
        subject,
        Values {
            enabled: false,
            count: 0,
            label: None,
            plain: 9,
            mode: 2,
        }
    );
    subject.load(ValuesOptional {
        mode: 2,
        ..Default::default()
    });
    assert_eq!(subject.plain, 9);
    assert_eq!(subject.label, None);

    let patch = subject.downgrade();
    assert_eq!(patch.enabled, Update::Present(false));
    assert_eq!(patch.label, Update::Present(None));
    assert_eq!(patch.plain, Some(9));
    assert_eq!(patch.upgrade().unwrap().mode, 2);
}

#[test]
fn custom_wrappers_merge_and_retain_against_full_and_partial_values() {
    let baseline = Values {
        enabled: true,
        count: 7,
        label: Some("old".into()),
        plain: 9,
        mode: 0,
    };
    let mut patch = ValuesOptional {
        enabled: Update::Present(true),
        count: Update::Present(7),
        label: Update::Present(Some("new".into())),
        plain: Some(9),
        mode: 0,
    };
    patch.merge(ValuesOptional {
        label: Update::Present(None),
        ..Default::default()
    });
    assert_eq!(patch.count, Update::Present(7));
    assert!(patch.retain(&baseline));
    assert_eq!(patch.enabled, Update::Missing);
    assert_eq!(patch.count, Update::Missing);
    assert_eq!(patch.plain, None);
    assert_eq!(patch.label, Update::Present(None));
    assert!(patch.retain(&ValuesOptional::default()));
    assert!(!patch.retain(&ValuesOptional {
        label: Update::Present(None),
        ..Default::default()
    }));
    assert_eq!(patch.label, Update::Missing);
}

#[test]
fn custom_wrappers_validate_presence_and_supply_upgrade_defaults() {
    #[optionized]
    #[optionize(optional = "Update")]
    struct Config {
        required: String,
        #[optionize(default = |patch| Into::<Option<&String>>::into(&patch.required).unwrap().len())]
        length: usize,
        #[optionize(default = |_| Some(String::from("fallback")))]
        label: Option<String>,
    }

    let patch = ConfigOptional {
        required: Update::Present("supplied".into()),
        length: Update::Missing,
        label: Update::Present(None),
    };
    patch.validate().unwrap();
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.required, "supplied");
    assert_eq!(subject.length, 8);
    assert_eq!(subject.label, None);
    let missing = ConfigOptional {
        required: Update::Missing,
        length: Update::Missing,
        label: Update::Missing,
    };
    assert_eq!(missing.validate().unwrap_err().len(), 1);
}

#[optionized]
#[optionize(optional = Update)]
#[derive(Debug, PartialEq)]
struct Child {
    host: String,
    port: u16,
}

#[optionized]
#[optionize(optional = Update)]
struct Parent {
    #[optionize(nest = ChildOptional, default = |_| Child { host: "default".into(), port: 80 })]
    child: Child,
}

#[test]
fn custom_wrappers_recurse_when_patching_merging_and_retaining() {
    let mut patch = ParentOptional {
        child: Update::Missing,
    };
    patch.merge(ParentOptional {
        child: Update::Present(ChildOptional {
            host: Update::Missing,
            port: Update::Present(443),
        }),
    });
    patch.merge(ParentOptional {
        child: Update::Present(ChildOptional {
            host: Update::Present("new".into()),
            port: Update::Missing,
        }),
    });
    let mut subject = Parent {
        child: Child {
            host: "old".into(),
            port: 80,
        },
    };
    subject.load(patch);
    assert_eq!(
        subject.child,
        Child {
            host: "new".into(),
            port: 443
        }
    );
    let mut same = Parent {
        child: Child {
            host: "new".into(),
            port: 443,
        },
    }
    .downgrade();
    assert!(!same.retain(&subject));
    assert_eq!(same.child, Update::Missing);
    assert_eq!(
        ParentOptional {
            child: Update::Missing
        }
        .upgrade()
        .unwrap()
        .child
        .port,
        80
    );
    assert!(
        ParentOptional {
            child: Update::Present(ChildOptional {
                host: Update::Missing,
                port: Update::Present(443),
            })
        }
        .upgrade()
        .is_err()
    );
    assert_eq!(subject.downgrade().upgrade().unwrap().child.host, "new");
}

#[test]
fn custom_wrappers_preserve_distinct_non_static_lifetimes() {
    #[optionized]
    #[optionize(optional = Update)]
    struct Borrowed<'v, 'v0> {
        first: &'v str,
        second: &'v0 str,
    }

    let first = String::from("first");
    let second = String::from("second");
    let baseline = Borrowed {
        first: &first,
        second: &second,
    };
    let mut patch = Borrowed {
        first: &first,
        second: &second,
    }
    .downgrade();
    assert_eq!(patch.view().v_first, Some(&first.as_str()));
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.first, Update::Missing);
    let full = baseline.downgrade().upgrade().unwrap();
    assert_eq!(full.second, second);
}

type Alias<T> = Update<T>;
type FixedAlias = Update<u32>;

#[optionized]
#[optionize(subject = models::GenericSubject::<Value>)]
struct AliasedPatch<Value> {
    value: Alias<Value>,
}

#[optionized]
#[optionize(subject = models::GenericSubject::<u32>)]
struct FixedPatch {
    value: FixedAlias,
}

#[optionized]
#[optionize(subject = "models::GenericSubject<<Wrapper as Optional>::Value>")]
struct WrapperPatch<Wrapper>
where
    Wrapper: Optional,
    for<'a> &'a Wrapper: Into<Option<&'a Wrapper::Value>>,
    for<'a> &'a mut Wrapper: Into<Option<&'a mut Wrapper::Value>>,
{
    value: Wrapper,
}

#[optionized]
#[optionize(subject = models::BorrowedSubject::<'v, Value, N>)]
struct BorrowedPatch<'v, Value, const N: usize> {
    values: Update<&'v [Value; N]>,
}

#[test]
fn reverse_mappings_accept_custom_aliases_and_borrowed_values() {
    let mut subject = models::GenericSubject {
        value: String::from("old"),
    };
    subject.load(AliasedPatch {
        value: Update::Present(String::from("new")),
    });
    let mut patch: AliasedPatch<String> = subject.downgrade();
    let baseline = models::GenericSubject {
        value: String::from("new"),
    };
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, Update::Missing);
    let full = FixedPatch {
        value: Update::Present(0),
    }
    .upgrade()
    .unwrap();
    assert_eq!(full.value, 0);
    let values = [String::from("non-Clone conversion")];
    let patch: BorrowedPatch<'_, String, 1> =
        models::BorrowedSubject { values: &values }.downgrade();
    assert_eq!(patch.upgrade().unwrap().values, &values);
}

#[test]
fn reverse_mappings_accept_a_generic_optional_wrapper() {
    let mut subject = models::GenericSubject {
        value: String::from("old"),
    };
    let patch = WrapperPatch {
        value: Update::Present(String::from("new")),
    };
    subject.load(patch);
    let mut patch: WrapperPatch<Update<String>> = subject.downgrade();
    assert_eq!(patch.view().v_value.map(String::as_str), Some("new"));
    assert!(!patch.retain(&models::GenericSubject {
        value: String::from("new")
    }));
    assert!(patch.validate().is_err());
    patch.merge(WrapperPatch {
        value: Update::Present(String::from("complete")),
    });
    assert_eq!(patch.upgrade().unwrap().value, "complete");
}

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
#[optionize(optional = Update)]
struct Converted {
    #[optionize(as = Seconds)]
    timeout: Duration,
}

#[optionized]
#[optionize(subject = models::DurationSubject)]
struct ConvertedPatch {
    #[optionize(as = Duration)]
    timeout: Update<Seconds>,
}

#[test]
fn custom_wrappers_compose_with_value_conversion_in_both_directions() {
    let mut subject = Converted {
        timeout: Duration::from_secs(30),
    };
    subject.load(ConvertedOptional {
        timeout: Update::Present(Seconds(0)),
    });
    assert_eq!(subject.timeout, Duration::ZERO);
    let mut patch = subject.downgrade();
    assert!(patch.retain(&Converted {
        timeout: Duration::ZERO
    }));
    assert_eq!(patch.upgrade().unwrap().timeout, Duration::ZERO);
    let patch: ConvertedPatch = models::DurationSubject {
        timeout: Duration::from_secs(7),
    }
    .downgrade();
    assert_eq!(patch.timeout, Update::Present(Seconds(7)));
    assert_eq!(patch.upgrade().unwrap().timeout, Duration::from_secs(7));
}

struct ExistingPatch<Value> {
    value: Update<Value>,
}

#[optionized]
#[optionize(object = ExistingPatch::<Value>, optional = Update)]
struct ExistingSubject<Value> {
    value: Value,
}

#[test]
fn custom_wrappers_support_existing_objects_and_tuple_fields() {
    let patch = ExistingSubject {
        value: String::from("existing"),
    }
    .downgrade();
    assert_eq!(patch.upgrade().unwrap().value, "existing");

    #[optionized]
    #[optionize(optional = Update)]
    struct Tuple(u32, Option<String>);

    let mut subject = Tuple(7, Some("old".into()));
    subject.load(TupleOptional(Update::Missing, Update::Present(None)));
    assert_eq!(subject.0, 7);
    assert_eq!(subject.1, None);
    let mut same = subject.downgrade();
    assert!(!same.retain(&Tuple(7, None)));
    assert_eq!(same.0, Update::Missing);
}

#[test]
fn wrapper_paths_support_qualified_constructors_with_extra_arguments() {
    trait Wrappers {
        type Field<Marker, Value>;
    }

    struct Family;

    impl Wrappers for Family {
        type Field<Marker, Value> = Update<Value>;
    }

    #[optionized]
    #[optionize(optional = <Family as Wrappers>::Field::<()>)]
    struct Bare {
        value: String,
    }

    #[optionized]
    #[optionize(optional = "<Family as Wrappers>::Field<()>")]
    struct Quoted {
        value: String,
    }

    let patch = Bare {
        value: "bare".into(),
    }
    .downgrade();
    assert_eq!(patch.value, Update::Present(String::from("bare")));
    assert_eq!(patch.upgrade().unwrap().value, "bare");
    let patch = Quoted {
        value: "quoted".into(),
    }
    .downgrade();
    assert_eq!(patch.upgrade().unwrap().value, "quoted");
}

#[test]
fn reverse_mappings_infer_non_generic_wrapper_value_types() {
    enum NumberUpdate {
        Missing,
        Number(u32),
    }

    impl From<NumberUpdate> for Option<u32> {
        fn from(value: NumberUpdate) -> Self {
            match value {
                NumberUpdate::Number(value) => Some(value),
                NumberUpdate::Missing => None,
            }
        }
    }

    impl<'a> From<&'a NumberUpdate> for Option<&'a u32> {
        fn from(value: &'a NumberUpdate) -> Self {
            match value {
                NumberUpdate::Number(value) => Some(value),
                NumberUpdate::Missing => None,
            }
        }
    }

    impl<'a> From<&'a mut NumberUpdate> for Option<&'a mut u32> {
        fn from(value: &'a mut NumberUpdate) -> Self {
            match value {
                NumberUpdate::Number(value) => Some(value),
                NumberUpdate::Missing => None,
            }
        }
    }

    impl Optional for NumberUpdate {
        type Value = u32;
        const NONE: Self = Self::Missing;

        fn some(value: u32) -> Self {
            Self::Number(value)
        }
    }

    struct Subject {
        count: u32,
        label: String,
    }

    #[optionized]
    #[optionize(subject = Subject)]
    struct Patch {
        count: NumberUpdate,
        label: Update<String>,
    }

    let mut subject = Subject {
        count: 7,
        label: "old".into(),
    };
    subject.load(Patch {
        count: NumberUpdate::Number(0),
        label: Update::Present("new".into()),
    });
    assert_eq!(subject.count, 0);
    assert_eq!(subject.label, "new");
    let patch = subject.downgrade();
    assert_eq!(patch.view().v_count, Some(&0));
    assert_eq!(patch.upgrade().unwrap().label, "new");
}

#[test]
fn nested_merge_preserves_incoming_wrapper_metadata_when_previously_missing() {
    struct Tracked<T> {
        value: Option<T>,
        source: &'static str,
    }

    impl<T> From<Tracked<T>> for Option<T> {
        fn from(value: Tracked<T>) -> Self {
            value.value
        }
    }

    impl<'a, T> From<&'a Tracked<T>> for Option<&'a T> {
        fn from(value: &'a Tracked<T>) -> Self {
            value.value.as_ref()
        }
    }

    impl<'a, T> From<&'a mut Tracked<T>> for Option<&'a mut T> {
        fn from(value: &'a mut Tracked<T>) -> Self {
            value.value.as_mut()
        }
    }

    impl<T> Optional for Tracked<T> {
        type Value = T;

        const NONE: Self = Self {
            value: None,
            source: "",
        };

        fn some(value: T) -> Self {
            Self {
                value: Some(value),
                source: "constructed",
            }
        }
    }

    #[optionized]
    #[optionize(optional = Tracked)]
    struct Config {
        #[optionize(nest = ChildOptional)]
        child: Child,
    }

    let mut patch = ConfigOptional {
        child: Tracked::NONE,
    };
    patch.merge(ConfigOptional {
        child: Tracked {
            value: Some(ChildOptional {
                host: Update::Present("incoming".into()),
                port: Update::Present(443),
            }),
            source: "input file",
        },
    });
    assert_eq!(patch.child.source, "input file");
    patch.merge(ConfigOptional {
        child: Tracked::NONE,
    });
    assert_eq!(patch.child.source, "input file");
    assert_eq!(patch.upgrade().unwrap().child.host, "incoming");
}
