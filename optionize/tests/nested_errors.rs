use optionize::{Error, Optionized, PartialOptionized, Schema, optionized};

struct Object<'o>(&'o str);

#[derive(Debug)]
struct Errors<'e>(&'e str);

impl IntoIterator for Errors<'_> {
    type Item = std::io::Error;
    type IntoIter = std::iter::Once<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(std::io::Error::other(self.0.to_owned()))
    }
}

impl<'s> PartialOptionized<&'s str> for Object<'s> {
    fn optionize(subject: &'s str) -> Self {
        Self(subject)
    }

    fn patch(self, subject: &mut &'s str) {
        *subject = self.0;
    }

    fn merge(&mut self, other: Self) {
        *self = other;
    }
}

impl<'s> Schema<&'s str> for Object<'s> {
    type View<'v>
        = &'s str
    where
        Self: 'v;

    fn view<'o>(&'o self) -> Self::View<'o>
    where
        &'s str: 'o,
    {
        self.0
    }
}

impl<'s> Optionized<&'s str> for Object<'s> {
    type Errors = Errors<'s>;

    fn validate(&self) -> Result<(), Self::Errors> {
        if self.0.starts_with('!') {
            Err(Errors(self.0))
        } else {
            Ok(())
        }
    }

    unsafe fn upgrade_unchecked(self) -> &'s str {
        self.0
    }
}

#[optionized]
struct Parent<'p> {
    #[optionize(nest = "Object<'p>")]
    wrapped: &'p str,
    #[optionize(flatten, nest = "Object<'p>")]
    flattened: &'p str,
}

#[test]
fn nested_errors_outlive_the_borrowed_collection() {
    let errors = {
        let input = String::from("!invalid");
        let object = ParentOptional {
            wrapped: Some(Object(&input)),
            flattened: Object(&input),
        };
        object.validate().unwrap_err()
    };
    assert_eq!(errors.len(), 2);
    for error in errors {
        let Error::Nested { source, .. } = error else {
            panic!("expected a nested validation error");
        };
        assert_eq!(source.to_string(), "!invalid");
    }
}

#[test]
fn nested_upgrade_accepts_objects_with_borrowed_error_collections() {
    let input = String::from("valid");
    let object = ParentOptional {
        wrapped: Some(Object(&input)),
        flattened: Object(&input),
    };
    let subject = object.upgrade().unwrap();
    assert_eq!(subject.wrapped, input);
    assert_eq!(subject.flattened, input);
}
