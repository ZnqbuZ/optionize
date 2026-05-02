#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use core::error::Error;
use core::fmt;
use core::fmt::Display;

#[doc(hidden)]
pub mod __private {
    pub extern crate alloc;
}

pub trait PartialOptionized: Sized {
    type Subject;
    fn optionize(subject: Self::Subject) -> Self;
    fn patch(self, subject: &mut Self::Subject);
    fn merge(&mut self, other: Self);
}

pub trait Optionizable<O: PartialOptionized<Subject = Self>>: Sized
where
    (): Sized,
{
    fn load(&mut self, other: O) {
        other.patch(self);
    }
    fn downgrade(self) -> O {
        O::optionize(self)
    }
}

impl<T, O> Optionizable<O> for T where O: PartialOptionized<Subject = T> {}

#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be upgraded to `{Subject}`",
    label = "Nested type lacks upgrade logic",
    note = "Ensure the struct `{Self}` is not partial, or is annotated with `#[optionize(partial(upgradable))]`"
)]
pub trait Upgradable<Subject>: Sized {
    type Error: IntoIterator;
    fn upgrade(self) -> Result<Subject, (Self::Error, Self)>;
}

pub trait Optionized: PartialOptionized + Upgradable<Self::Subject> {}

impl<O> Optionized for O where O: PartialOptionized + Upgradable<Self::Subject> {}

#[derive(Debug)]
pub enum FieldName {
    Identical(&'static str),
    Renamed {
        original: &'static str,
        optionized: &'static str,
    },
}

impl Display for FieldName {
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

#[derive(Debug)]
pub enum UpgradeError {
    MissingField(FieldName),
    NestedError {
        field: FieldName,
        source: Box<dyn Error + 'static>,
    },
}

impl Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => {
                write!(f, "Missing required field for upgrade: {}", field)
            }
            Self::NestedError { field, .. } => {
                write!(f, "Failed to upgrade nested field: {}", field)
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
