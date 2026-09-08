use optionize::{Optionizable, Optionized, PartialOptionized, optionized};

mod wire {
    #[derive(Debug, PartialEq)]
    pub struct Named<Value> {
        pub value: Option<Value>,
    }

    #[derive(Debug, PartialEq)]
    pub struct Tuple<Value>(pub Option<Value>);

    pub struct Empty;

    pub struct Borrowed<'v, Value, const N: usize> {
        pub values: Option<&'v [Value; N]>,
    }

    pub struct FormattedOptional {
        pub value: Option<u32>,
    }

    pub struct Bare<Value> {
        pub value: Option<Value>,
    }

    pub struct BareBorrowed<'v, Value, const N: usize> {
        pub values: Option<&'v [Value; N]>,
    }

    pub struct GenericFormattedOp<Value> {
        pub value: Option<Value>,
    }

    pub struct ForwardedTemplateOp<Value> {
        pub value: Option<Value>,
    }

    pub struct ForwardedPath<Value> {
        pub value: Option<Value>,
    }
}

#[optionized]
#[optionize(object = "crate::wire::Named<Value>")]
#[derive(Debug, PartialEq)]
pub struct Named<Value> {
    value: Value,
}

#[optionized]
#[optionize(object = "self::wire::Tuple<Value>")]
#[derive(Debug, PartialEq)]
pub struct Tuple<Value>(Value);

#[optionized]
#[optionize(object = "wire::Empty")]
#[derive(Debug, PartialEq)]
pub struct Empty;

#[optionized]
#[optionize(object = "wire::Borrowed<'v, Value, N>")]
pub struct Borrowed<'v, Value, const N: usize> {
    values: &'v [Value; N],
}

#[optionized]
#[optionize(object = "wire::{}Optional")]
pub struct Formatted {
    value: u32,
}

#[optionized]
#[optionize(object = crate::wire::Bare::<Value>)]
pub struct Bare<Value> {
    value: Value,
}

#[optionized]
#[optionize(object = wire::BareBorrowed::<'v, Value, N>)]
pub struct BareBorrowed<'v, Value, const N: usize> {
    values: &'v [Value; N],
}

#[optionized]
#[optionize(object = "wire::{}Op<Value>")]
pub struct GenericFormatted<Value> {
    value: Value,
}

macro_rules! forwarded_objects {
    ($($name:ident => $object:expr),+ $(,)?) => {
        $(
            #[optionized]
            #[optionize(object = $object)]
            pub struct $name<Value> {
                value: Value,
            }
        )+
    };
}

forwarded_objects! {
    ForwardedTemplate => "wire::{}Op<Value>",
    ForwardedBare => wire::ForwardedPath::<Value>,
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

#[test]
fn bare_generic_path_supports_all_operations() {
    let mut full = Bare {
        value: String::from("old"),
    };
    let mut patch = wire::Bare { value: None };
    patch.merge(wire::Bare {
        value: Some(String::from("new")),
    });
    full.load(patch);
    assert_eq!(full.value, "new");

    let patch: wire::Bare<String> = full.downgrade();
    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, "new");
}

#[test]
fn bare_paths_preserve_lifetime_and_const_arguments() {
    let values = [5, 6, 7];
    let patch = BareBorrowed { values: &values }.downgrade();

    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().values, &values);
}

#[test]
fn string_templates_preserve_generic_arguments() {
    let patch = GenericFormatted {
        value: String::from("generic template"),
    }
    .downgrade();

    patch.validate().unwrap();
    assert_eq!(patch.upgrade().unwrap().value, "generic template");
}

#[test]
fn macro_expression_forwarding_accepts_templates_and_bare_paths() {
    let template = ForwardedTemplate {
        value: String::from("forwarded template"),
    }
    .downgrade();
    template.validate().unwrap();
    assert_eq!(template.upgrade().unwrap().value, "forwarded template");

    let path = ForwardedBare {
        value: String::from("forwarded path"),
    }
    .downgrade();
    path.validate().unwrap();
    assert_eq!(path.upgrade().unwrap().value, "forwarded path");
}
