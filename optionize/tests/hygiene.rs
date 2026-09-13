use optionize::{Optionizable, Optionized, Retain};

mod models {
    use optionize::optionized;

    #[optionized]
    pub struct LifetimeNames<'v, 'v0> {
        first: &'v str,
        second: &'v0 str,
    }

    impl<'v, 'v0> LifetimeNames<'v, 'v0> {
        pub fn new(first: &'v str, second: &'v0 str) -> Self {
            Self { first, second }
        }

        pub fn values(&self) -> (&'v str, &'v0 str) {
            (self.first, self.second)
        }
    }
}

#[test]
fn generated_lifetimes_avoid_type_parameter_names() {
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
fn generated_lifetimes_avoid_field_and_where_clause_binders() {
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

pub mod helper_names {
    use optionize::optionized;

    #[optionized]
    #[optionize(name = "__NamedOptionizeView")]
    pub struct Named {
        pub value: u32,
    }

    #[derive(PartialEq)]
    pub struct __FieldTypeOptionizeView {
        pub value: u32,
    }

    #[optionized]
    pub struct Generic<__GenericOptionizeView, __GenericOptionizeView0, __GenericOptionizeView1> {
        pub first: __GenericOptionizeView,
        pub second: __GenericOptionizeView0,
        pub third: __GenericOptionizeView1,
    }

    #[optionized]
    pub struct FieldType {
        pub value: __FieldTypeOptionizeView,
    }

    pub struct __ReverseOptionizeView {
        pub value: u32,
    }

    #[optionized]
    #[optionize(subject = "__ReverseOptionizeView")]
    pub struct Reverse {
        pub value: Option<u32>,
    }

    pub mod string_object {
        use optionize::optionized;

        pub struct __SubjectOptionizeView {
            pub value: Option<u32>,
        }

        #[optionized]
        #[optionize(object = "__SubjectOptionizeView")]
        pub struct Subject {
            pub value: u32,
        }
    }
}

#[test]
fn generated_view_names_avoid_object_generic_and_field_type_names() {
    use helper_names::*;

    let baseline = Named { value: 1 };
    let mut patch: __NamedOptionizeView = Named { value: 1 }.downgrade();
    assert!(!patch.retain(&baseline));

    let baseline = Generic {
        first: 3_u32,
        second: "borrowed",
        third: false,
    };
    let mut patch = Generic {
        first: 3_u32,
        second: "borrowed",
        third: false,
    }
    .downgrade();
    assert!(!patch.retain(&baseline));

    let baseline = FieldType {
        value: __FieldTypeOptionizeView { value: 4 },
    };
    let mut patch = FieldType {
        value: __FieldTypeOptionizeView { value: 4 },
    }
    .downgrade();
    assert!(!patch.retain(&baseline));
}

#[test]
fn generated_view_names_avoid_paths_in_attribute_strings() {
    let baseline = helper_names::__ReverseOptionizeView { value: 5 };
    let mut patch = helper_names::Reverse { value: Some(5) };
    assert!(!patch.retain(&baseline));

    let baseline = helper_names::string_object::Subject { value: 6 };
    let mut patch = helper_names::string_object::__SubjectOptionizeView { value: Some(6) };
    assert!(!patch.retain(&baseline));
}

#[test]
fn raw_field_names_preserve_views_and_renamed_mappings() {
    use optionize::{PartialOptionized, Schema, optionized};
    use optionize_test_models as models;

    #[optionized]
    struct RawSubject {
        #[optionize(name = "{}_value")]
        r#type: u32,
    }

    #[optionized]
    #[optionize(subject = models::NestedSubject)]
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

    let mut subject = models::NestedSubject { value: 8 };
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
