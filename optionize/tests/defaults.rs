use std::cell::{Cell, RefCell};
use std::rc::Rc;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};
use optionize_test_models as models;

struct Payload(String);

#[optionized]
#[optionize(partial(upgradable))]
struct Config {
    payload: Payload,
    #[optionize(flatten)]
    calls: Rc<RefCell<Vec<&'static str>>>,
    #[optionize(default = |object| {
        object.calls.borrow_mut().push("size");
        object.payload.as_ref().unwrap().0.len()
    })]
    size: usize,
    #[optionize(skip, default = |object| {
        object.calls.borrow_mut().push("skipped");
        assert!(object.size.is_none());
        object.payload.as_ref().unwrap().0.len() + 1
    })]
    skipped: usize,
}

#[test]
fn defaults_borrow_original_non_clone_fields_before_any_moves() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let patch = ConfigOptional {
        payload: Some(Payload(String::from("data"))),
        calls: calls.clone(),
        size: None,
    };
    patch.validate().unwrap();
    assert!(calls.borrow().is_empty());
    let config = patch.upgrade().unwrap();
    assert_eq!(config.size, 4);
    assert_eq!(config.skipped, 5);
    assert_eq!(config.payload.0, "data");
    assert_eq!(*calls.borrow(), ["size", "skipped"]);
}

#[test]
fn failed_validation_does_not_run_any_defaults() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let patch = ConfigOptional {
        payload: None,
        calls: calls.clone(),
        size: None,
    };
    assert!(patch.upgrade().is_err());
    assert!(calls.borrow().is_empty());
}

#[optionized]
#[derive(Debug, PartialEq)]
struct Values {
    #[optionize(default = |_| true)]
    enabled: bool,
    #[optionize(default = |_| 7)]
    count: u32,
    #[optionize(default = |_| String::from("fallback"))]
    name: String,
    #[optionize(default = |_| Some(String::from("fallback")))]
    note: Option<String>,
}

#[test]
fn defaults_distinguish_omissions_from_false_zero_empty_and_clear() {
    let patch = ValuesOptional {
        enabled: None,
        count: None,
        name: None,
        note: None,
    };
    let full = patch.upgrade().unwrap();
    assert!(full.enabled);
    assert_eq!(full.count, 7);
    assert_eq!(full.name, "fallback");
    assert_eq!(full.note.as_deref(), Some("fallback"));

    let patch = ValuesOptional {
        enabled: Some(false),
        count: Some(0),
        name: Some(String::new()),
        note: Some(None),
    };
    let full = patch.upgrade().unwrap();
    assert!(!full.enabled);
    assert_eq!(full.count, 0);
    assert!(full.name.is_empty());
    assert_eq!(full.note, None);
}

#[test]
fn defaults_do_not_change_patch_merge_or_retain() {
    let mut full = Values {
        enabled: false,
        count: 0,
        name: String::new(),
        note: None,
    };
    let mut patch = ValuesOptional {
        enabled: None,
        count: None,
        name: None,
        note: None,
    };
    patch.merge(ValuesOptional {
        enabled: Some(false),
        count: None,
        name: None,
        note: None,
    });
    assert!(!patch.retain(&full));
    assert_eq!(patch.enabled, None);
    full.load(patch);
    assert!(!full.enabled);
    assert_eq!(full.count, 0);
    assert!(full.name.is_empty());
    assert_eq!(full.note, None);
}

#[optionized]
struct Child {
    required: String,
}

#[optionized]
struct Parent {
    #[optionize(nest = ChildOptional, default = |_| Child { required: String::from("child") })]
    child: Child,
}

#[test]
fn nested_defaults_supply_complete_children_only_when_omitted() {
    let full = ParentOptional { child: None }.upgrade().unwrap();
    assert_eq!(full.child.required, "child");
    assert!(
        ParentOptional {
            child: Some(ChildOptional { required: None })
        }
        .upgrade()
        .is_err()
    );
    let full = ParentOptional {
        child: Some(ChildOptional {
            required: Some(String::from("provided")),
        }),
    }
    .upgrade()
    .unwrap();
    assert_eq!(full.child.required, "provided");
}

#[optionized]
struct Generic<Value> {
    #[optionize(default)]
    value: Value,
}

#[test]
fn standard_defaults_only_constrain_upgrading() {
    struct NoDefault;
    let mut full = Generic { value: NoDefault };
    full.load(GenericOptional {
        value: Some(NoDefault),
    });
    let _: GenericOptional<NoDefault> = full.downgrade();
    let full = GenericOptional::<String> { value: None }.upgrade().unwrap();
    assert!(full.value.is_empty());
}

#[optionized]
struct Borrowed<'s> {
    source: &'s str,
    #[optionize(default = |object| object.source.unwrap())]
    value: &'s str,
}

#[test]
fn defaults_preserve_subject_lifetimes() {
    let source = String::from("borrowed");
    let full = BorrowedOptional {
        source: Some(&source),
        value: None,
    }
    .upgrade()
    .unwrap();
    assert_eq!(full.source, full.value);
}

#[optionized]
#[optionize(object = models::Single)]
struct ExternalObject {
    #[optionize(default = |_| true)]
    enabled: bool,
    #[optionize(default = |object| if object.enabled == Some(false) { "disabled" } else { "enabled" }.into())]
    label: String,
}

#[optionized]
#[optionize(subject = models::Subject, partial(upgradable))]
struct ExternalSubjectPatch {
    #[optionize(default = |_| true)]
    enabled: Option<bool>,
    #[optionize(default)]
    count: Option<u32>,
    #[optionize(default = label)]
    label: Option<String>,
    #[optionize(skip, default)]
    optional: Option<u32>,
}

fn label(object: &ExternalSubjectPatch) -> String {
    format!("count: {}", object.count.unwrap_or_default())
}

#[test]
fn defaults_support_both_external_mapping_directions_and_named_functions() {
    let full = models::Single {
        enabled: Some(false),
        label: None,
    }
    .upgrade()
    .unwrap();
    assert!(!full.enabled);
    assert_eq!(full.label, "disabled");
    let full = ExternalSubjectPatch {
        enabled: None,
        count: Some(3),
        label: None,
    }
    .upgrade()
    .unwrap();
    assert!(full.enabled);
    assert_eq!(full.count, 3);
    assert_eq!(full.label, "count: 3");
    assert_eq!(full.optional, None);
}

#[optionized]
#[optionize(partial(upgradable))]
struct Tuple(
    #[optionize(default = |object| object.1.as_deref().map_or(0, str::len))] usize,
    String,
    #[optionize(skip, default = |object| object.0.unwrap_or(9))] usize,
);

#[test]
fn tuple_defaults_use_compacted_object_indices() {
    let full = TupleOptional(None, Some(String::from("tuple")))
        .upgrade()
        .unwrap();
    assert_eq!((full.0, full.1.as_str(), full.2), (5, "tuple", 9));
}

#[optionized]
struct Counted {
    #[optionize(flatten)]
    calls: Rc<Cell<u32>>,
    #[optionize(default = |object| {
        object.calls.set(object.calls.get() + 1);
        7
    })]
    value: u32,
}

#[test]
fn unchecked_upgrade_evaluates_only_missing_defaults_once() {
    let calls = Rc::new(Cell::new(0));
    for (value, expected) in [(Some(0), 0), (None, 7)] {
        let patch = CountedOptional {
            calls: calls.clone(),
            value,
        };
        patch.validate().unwrap();
        // SAFETY: the patch has been validated and its default returns a full u32.
        let full = unsafe { patch.upgrade_unchecked() };
        assert_eq!(full.value, expected);
    }
    assert_eq!(calls.get(), 1);
}

mod shared_state {
    use super::*;
    use optionize::Schema;

    struct Object {
        value: Rc<Cell<Option<u32>>>,
        unchecked_calls: Rc<Cell<u32>>,
    }

    impl Schema<u32> for Object {
        type View<'v> = ();
        fn view<'s>(&'s self)
        where
            u32: 's,
            Self: 's,
        {
        }
    }

    impl PartialOptionized<u32> for Object {
        fn optionize(value: u32) -> Self {
            Self {
                value: Rc::new(Cell::new(Some(value))),
                unchecked_calls: Rc::new(Cell::new(0)),
            }
        }
        fn patch(self, subject: &mut u32) {
            if let Some(value) = self.value.get() {
                *subject = value;
            }
        }
        fn merge(&mut self, other: Self) {
            if other.value.get().is_some() {
                self.value.set(other.value.get());
            }
        }
    }

    impl Optionized<u32> for Object {
        type Errors = Vec<std::io::Error>;
        fn validate(&self) -> Result<(), Self::Errors> {
            self.value
                .get()
                .map(|_| ())
                .ok_or_else(|| vec![std::io::Error::other("missing value")])
        }
        unsafe fn upgrade_unchecked(self) -> u32 {
            self.unchecked_calls.set(self.unchecked_calls.get() + 1);
            self.value
                .get()
                .expect("unchecked conversion must not receive an invalid object")
        }
    }

    #[optionized]
    struct Parent {
        #[optionize(nest = Object)]
        child: u32,
        #[optionize(default = |object| {
            object.child.as_ref().unwrap().value.set(None);
            true
        })]
        enabled: bool,
    }

    #[test]
    fn defaults_cannot_bypass_nested_validation_through_interior_mutability() {
        let child = Object::optionize(7);
        let calls = child.unchecked_calls.clone();
        let patch = ParentOptional {
            child: Some(child),
            enabled: None,
        };
        patch.validate().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| patch.upgrade()));
        let panic = result
            .err()
            .expect("the callback invalidates the nested object");
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"nested object became invalid during upgrading")
        );
        assert_eq!(calls.get(), 0);
    }
}
