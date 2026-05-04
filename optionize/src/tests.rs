use super::*;
use alloc::string::ToString;
use core::marker::PhantomData;

// ==========================================
// Basic Fields: Wrap & Flatten
// ==========================================
#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Basic {
    a: u32,
    #[optionize(flatten)]
    b: u32,
    c: Option<u32>,
    #[optionize(flatten)]
    d: Option<u32>,
}

#[test]
fn test_basic_upgrade() {
    let o = BasicOptional {
        a: Some(1),
        b: 2,
        c: Some(Some(3)),
        d: Some(4),
    };

    assert!(o.validate().is_ok());

    let subject = o.clone().upgrade().unwrap();
    assert_eq!(
        subject,
        Basic {
            a: 1,
            b: 2,
            c: Some(3),
            d: Some(4),
        }
    );
}

#[test]
fn test_basic_patch() {
    let o = BasicOptional {
        a: Some(1),
        b: 2,
        c: Some(Some(3)),
        d: Some(4),
    };

    let mut subject = Basic {
        a: 0,
        b: 0,
        c: None,
        d: None,
    };

    o.patch(&mut subject);
    assert_eq!(
        subject,
        Basic {
            a: 1,
            b: 2,
            c: Some(3),
            d: Some(4),
        }
    );
}

#[test]
fn test_basic_merge() {
    let mut o1 = BasicOptional {
        a: Some(1),
        b: 2,
        c: Some(Some(3)),
        d: Some(4),
    };

    let o2 = BasicOptional {
        a: Some(10),
        b: 20,
        c: None,
        d: None,
    };

    o1.merge(o2);
    assert_eq!(
        o1,
        BasicOptional {
            a: Some(10),
            b: 20,
            c: Some(Some(3)), // Not replaced since o2.c is None and it's wrapped
            d: None,          // Replaced since it's flattened
        }
    );
}

#[test]
fn test_downgrade() {
    let subject = Basic {
        a: 1,
        b: 2,
        c: Some(3),
        d: Some(4),
    };

    let downgraded: BasicOptional = subject.downgrade();
    assert_eq!(
        downgraded,
        BasicOptional {
            a: Some(1),
            b: 2,
            c: Some(Some(3)),
            d: Some(4),
        }
    );
}

#[test]
fn test_load() {
    let mut subject = Basic {
        a: 0,
        b: 0,
        c: None,
        d: None,
    };

    let partial = BasicOptional {
        a: Some(1),
        b: 2,
        c: Some(Some(3)),
        d: Some(4),
    };

    subject.load(partial);
    assert_eq!(
        subject,
        Basic {
            a: 1,
            b: 2,
            c: Some(3),
            d: Some(4),
        }
    );
}

// ==========================================
// Custom Names & Attributes
// ==========================================
#[optionized]
#[optionize(name = "Custom{}", attrs(derive(Debug, PartialEq, Clone)))]
#[derive(Debug, PartialEq)]
struct Renamed {
    #[optionize(name = "x")]
    a: u32,
    #[optionize(attrs(doc = "Test field level attribute"))]
    b: u32,
}

#[test]
fn test_renamed_and_attrs() {
    let o = CustomRenamed {
        x: Some(1),
        b: Some(2),
    };

    // Test derived Clone (from attrs override)
    let o2 = o.clone();
    assert_eq!(o, o2);

    assert_eq!(o.upgrade().unwrap(), Renamed { a: 1, b: 2 });
}

// ==========================================
// Skip Upgrades
// ==========================================
#[optionized]
#[optionize(partial(upgradable))]
#[derive(Debug, PartialEq)]
struct Skipped {
    #[optionize(skip(upgrade = 42))]
    a: u32,
    b: u32,
}

#[test]
fn test_skipped() {
    let o = SkippedOptional { b: Some(1) };
    assert_eq!(o.upgrade().unwrap(), Skipped { a: 42, b: 1 });
}

#[optionized]
#[optionize(partial(upgradable))]
#[derive(Debug, PartialEq)]
struct SkippedDefault {
    #[optionize(skip)]
    a: u32,
    b: u32,
}

#[test]
fn test_skipped_default() {
    let o = SkippedDefaultOptional { b: Some(1) };
    assert_eq!(o.upgrade().unwrap(), SkippedDefault { a: 0, b: 1 });
}

// ==========================================
// Marked (PhantomData / Generics)
// ==========================================
#[optionized]
#[optionize(partial(marked, upgradable))]
#[derive(Debug, PartialEq)]
struct MarkedUnit;

#[test]
fn test_marked_unit() {
    let o = MarkedUnitOptional(PhantomData);
    assert_eq!(o.upgrade().unwrap(), MarkedUnit);
}

#[optionized]
#[optionize(partial(marked(name = _marker), upgradable))]
#[derive(Debug, PartialEq)]
struct MarkedStruct {
    a: u32,
}

#[test]
fn test_marked_struct() {
    let o = MarkedStructOptional {
        a: Some(1),
        _marker: PhantomData,
    };
    assert_eq!(o.upgrade().unwrap(), MarkedStruct { a: 1 });
}

#[optionized]
#[optionize(partial(marked(name = _marker), upgradable))]
#[derive(Debug, PartialEq)]
struct MarkedGeneric<T: Default> {
    #[optionize(skip)]
    a: T,
}

#[test]
fn test_marked_generic() {
    let o = MarkedGenericOptional::<u32> {
        _marker: PhantomData,
    };
    assert_eq!(o.upgrade().unwrap(), MarkedGeneric { a: 0 });
}

// ==========================================
// Unnamed Fields
// ==========================================
#[optionized]
#[derive(Debug, PartialEq)]
struct Unnamed(u32, #[optionize(flatten)] u32);

#[test]
fn test_unnamed() {
    let o = UnnamedOptional(Some(1), 2);
    assert_eq!(o.upgrade().unwrap(), Unnamed(1, 2));
}

// ==========================================
// Nested Types
// ==========================================
#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Inner {
    x: u32,
}

#[optionized]
#[derive(Debug, PartialEq, Clone)]
struct Outer {
    #[optionize(nest = "InnerOptional")]
    a: Inner,
    #[optionize(flatten, nest = "InnerOptional")]
    b: Inner,
}

#[test]
fn test_nested() {
    let mut o = OuterOptional {
        a: Some(InnerOptional { x: Some(1) }),
        b: InnerOptional { x: Some(2) },
    };

    assert_eq!(
        o.clone().upgrade().unwrap(),
        Outer {
            a: Inner { x: 1 },
            b: Inner { x: 2 },
        }
    );

    let o2 = OuterOptional {
        a: Some(InnerOptional { x: Some(10) }),
        b: InnerOptional { x: None }, // x remains from original due to nesting wrap logic
    };

    o.merge(o2);
    assert_eq!(
        o,
        OuterOptional {
            a: Some(InnerOptional { x: Some(10) }),
            b: InnerOptional { x: Some(2) }, // Due to inner merge strategy
        }
    );
}

// ==========================================
// Validation Errors
// ==========================================
#[test]
fn test_errors() {
    let o = BasicOptional {
        a: None,
        b: 0,
        c: None,
        d: None,
    };

    let errs = o.validate().unwrap_err();
    let err_str = errs.to_string();
    assert!(err_str.contains("Missing required field"));

    let o2 = OuterOptional {
        a: Some(InnerOptional { x: None }),
        b: InnerOptional { x: Some(1) },
    };
    let errs = o2.validate().unwrap_err();
    let err_str = errs.to_string();
    assert!(err_str.contains("Failed to upgrade nested field"));
}
