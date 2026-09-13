use optionize::{Error, FieldInfo, Optionizable, Optionized, Retain, optionized};

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
fn object_paths_accept_quoted_and_bare_generic_arguments() {
    let quoted = Named {
        value: String::from("quoted"),
    }
    .downgrade();
    assert_eq!(quoted.upgrade().unwrap().value, "quoted");

    let bare = Bare {
        value: String::from("bare"),
    }
    .downgrade();
    assert_eq!(bare.upgrade().unwrap().value, "bare");
}

#[test]
fn object_paths_support_tuple_and_unit_structs() {
    assert_eq!(wire::Tuple(Some(3)).upgrade().unwrap(), Tuple(3));
    assert_eq!(wire::Empty.upgrade().unwrap(), Empty);
}

#[test]
fn object_paths_preserve_lifetime_and_const_arguments() {
    let values = [String::from("locally"), String::from("borrowed")];
    let quoted = Borrowed { values: &values }.downgrade();
    assert_eq!(quoted.upgrade().unwrap().values, &values);

    let bare = BareBorrowed { values: &values }.downgrade();
    assert_eq!(bare.upgrade().unwrap().values, &values);
}

#[test]
fn object_templates_expand_type_names_and_preserve_generic_arguments() {
    assert_eq!(
        wire::FormattedOptional { value: Some(7) }
            .upgrade()
            .unwrap()
            .value,
        7
    );

    let patch = GenericFormatted {
        value: String::from("generic template"),
    }
    .downgrade();
    assert_eq!(patch.upgrade().unwrap().value, "generic template");
}

#[test]
fn object_arguments_survive_expression_macro_forwarding() {
    let template = ForwardedTemplate {
        value: String::from("forwarded template"),
    }
    .downgrade();
    assert_eq!(template.upgrade().unwrap().value, "forwarded template");

    let path = ForwardedBare {
        value: String::from("forwarded path"),
    }
    .downgrade();
    assert_eq!(path.upgrade().unwrap().value, "forwarded path");
}

#[test]
fn validate_reports_qualified_object_paths() {
    for (errors, object) in [
        (
            wire::Named::<u32> { value: None }.validate().unwrap_err(),
            "crate::wire::Named<Value>",
        ),
        (
            wire::Bare::<u32> { value: None }.validate().unwrap_err(),
            "crate::wire::Bare::<Value>",
        ),
    ] {
        let [
            Error::Missing {
                ty,
                field: FieldInfo::Identical("value"),
            },
        ] = errors.errors.as_slice()
        else {
            panic!("unexpected errors: {errors:?}");
        };
        assert_eq!(ty.object.replace(' ', ""), object);
    }
}

#[test]
fn object_paths_accept_quoted_and_bare_associated_type_projections() {
    trait Model {
        type Object;
    }

    struct Family;

    struct Patch {
        value: Option<u32>,
    }

    impl Model for Family {
        type Object = Patch;
    }

    #[optionized]
    #[optionize(object = "<Family as Model>::Object")]
    struct Quoted {
        value: u32,
    }

    #[optionized]
    #[optionize(object = <Family as Model>::Object)]
    struct Bare {
        value: u32,
    }

    let baseline: Quoted = Patch { value: Some(7) }.upgrade().unwrap();
    assert_eq!(baseline.value, 7);
    let mut patch = Quoted { value: 7 }.downgrade();
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, None);

    let baseline: Bare = Patch { value: Some(9) }.upgrade().unwrap();
    assert_eq!(baseline.value, 9);
    let mut patch = Bare { value: 9 }.downgrade();
    assert!(!patch.retain(&baseline));
    assert_eq!(patch.value, None);
}
