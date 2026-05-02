#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::error::Error;
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
    type UpgradeErrors: IntoIterator;
    fn upgrade(self) -> Result<Subject, (Self::UpgradeErrors, Self)>;
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
pub enum UpgradeError {
    MissingField {
        ty: TypeInfo,
        field: FieldInfo,
    },
    NestedError {
        ty: TypeInfo,
        field: FieldInfo,
        source: Box<dyn Error + Send + Sync + 'static>,
    },
}

impl Display for UpgradeError {
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

impl Error for UpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NestedError { source, .. } => Some(&**source),
            _ => None,
        }
    }
}

#[derive(Debug, Default, From, Into, Deref, DerefMut, IntoIterator)]
#[into_iterator(owned, ref, ref_mut)]
pub struct UpgradeErrorCollection {
    pub errors: Vec<UpgradeError>,
}

impl Display for UpgradeErrorCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            return write!(f, "No upgrade errors");
        }

        writeln!(f, "Upgrade failed with {} error(s):", self.errors.len())?;

        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for error in &self.errors {
            let ty = match error {
                UpgradeError::MissingField { ty, .. } => *ty,
                UpgradeError::NestedError { ty, .. } => *ty,
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
                    UpgradeError::MissingField { field, .. } => {
                        write!(f, "    - Missing required field: {}", field)?;
                    }
                    UpgradeError::NestedError { field, source, .. } => {
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

impl Error for UpgradeErrorCollection {}

impl FromIterator<UpgradeError> for UpgradeErrorCollection {
    fn from_iter<I: IntoIterator<Item = UpgradeError>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl Extend<UpgradeError> for UpgradeErrorCollection {
    delegate! {
        to self.errors {
            fn extend<T: IntoIterator<Item = UpgradeError>>(&mut self, iter: T);
        }
    }
}

impl AsRef<[UpgradeError]> for UpgradeErrorCollection {
    delegate! {
        to self.errors {
            fn as_ref(&self) -> &[UpgradeError];
        }
    }
}

impl AsMut<[UpgradeError]> for UpgradeErrorCollection {
    delegate! {
        to self.errors {
            fn as_mut(&mut self) -> &mut [UpgradeError];
        }
    }
}
