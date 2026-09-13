use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Display;
use derive_more::{AsMut, AsRef, Deref, DerefMut, Error, From, Into, IntoIterator};

#[derive(Debug)]
pub enum FieldInfo {
    Identical(&'static str),
    Renamed {
        original: &'static str,
        optionized: &'static str,
    },
}

impl Display for FieldInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identical(name) => write!(f, "`{}`", name),
            Self::Renamed {
                original,
                optionized,
            } => {
                write!(f, "optionized `{}` -> original `{}`", optionized, original)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeInfo {
    pub subject: &'static str,
    pub object: &'static str,
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` -> `{}`", self.object, self.subject)
    }
}

#[derive(Debug)]
pub enum Error {
    Missing {
        ty: TypeInfo,
        field: FieldInfo,
    },
    Nested {
        ty: TypeInfo,
        field: FieldInfo,
        source: Box<dyn core::error::Error + Send + Sync + 'static>,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { ty, field } => {
                write!(f, "Missing required field when upgrading {}: {}", ty, field)
            }
            Self::Nested { ty, field, .. } => {
                write!(
                    f,
                    "Failed to upgrade nested field when upgrading {}: {}",
                    ty, field
                )
            }
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Nested { source, .. } => Some(&**source),
            _ => None,
        }
    }
}

#[derive(Debug, Default, From, Into, Deref, DerefMut, AsRef, AsMut, IntoIterator, Error)]
#[as_ref(forward)]
#[as_mut(forward)]
#[into_iterator(owned, ref, ref_mut)]
pub struct ErrorCollection {
    pub errors: Vec<Error>,
}

impl Display for ErrorCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            return write!(f, "No upgrade errors");
        }

        write!(f, "Upgrade failed with {} error(s):", self.errors.len())?;

        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for error in &self.errors {
            let ty = match error {
                Error::Missing { ty, .. } | Error::Nested { ty, .. } => ty,
            };
            groups.entry(ty).or_default().push(error);
        }

        for (ty, errors) in groups {
            write!(f, "\n  {}", ty)?;
            for error in errors {
                match error {
                    Error::Missing { field, .. } => {
                        write!(f, "\n    - Missing required field: {}", field)?;
                    }
                    Error::Nested { field, source, .. } => {
                        writeln!(f, "\n    - Failed to upgrade nested field: {}", field)?;
                        write!(f, "      - {}", source)?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl FromIterator<Error> for ErrorCollection {
    fn from_iter<Iterable: IntoIterator<Item = Error>>(iter: Iterable) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl Extend<Error> for ErrorCollection {
    fn extend<Iterable: IntoIterator<Item = Error>>(&mut self, iter: Iterable) {
        self.errors.extend(iter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn missing(field: &'static str) -> Error {
        Error::Missing {
            ty: TypeInfo {
                subject: "Config",
                object: "ConfigOptional",
            },
            field: FieldInfo::Identical(field),
        }
    }

    #[test]
    fn error_display_includes_mapping_and_field_names() {
        let ty = TypeInfo {
            subject: "Config",
            object: "ConfigOptional",
        };
        let field = FieldInfo::Renamed {
            original: "enabled",
            optionized: "active",
        };
        assert_eq!(ty.to_string(), "`ConfigOptional` -> `Config`");
        assert_eq!(
            field.to_string(),
            "optionized `active` -> original `enabled`"
        );
        assert_eq!(
            missing("enabled").to_string(),
            "Missing required field when upgrading `ConfigOptional` -> `Config`: `enabled`"
        );
        assert_eq!(
            Error::Nested {
                ty,
                field,
                source: Box::new(missing("child")),
            }
            .to_string(),
            "Failed to upgrade nested field when upgrading `ConfigOptional` -> `Config`: optionized `active` -> original `enabled`"
        );
    }

    #[test]
    fn error_collection_groups_display_without_reordering_stored_errors() {
        let alpha = TypeInfo {
            subject: "Alpha",
            object: "AlphaOptional",
        };
        let errors: ErrorCollection = vec![
            missing("last"),
            Error::Nested {
                ty: alpha,
                field: FieldInfo::Renamed {
                    original: "nested",
                    optionized: "child",
                },
                source: Box::new(missing("value")),
            },
            Error::Missing {
                ty: alpha,
                field: FieldInfo::Identical("name"),
            },
        ]
        .into();
        assert_eq!(
            errors.to_string(),
            concat!(
                "Upgrade failed with 3 error(s):\n",
                "  `AlphaOptional` -> `Alpha`\n",
                "    - Failed to upgrade nested field: optionized `child` -> original `nested`\n",
                "      - Missing required field when upgrading `ConfigOptional` -> `Config`: `value`\n",
                "    - Missing required field: `name`\n",
                "  `ConfigOptional` -> `Config`\n",
                "    - Missing required field: `last`",
            )
        );
        assert!(matches!(
            errors.first(),
            Some(Error::Missing {
                field: FieldInfo::Identical("last"),
                ..
            })
        ));
    }

    #[test]
    fn error_collection_collects_extends_and_iterates_in_insertion_order() {
        let mut errors: ErrorCollection =
            [missing("first"), missing("second")].into_iter().collect();
        errors.extend([missing("third")]);
        let fields = (&errors)
            .into_iter()
            .map(|error| match error {
                Error::Missing { field, .. } => field.to_string(),
                Error::Nested { .. } => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(fields, ["`first`", "`second`", "`third`"]);

        for error in &mut errors {
            match error {
                Error::Missing { ty, .. } => ty.object = "RenamedOptional",
                Error::Nested { .. } => unreachable!(),
            }
        }
        let mut errors = errors.into_iter();
        for name in ["first", "second", "third"] {
            assert!(matches!(errors.next(), Some(Error::Missing {
                ty: TypeInfo { subject: "Config", object: "RenamedOptional" },
                field: FieldInfo::Identical(field),
            }) if field == name));
        }
        assert!(errors.next().is_none());
    }

    #[test]
    fn error_collection_display_describes_an_empty_collection() {
        let errors = ErrorCollection::default();
        assert!(errors.is_empty());
        assert_eq!(errors.to_string(), "No upgrade errors");
    }
}
