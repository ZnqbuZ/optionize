use super::*;
use alloc::string::{String, ToString};
use core::marker::PhantomData;

#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Config {
    count: u32,
    #[optionize(flatten)]
    limit: u32,
    optional: Option<u32>,
    #[optionize(flatten)]
    flattened: Option<u32>,
}

#[test]
fn patch_and_load_preserve_omitted_fields_and_apply_explicit_clears() {
    for count in [None, Some(0)] {
        for optional in [None, Some(None), Some(Some(0))] {
            let mut subject = Config {
                count: 1,
                limit: 2,
                optional: Some(3),
                flattened: Some(4),
            };
            let mut loaded = subject.clone();
            let patch = ConfigOptional {
                count,
                limit: 0,
                optional,
                flattened: None,
            };
            patch.clone().patch(&mut subject);
            loaded.load(patch);
            assert_eq!(
                subject,
                Config {
                    count: count.unwrap_or(1),
                    limit: 0,
                    optional: optional.unwrap_or(Some(3)),
                    flattened: None,
                }
            );
            assert_eq!(loaded, subject);
        }
    }
}

#[test]
fn downgrade_and_upgrade_preserve_nullable_and_flattened_fields() {
    for optional in [None, Some(3)] {
        let subject = Config {
            count: 1,
            limit: 2,
            optional,
            flattened: optional,
        };
        let patch: ConfigOptional = subject.clone().downgrade();
        assert_eq!(
            patch,
            ConfigOptional {
                count: Some(1),
                limit: 2,
                optional: Some(optional),
                flattened: optional,
            }
        );
        assert!(patch.validate().is_ok());
        assert_eq!(patch.upgrade().unwrap(), subject);
    }
}

#[test]
fn merge_preserves_omitted_fields_and_applies_explicit_clears() {
    for previous in [None, Some(None), Some(Some(1))] {
        for incoming in [None, Some(None), Some(Some(0))] {
            let mut patch = ConfigOptional {
                count: Some(1),
                limit: 2,
                optional: previous,
                flattened: Some(3),
            };
            patch.merge(ConfigOptional {
                count: None,
                limit: 0,
                optional: incoming,
                flattened: None,
            });
            assert_eq!(
                patch,
                ConfigOptional {
                    count: Some(1),
                    limit: 0,
                    optional: incoming.or(previous),
                    flattened: None,
                }
            );
        }
    }
}

#[optionized]
#[optionize(
    name = "Custom{}",
    attrs(derive(Debug, PartialEq)),
    attrs(derive(Clone))
)]
#[derive(Debug, PartialEq)]
struct Renamed {
    #[optionize(name = "renamed_{}")]
    value: u32,
    #[optionize(attrs(doc = "The generated field's documentation."))]
    other: u32,
}

#[test]
fn attributes_accumulate_derives_and_expand_name_templates() {
    let patch = CustomRenamed {
        renamed_value: Some(1),
        other: Some(2),
    };
    assert_eq!(patch.clone(), patch);
    assert_eq!(patch.upgrade().unwrap(), Renamed { value: 1, other: 2 });
}

#[optionized]
#[optionize(partial(upgradable))]
#[derive(Debug, PartialEq)]
struct Skipped {
    #[optionize(skip(upgrade = 42))]
    cached: u32,
    #[optionize(skip)]
    label: String,
    count: u32,
}

#[test]
fn skip_preserves_subject_fields_and_supplies_upgrade_defaults() {
    let mut subject = Skipped {
        cached: 9,
        label: "kept".into(),
        count: 1,
    };
    subject.load(SkippedOptional { count: Some(2) });
    assert_eq!(subject.cached, 9);
    assert_eq!(subject.label, "kept");
    let patch: SkippedOptional = subject.downgrade();
    assert_eq!(patch.count, Some(2));
    assert_eq!(
        patch.upgrade().unwrap(),
        Skipped {
            cached: 42,
            label: String::new(),
            count: 2,
        }
    );
}

#[test]
fn skip_evaluates_upgrade_expressions_only_when_upgrade_succeeds() {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

    struct Connection(usize);

    #[optionized]
    #[optionize(partial(upgradable))]
    struct Config {
        #[optionize(skip(upgrade = Connection(CONSTRUCTIONS.fetch_add(1, Ordering::SeqCst))))]
        connection: Connection,
        enabled: bool,
    }

    let missing = ConfigOptional { enabled: None };
    assert!(missing.validate().is_err());
    assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);

    let complete = ConfigOptional {
        enabled: Some(false),
    };
    assert!(complete.validate().is_ok());
    assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);

    assert!(missing.upgrade().is_err());
    assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);

    let subject = complete.upgrade().unwrap();
    assert_eq!(subject.connection.0, 0);
    assert!(!subject.enabled);
    assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 1);
}

#[test]
fn partial_skips_fields_without_default_or_upgrade() {
    struct Connection(u32);

    #[optionized]
    #[optionize(partial)]
    struct Config {
        #[optionize(skip)]
        connection: Connection,
        enabled: bool,
    }

    let mut subject = Config {
        connection: Connection(7),
        enabled: true,
    };
    subject.load(ConfigOptional {
        enabled: Some(false),
    });
    assert_eq!(subject.connection.0, 7);
    let patch: ConfigOptional = subject.downgrade();
    assert_eq!(patch.enabled, Some(false));
}

#[test]
fn marked_preserves_tuple_fields_and_supports_both_unit_shapes() {
    #[optionized]
    #[optionize(partial(marked, upgradable))]
    #[derive(Debug, PartialEq)]
    struct Tuple(u32, #[optionize(skip)] String, #[optionize(flatten)] u32);

    #[optionized]
    #[optionize(partial(marked, upgradable))]
    #[derive(Debug, PartialEq)]
    struct Unit;

    #[optionized]
    #[optionize(partial(marked(name = marker), upgradable))]
    #[derive(Debug, PartialEq)]
    struct NamedUnit;

    assert_eq!(
        TupleOptional(Some(1), 2, PhantomData).upgrade().unwrap(),
        Tuple(1, String::new(), 2)
    );
    assert_eq!(UnitOptional(PhantomData).upgrade().unwrap(), Unit);
    assert_eq!(
        NamedUnitOptional {
            marker: PhantomData
        }
        .upgrade()
        .unwrap(),
        NamedUnit
    );
}

#[test]
fn marked_avoids_existing_field_names() {
    #[optionized]
    #[optionize(partial(marked, upgradable))]
    #[derive(Debug, PartialEq)]
    struct Config {
        r#_marker: u32,
        __marker: u32,
    }

    let patch = ConfigOptional {
        _marker: Some(1),
        __marker: Some(2),
        ___marker: PhantomData,
    };
    assert_eq!(
        patch.upgrade().unwrap(),
        Config {
            _marker: 1,
            __marker: 2
        }
    );
}

#[test]
fn marked_preserves_generics_used_only_by_skipped_fields() {
    #[optionized]
    #[optionize(partial(marked(name = marker), upgradable))]
    #[derive(Debug, PartialEq)]
    struct Config<Value: Default> {
        #[optionize(skip)]
        value: Value,
    }

    let patch = ConfigOptional::<String> {
        marker: PhantomData,
    };
    assert_eq!(
        patch.upgrade().unwrap(),
        Config {
            value: String::new()
        }
    );
}

#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Inner {
    first: u32,
    second: u32,
}

#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Outer {
    #[optionize(nest = InnerOptional)]
    nested: Inner,
    #[optionize(flatten, nest = InnerOptional)]
    flattened: Inner,
}

#[test]
fn nested_merge_handles_present_and_absent_objects() {
    for previous in [
        None,
        Some(InnerOptional {
            first: Some(1),
            second: Some(2),
        }),
    ] {
        for incoming in [
            None,
            Some(InnerOptional {
                first: Some(3),
                second: None,
            }),
        ] {
            let mut patch = OuterOptional {
                nested: previous.clone(),
                flattened: InnerOptional {
                    first: Some(1),
                    second: Some(2),
                },
            };
            patch.merge(OuterOptional {
                nested: incoming.clone(),
                flattened: InnerOptional {
                    first: Some(3),
                    second: None,
                },
            });
            assert_eq!(
                patch.flattened,
                InnerOptional {
                    first: Some(3),
                    second: Some(2)
                }
            );
            let expected = match (&previous, &incoming) {
                (Some(_), Some(_)) => Some(InnerOptional {
                    first: Some(3),
                    second: Some(2),
                }),
                (_, Some(incoming)) => Some(incoming.clone()),
                (previous, None) => previous.clone(),
            };
            assert_eq!(patch.nested, expected);
        }
    }
}

#[test]
fn nested_patch_preserves_omitted_children_and_recurses_into_flattened_fields() {
    let mut subject = Outer {
        nested: Inner {
            first: 1,
            second: 2,
        },
        flattened: Inner {
            first: 3,
            second: 4,
        },
    };
    subject.load(OuterOptional {
        nested: None,
        flattened: InnerOptional {
            first: Some(0),
            second: None,
        },
    });
    assert_eq!(
        subject.nested,
        Inner {
            first: 1,
            second: 2
        }
    );
    assert_eq!(
        subject.flattened,
        Inner {
            first: 0,
            second: 4
        }
    );
    subject.load(OuterOptional {
        nested: Some(InnerOptional {
            first: None,
            second: Some(0),
        }),
        flattened: InnerOptional {
            first: None,
            second: None,
        },
    });
    assert_eq!(
        subject.nested,
        Inner {
            first: 1,
            second: 0
        }
    );
    assert_eq!(
        subject.flattened,
        Inner {
            first: 0,
            second: 4
        }
    );
    let patch: OuterOptional = subject.clone().downgrade();
    assert_eq!(patch.upgrade().unwrap(), subject);
}

#[test]
fn validate_reports_all_missing_fields_with_original_and_generated_names() {
    let patch = CustomRenamed {
        renamed_value: None,
        other: None,
    };
    let errors = patch.validate().unwrap_err();
    let ty = TypeInfo {
        subject: "Renamed",
        object: "CustomRenamed",
    };
    assert!(matches!(errors.as_slice(), [
        Error::Missing {
            ty: first,
            field: FieldInfo::Renamed { original: "value", optionized: "renamed_value" },
        },
        Error::Missing { ty: second, field: FieldInfo::Identical("other") },
    ] if *first == ty && *second == ty));
    assert_eq!(patch.upgrade().unwrap_err().to_string(), errors.to_string());
}

#[test]
fn validate_requires_nullable_fields_but_accepts_explicit_clears() {
    let mut patch = ConfigOptional {
        count: Some(0),
        limit: 0,
        optional: None,
        flattened: None,
    };
    let errors = patch.validate().unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [Error::Missing {
            field: FieldInfo::Identical("optional"),
            ..
        }]
    ));
    patch.optional = Some(None);
    assert!(patch.validate().is_ok());
    assert_eq!(patch.upgrade().unwrap().optional, None);
}

#[test]
fn validate_reports_every_nested_error_and_preserves_source_chains() {
    use core::error::Error as _;

    let mut patch = OuterOptional {
        nested: Some(InnerOptional {
            first: None,
            second: None,
        }),
        flattened: InnerOptional {
            first: Some(1),
            second: None,
        },
    };
    let errors = patch.validate().unwrap_err();
    assert_eq!(errors.len(), 3);
    for (error, (parent, child)) in errors.iter().zip([
        ("nested", "first"),
        ("nested", "second"),
        ("flattened", "second"),
    ]) {
        assert!(
            matches!(error, Error::Nested { ty, field: FieldInfo::Identical(field), .. }
            if ty.subject == "Outer" && ty.object == "OuterOptional" && *field == parent)
        );
        let source = error.source().unwrap().downcast_ref::<Error>().unwrap();
        assert!(
            matches!(source, Error::Missing { ty, field: FieldInfo::Identical(field) }
            if ty.subject == "Inner" && ty.object == "InnerOptional" && *field == child)
        );
        assert!(source.source().is_none());
    }

    patch.nested = None;
    let errors = patch.validate().unwrap_err();
    assert!(matches!(
        errors.as_slice(),
        [
            Error::Missing {
                field: FieldInfo::Identical("nested"),
                ..
            },
            Error::Nested {
                field: FieldInfo::Identical("flattened"),
                ..
            },
        ]
    ));
}

#[test]
fn optionized_accepts_an_explicit_crate_path() {
    use crate as config;

    #[config::optionized(crate = config)]
    #[derive(Debug, PartialEq)]
    struct Config {
        enabled: bool,
    }

    assert_eq!(
        ConfigOptional {
            enabled: Some(false)
        }
        .upgrade()
        .unwrap(),
        Config { enabled: false }
    );
}
