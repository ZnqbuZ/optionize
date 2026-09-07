use optionize::{Optionizable, Optionized, PartialOptionized, optionized};

mod wire {
    #[derive(Debug, PartialEq)]
    pub struct Named<T> {
        pub value: Option<T>,
    }

    #[derive(Debug, PartialEq)]
    pub struct Tuple<T>(pub Option<T>);

    pub struct Empty;

    pub struct Borrowed<'a, T, const N: usize> {
        pub values: Option<&'a [T; N]>,
    }

    pub struct FormattedOptional {
        pub value: Option<u32>,
    }
}

#[optionized]
#[optionize(object = "crate::wire::Named<T>")]
#[derive(Debug, PartialEq)]
pub struct Named<T> {
    value: T,
}

#[optionized]
#[optionize(object = "self::wire::Tuple<T>")]
#[derive(Debug, PartialEq)]
pub struct Tuple<T>(T);

#[optionized]
#[optionize(object = "wire::Empty")]
#[derive(Debug, PartialEq)]
pub struct Empty;

#[optionized]
#[optionize(object = "wire::Borrowed<'a, T, N>")]
pub struct Borrowed<'a, T, const N: usize> {
    values: &'a [T; N],
}

#[optionized]
#[optionize(object = "wire::{}Optional")]
pub struct Formatted {
    value: u32,
}

#[test]
fn named_generic_path_supports_all_operations() {
    let mut value = Named {
        value: String::from("old"),
    };
    let mut patch = wire::Named { value: None };
    patch.merge(wire::Named {
        value: Some(String::from("new")),
    });
    value.load(patch);
    assert_eq!(value.value, "new");
    let patch: wire::Named<String> = value.downgrade();
    assert!(patch.validate().is_ok());
    assert_eq!(patch.upgrade().unwrap().value, "new");

    let errors = wire::Named::<u32> { value: None }.validate().unwrap_err();
    let error = errors.into_iter().next().unwrap().to_string();
    assert!(error.contains("wire"), "{error}");
    assert!(error.contains("value"), "{error}");
}

#[test]
fn tuple_and_unit_paths_upgrade() {
    assert_eq!(wire::Tuple(Some(3)).upgrade().unwrap(), Tuple(3));
    assert_eq!(wire::Empty.upgrade().unwrap(), Empty);
}

#[test]
fn lifetime_and_const_arguments_are_preserved() {
    let values = [1, 2, 3];
    let patch = Borrowed { values: &values }.downgrade();
    assert_eq!(patch.upgrade().unwrap().values, &values);
}

#[test]
fn object_path_retains_name_placeholder() {
    assert_eq!(
        wire::FormattedOptional { value: Some(7) }
            .upgrade()
            .unwrap()
            .value,
        7
    );
}
