#![no_std]

extern crate alloc;
extern crate self as optionize;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Display;
use delegate::delegate;
use derive_more::{Deref, DerefMut, From, Into, IntoIterator};

pub use optionize_macros::optionized;

#[doc(hidden)]
pub mod __private {
    pub extern crate alloc;

    pub use optionize_macros::Optionize;
}

pub trait PartialOptionized<Subject>: Sized {
    fn optionize(subject: Subject) -> Self;
    fn patch(self, subject: &mut Subject);
    fn merge(&mut self, other: Self);
}

pub trait Optionizable<O: PartialOptionized<Self>>: Sized {
    fn load(&mut self, other: O) {
        other.patch(self);
    }
    fn downgrade(self) -> O {
        O::optionize(self)
    }
}

impl<T, O> Optionizable<O> for T where O: PartialOptionized<T> {}

#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be upgraded to `{Subject}`",
    label = "Nested type lacks upgrade logic",
    note = "Ensure the struct `{Self}` is not partial, or is annotated with `#[optionize(partial(upgradable))]`"
)]
pub trait Optionized<Subject>: PartialOptionized<Subject> {
    type Errors: IntoIterator<Item: core::error::Error + Send + Sync + 'static>;
    fn validate(&self) -> Result<(), Self::Errors>;
    /// # Safety
    ///
    /// TODO
    unsafe fn upgrade_unchecked(self) -> Subject;
    fn upgrade(self) -> Result<Subject, Self::Errors> {
        self.validate()?;
        Ok(unsafe { self.upgrade_unchecked() })
    }
}

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
    pub original: &'static str,
    pub optionized: &'static str,
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` -> `{}`", self.optionized, self.original)
    }
}

#[derive(Debug)]
pub enum Error {
    MissingField {
        ty: TypeInfo,
        field: FieldInfo,
    },
    NestedError {
        ty: TypeInfo,
        field: FieldInfo,
        source: Box<dyn core::error::Error + Send + Sync + 'static>,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField { ty, field } => {
                write!(f, "Missing required field when upgrading {}: {}", ty, field)
            }
            Self::NestedError { ty, field, .. } => {
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
            Self::NestedError { source, .. } => Some(&**source),
            _ => None,
        }
    }
}

#[derive(Debug, Default, From, Into, Deref, DerefMut, IntoIterator)]
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
                Error::MissingField { ty, .. } => *ty,
                Error::NestedError { ty, .. } => *ty,
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
                    Error::MissingField { field, .. } => {
                        write!(f, "    - Missing required field: {}", field)?;
                    }
                    Error::NestedError { field, source, .. } => {
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

impl core::error::Error for ErrorCollection {}

impl FromIterator<Error> for ErrorCollection {
    fn from_iter<I: IntoIterator<Item = Error>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl Extend<Error> for ErrorCollection {
    delegate! {
        to self.errors {
            fn extend<T: IntoIterator<Item = Error>>(&mut self, iter: T);
        }
    }
}

impl AsRef<[Error]> for ErrorCollection {
    delegate! {
        to self.errors {
            fn as_ref(&self) -> &[Error];
        }
    }
}

impl AsMut<[Error]> for ErrorCollection {
    delegate! {
        to self.errors {
            fn as_mut(&mut self) -> &mut [Error];
        }
    }
}
