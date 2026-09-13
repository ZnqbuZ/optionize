use optionize::{Optionizable, Optionized, Retain};

pub mod models {
    use optionize::optionized;

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
}

#[test]
fn retain_supports_public_subjects_with_private_field_types() {
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
fn partial_operations_support_private_fields_without_equality() {
    let patch = models::Uncomparable::new(3).downgrade();
    patch.validate().unwrap();
    let mut baseline = patch.upgrade().unwrap();
    assert_eq!(baseline.value(), 3);

    baseline.load(models::Uncomparable::new(4).downgrade());
    assert_eq!(baseline.value(), 4);
}

#[test]
fn retain_preserves_local_lifetimes_of_private_field_types() {
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
fn retain_supports_private_nested_fields_of_public_subjects() {
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
