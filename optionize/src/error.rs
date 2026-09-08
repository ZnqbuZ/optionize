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

        writeln!(f, "Upgrade failed with {} error(s):", self.errors.len())?;

        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for error in &self.errors {
            let ty = match error {
                Error::Missing { ty, .. } => *ty,
                Error::Nested { ty, .. } => *ty,
            };
            groups.entry(ty).or_default().push(error);
        }

        let mut groups = groups.into_iter().peekable();
        while let Some((ty, errors)) = groups.next() {
            writeln!(f, "  {}", ty)?;

            let mut errors = errors.into_iter().peekable();
            while let Some(error) = errors.next() {
                let last = groups.peek().is_none() && errors.peek().is_none();

                match error {
                    Error::Missing { field, .. } => {
                        write!(f, "    - Missing required field: {}", field)?;
                    }
                    Error::Nested { field, source, .. } => {
                        writeln!(f, "    - Failed to upgrade nested field: {}", field)?;
                        write!(f, "      - {}", source)?;
                    }
                };

                if !last {
                    writeln!(f)?;
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
