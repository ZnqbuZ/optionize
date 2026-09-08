use optionize::{Optionizable, Optionized, Retain};

pub mod models {
    use core::marker::PhantomData;

    use optionize::optionized;
    use optionize_test_models as proto;

    #[derive(PartialEq)]
    struct PrivateEq(u32);

    #[optionized]
    pub struct Comparable {
        value: PrivateEq,
    }

    impl Comparable {
        pub fn new(value: u32) -> Self {
            Self {
                value: PrivateEq(value),
            }
        }

        pub fn value(&self) -> u32 {
            self.value.0
        }
    }

    struct PrivateNoEq(u32);

    #[optionized]
    pub struct Uncomparable {
        value: PrivateNoEq,
    }

    impl Uncomparable {
        pub fn new(value: u32) -> Self {
            Self {
                value: PrivateNoEq(value),
            }
        }

        pub fn value(&self) -> u32 {
            self.value.0
        }
    }

    #[derive(PartialEq)]
    struct PrivateBorrowed<'s>(&'s str);

    #[optionized]
    pub struct Borrowed<'s> {
        value: PrivateBorrowed<'s>,
    }

    impl<'s> Borrowed<'s> {
        pub fn new(value: &'s str) -> Self {
            Self {
                value: PrivateBorrowed(value),
            }
        }

        pub fn value(&self) -> &'s str {
            self.value.0
        }
    }

    #[optionized]
    struct PrivateInner {
        value: u32,
    }

    #[optionized]
    pub struct Nested {
        #[optionize(nest = PrivateInnerOptional)]
        value: PrivateInner,
        #[optionize(flatten, nest = PrivateInnerOptional)]
        flattened: PrivateInner,
    }

    impl Nested {
        pub fn new(value: u32, flattened: u32) -> Self {
            Self {
                value: PrivateInner { value },
                flattened: PrivateInner { value: flattened },
            }
        }

        pub fn values(&self) -> (u32, u32) {
            (self.value.value, self.flattened.value)
        }
    }

    #[optionized]
    pub struct LifetimeNames<'v, 'v_> {
        first: &'v str,
        second: &'v_ str,
    }

    impl<'v, 'v_> LifetimeNames<'v, 'v_> {
        pub fn new(first: &'v str, second: &'v_ str) -> Self {
            Self { first, second }
        }

        pub fn values(&self) -> (&'v str, &'v_ str) {
            (self.first, self.second)
        }
    }

    #[optionized]
    #[optionize(subject = proto::GenericSubject::<Value>, partial(marked, upgradable))]
    pub struct ReverseMarked<Value> {
        value: Option<Value>,
    }

    impl<Value> ReverseMarked<Value> {
        pub fn new(value: Option<Value>) -> Self {
            Self {
                value,
                _marker: PhantomData,
            }
        }
    }

    #[optionized]
    #[optionize(subject = proto::GenericSubject::<Value>, partial(marked, upgradable))]
    pub struct ReverseSkipped<Value: Default> {
        #[optionize(skip)]
        value: Value,
    }

    #[optionized]
    #[optionize(subject = proto::UnitSubject, partial(marked, upgradable))]
    pub struct ReverseMarkedUnit;
}

#[test]
fn public_subjects_with_private_equality_types_support_retain() {
    let mut baseline = models::Comparable::new(1);
    let mut equal = models::Comparable::new(1).downgrade();
    assert!(!equal.retain(&baseline));
    baseline.load(equal);
    assert_eq!(baseline.value(), 1);

    let mut changed = models::Comparable::new(2).downgrade();
    assert!(changed.retain(&baseline));
    baseline.load(changed);
    assert_eq!(baseline.value(), 2);
}

#[test]
fn private_non_equality_types_preserve_normal_operations() {
    let patch = models::Uncomparable::new(3).downgrade();
    patch.validate().unwrap();
    let mut baseline = patch.upgrade().unwrap();
    assert_eq!(baseline.value(), 3);

    baseline.load(models::Uncomparable::new(4).downgrade());
    assert_eq!(baseline.value(), 4);
}

#[test]
fn private_borrowed_types_keep_their_local_lifetimes() {
    let value = String::from("private local borrow");
    let baseline = models::Borrowed::new(&value);
    let mut patch = models::Borrowed::new(&value).downgrade();
    assert!(!patch.retain(&baseline));
    assert_eq!(baseline.value(), "private local borrow");

    let different = String::from("a different borrow");
    let mut patch = models::Borrowed::new(&different).downgrade();
    assert!(patch.retain(&baseline));
    let changed = patch.upgrade().unwrap();
    assert_eq!(changed.value(), "a different borrow");
}

#[test]
fn private_nested_types_are_usable_through_public_parent_operations() {
    let mut baseline = models::Nested::new(5, 6);
    let mut equal = models::Nested::new(5, 6).downgrade();
    assert!(!equal.retain(&baseline));
    baseline.load(equal);
    assert_eq!(baseline.values(), (5, 6));

    let mut changed = models::Nested::new(7, 6).downgrade();
    assert!(changed.retain(&baseline));
    baseline.load(changed);
    assert_eq!(baseline.values(), (7, 6));
}

#[test]
fn generated_borrow_lifetimes_avoid_user_names() {
    let first = String::from("first local borrow");
    let second = String::from("second local borrow");
    let baseline = models::LifetimeNames::new(&first, &second);
    let mut patch = models::LifetimeNames::new(&first, &second).downgrade();
    assert!(!patch.retain(&baseline));
    assert_eq!(baseline.values(), (first.as_str(), second.as_str()));

    let patch = models::LifetimeNames::new(&first, &second).downgrade();
    patch.validate().unwrap();
    assert_eq!(
        patch.upgrade().unwrap().values(),
        (first.as_str(), second.as_str())
    );
}

#[test]
fn generated_borrow_lifetimes_avoid_field_and_where_clause_binders() {
    use optionize::{PartialOptionized, Schema, optionized};

    #[optionized]
    #[optionize(partial(marked))]
    struct CallbackConfig<Callback>
    where
        Callback: for<'s> Fn(&'s str),
    {
        #[optionize(skip)]
        first: for<'v> fn(&'v str),
        #[optionize(skip)]
        second: Callback,
        value: u32,
    }

    fn assert_first(value: &str) {
        assert_eq!(value, "first");
    }

    fn assert_second(value: &str) {
        assert_eq!(value, "second");
    }

    let first = String::from("first");
    let second = String::from("second");
    let mut baseline = CallbackConfig {
        first: assert_first,
        second: assert_second,
        value: 1,
    };
    let view = baseline.view();
    assert_eq!(view.v_value, Some(&1));
    view.v_first.unwrap()(&first);
    view.v_second.unwrap()(&second);

    let patch = CallbackConfig {
        first: assert_second,
        second: assert_second,
        value: 2,
    }
    .downgrade();
    let view = patch.view();
    assert_eq!(view.v_value, Some(&2));
    assert!(view.v_first.is_none());
    assert!(view.v_second.is_none());
    patch.patch(&mut baseline);
    assert_eq!(baseline.value, 2);
    (baseline.first)(&first);
    (baseline.second)(&second);
}

#[test]
fn reverse_mappings_can_inject_markers_into_the_declared_object() {
    use optionize_test_models as proto;

    let baseline = proto::GenericSubject { value: 8_u32 };
    let mut patch = models::ReverseMarked::new(Some(8));
    assert!(!patch.retain(&baseline));

    let patch: models::ReverseMarked<u32> = baseline.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, 8);

    let baseline = proto::GenericSubject { value: 9_u32 };
    let mut patch: models::ReverseSkipped<u32> = proto::GenericSubject { value: 1_u32 }.downgrade();
    assert!(!patch.retain(&baseline));
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, 0);
}

#[test]
fn reverse_unit_mappings_can_be_marked_and_upgraded() {
    use optionize_test_models as proto;

    let mut patch: models::ReverseMarkedUnit = proto::UnitSubject.downgrade();
    assert!(!patch.retain(&proto::UnitSubject));
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap(), proto::UnitSubject);
}

pub mod helper_names {
    use optionize::optionized;

    #[optionized]
    #[optionize(name = "__OptionizeNamedView")]
    pub struct Named {
        pub value: u32,
    }

    #[derive(PartialEq)]
    pub struct __OptionizeFieldTypeView {
        pub value: u32,
    }

    #[optionized]
    pub struct Generic<__OptionizeGenericView, __OptionizeGenericView_> {
        pub first: __OptionizeGenericView,
        pub second: __OptionizeGenericView_,
    }

    #[optionized]
    pub struct FieldType {
        pub value: __OptionizeFieldTypeView,
    }

    pub struct __OptionizeReverseView {
        pub value: u32,
    }

    #[optionized]
    #[optionize(subject = "__OptionizeReverseView")]
    pub struct Reverse {
        pub value: Option<u32>,
    }

    pub mod string_object {
        use optionize::optionized;

        pub struct __OptionizeSubjectView {
            pub value: Option<u32>,
        }

        #[optionized]
        #[optionize(object = "__OptionizeSubjectView")]
        pub struct Subject {
            pub value: u32,
        }
    }
}

#[test]
fn generated_helper_types_avoid_object_parameter_and_field_type_names() {
    use helper_names::*;

    let baseline = Named { value: 1 };
    let mut patch: __OptionizeNamedView = Named { value: 1 }.downgrade();
    assert!(!patch.retain(&baseline));

    let baseline = Generic {
        first: 3_u32,
        second: "borrowed",
    };
    let mut patch = Generic {
        first: 3_u32,
        second: "borrowed",
    }
    .downgrade();
    assert!(!patch.retain(&baseline));

    let baseline = FieldType {
        value: __OptionizeFieldTypeView { value: 4 },
    };
    let mut patch = FieldType {
        value: __OptionizeFieldTypeView { value: 4 },
    }
    .downgrade();
    assert!(!patch.retain(&baseline));
}

#[test]
fn generated_helper_types_avoid_paths_supplied_inside_attribute_strings() {
    let baseline = helper_names::__OptionizeReverseView { value: 5 };
    let mut patch = helper_names::Reverse { value: Some(5) };
    assert!(!patch.retain(&baseline));

    let baseline = helper_names::string_object::Subject { value: 6 };
    let mut patch = helper_names::string_object::__OptionizeSubjectView { value: Some(6) };
    assert!(!patch.retain(&baseline));
}

#[test]
fn raw_field_names_preserve_subject_names_in_views_and_renamed_mappings() {
    use optionize::{PartialOptionized, Schema, optionized};
    use optionize_test_models as proto;

    #[optionized]
    struct RawSubject {
        #[optionize(name = "{}_value")]
        r#type: u32,
    }

    #[optionized]
    #[optionize(subject = proto::NestedSubject)]
    struct RawPatch {
        #[optionize(name = "value")]
        r#type: Option<u32>,
    }

    let patch = RawSubject { r#type: 7 }.downgrade();
    assert_eq!(patch.type_value, Some(7));
    assert_eq!(patch.view().v_type, Some(&7));
    let subject = patch.upgrade().unwrap();
    assert_eq!(subject.r#type, 7);
    assert_eq!(subject.view().v_type, Some(&7));

    let mut subject = proto::NestedSubject { value: 8 };
    let mut patch = RawPatch { r#type: Some(8) };
    assert_eq!(subject.view().v_value, Some(&8));
    assert_eq!(patch.view().v_value, Some(&8));
    assert!(!patch.retain(&subject));
    assert!(patch.r#type.is_none());
    patch.r#type = Some(9);
    patch.patch(&mut subject);
    assert_eq!(subject.value, 9);

    let patch: RawPatch = subject.downgrade();
    assert_eq!(patch.r#type, Some(9));
    assert_eq!(patch.upgrade().unwrap().value, 9);
}
