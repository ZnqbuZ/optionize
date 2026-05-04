//! `optionize` is a procedural macro to generate optionized versions of structs for partial configurations, updates, and builders.
//!
//! The generated structs wrap their fields in `Option<T>` (unless explicitly overridden), allowing you to define partial state.
//! It also generates corresponding implementations for parsing, merging, downgrading, and upgrading to the original struct.
//!
//! ## Table of Contents
//! - [Basic Usage](#basic-usage)
//! - [Struct-level Attributes](#struct-level-attributes)
//!   - [Renaming the generated struct](#renaming-the-generated-struct)
//!   - [Overriding derived attributes](#overriding-derived-attributes)
//!   - [Upgrading and the `partial` attribute](#upgrading-and-the-partial-attribute)
//!   - [Generics and the `marked` attribute](#generics-and-the-marked-attribute)
//! - [Field-level Attributes](#field-level-attributes)
//!   - [Renaming fields](#renaming-fields)
//!   - [Overriding field attributes](#overriding-field-attributes)
//!   - [Flattening fields (Disabling `Option<T>` wrapping)](#flattening-fields)
//!   - [Skipping fields during upgrade](#skipping-fields-during-upgrade)
//!   - [Nesting other optionized structs](#nesting-other-optionized-structs)
//! - [Trait Operations](#trait-operations)
//!   - [Downgrading & Loading](#downgrading--loading)
//!   - [Patching & Merging](#patching--merging)
//!   - [Validation & Upgrading](#validation--upgrading)
//! - [Validation Errors](#validation-errors)
//!
//! ---
//!
//! ## Basic Usage
//! The simplest use case wraps every field in an `Option`, allowing you to create partial representations.
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[derive(Debug, PartialEq)]
//! struct Wrap {
//!     _0: i32,
//!     _1: i32,
//! }
//!
//! let wrap = Wrap {
//!     _0: 0,
//!     _1: 1,
//! };
//!
//! // The generated struct is named `WrapOptional` by default.
//! let wrap_optional = WrapOptional {
//!     _0: Some(10),
//!     _1: Some(11),
//! };
//! ```
//!
//! ---
//!
//! ## Struct-level Attributes
//!
//! ### Renaming the generated struct
//! By default, the generated struct's name appends `Optional` to the original name. You can customize this using `name = "..."`.
//! `{}` acts as a placeholder for the original struct name.
//! ```rust
//! use optionize::optionized;
//!
//! #[optionized]
//! #[optionize(name = "{}Builder")]
//! struct Config {
//!     port: u16,
//! }
//!
//! let builder = ConfigBuilder { port: Some(8080) };
//! ```
//!
//! ### Overriding derived attributes
//! The generated struct inherits all attributes (e.g., `#[derive(...)]`) from the original struct by default.
//! Using `attrs(...)` allows you to **completely override** the attributes on the generated struct.
//! ```rust
//! use optionize::optionized;
//!
//! #[optionized]
//! // The generated struct will derive Clone and Debug, but NOT PartialEq.
//! #[optionize(attrs(derive(Clone, Debug)))]
//! #[derive(Debug, PartialEq)]
//! struct User {
//!     id: u32,
//! }
//! ```
//!
//! ### Upgrading and the `partial` attribute
//! To support converting the partial struct back into the full struct, you must specify `partial(upgradable)`.
//! This implements the `Optionized` trait, providing `.validate()` and `.upgrade()` methods.
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(partial(upgradable))]
//! #[derive(Debug, PartialEq)]
//! struct Data {
//!     value: i32,
//! }
//!
//! let partial = DataOptional { value: Some(42) };
//! assert_eq!(partial.upgrade().unwrap().value, 42);
//! ```
//!
//! ### Generics and the `marked` attribute
//! If you use `skip` on a field that uses a generic type, that generic type becomes unused in the generated struct, resulting in a compilation error.
//! To fix this, use `marked` to inject a `PhantomData` field that consumes the generic type.
//! ```rust
//! use optionize::{optionized, Optionized};
//! use std::marker::PhantomData;
//!
//! #[optionized]
//! #[optionize(partial(marked(name = _marker), upgradable))]
//! #[derive(Debug)]
//! struct Generic<D: Clone + Default> {
//!     #[optionize(skip)]
//!     data: D,
//! }
//!
//! let generic_optional = GenericOptional::<i32> { _marker: PhantomData };
//! assert!(generic_optional.validate().is_ok());
//! ```
//! *Note: For unit structs, using `marked` changes the generated struct to a tuple struct, or a named struct if `marked(name = ...)` is provided.*
//!
//! ---
//!
//! ## Field-level Attributes
//!
//! ### Renaming fields
//! Use `name` to change a field's name in the generated struct. `{}` acts as a placeholder for the original name or index.
//! ```rust
//! use optionize::optionized;
//!
//! #[optionized]
//! struct RenameExample {
//!     #[optionize(name = "renamed_value")]
//!     value: i32,
//! }
//! ```
//!
//! ### Overriding field attributes
//! Similar to struct-level `attrs`, you can override attributes on specific fields.
//! ```rust
//! use optionize::optionized;
//!
//! #[optionized]
//! struct FieldAttrExample {
//!     #[optionize(attrs(doc = "This is a custom doc string for the partial struct field"))]
//!     value: i32,
//! }
//! ```
//!
//! ### Flattening fields
//! `flatten` prevents the macro from wrapping the field's type in `Option<T>`. The field will have the exact same type in the generated struct.
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[derive(Debug, PartialEq)]
//! struct FlattenExample {
//!     #[optionize(flatten)]
//!     id: u32,
//!     name: String,
//! }
//!
//! let partial = FlattenExampleOptional {
//!     id: 1, // Notice `id` is `u32`, not `Option<u32>`
//!     name: Some("Alice".into()),
//! };
//! ```
//!
//! ### Skipping fields during upgrade
//! `skip` removes the field entirely from the generated struct.
//! Because a skipped field cannot be supplied during an upgrade, the struct must be `upgradable`.
//! You can optionally provide a default value via `upgrade = expr`; otherwise, it uses `Default::default()`.
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(partial(upgradable))]
//! #[derive(Debug, PartialEq)]
//! struct SkipExample {
//!     #[optionize(skip(upgrade = 123))]
//!     id: i32,
//! }
//!
//! let partial = SkipExampleOptional {};
//! assert_eq!(partial.upgrade().unwrap().id, 123);
//! ```
//!
//! ### Nesting other optionized structs
//! For complex configurations, you can delegate optionization to nested types using `nest`.
//! ```rust
//! use optionize::{optionized, Optionized, PartialOptionized};
//!
//! #[optionized]
//! #[derive(Debug, Clone, PartialEq)]
//! #[optionize(partial(upgradable))]
//! struct Inner {
//!     x: u32,
//! }
//!
//! #[optionized]
//! #[derive(Debug, Clone)]
//! struct Outer {
//!     #[optionize(nest = "InnerOptional")]
//!     a: Inner,
//! }
//!
//! let mut outer = OuterOptional {
//!     a: Some(InnerOptional { x: Some(1) }),
//! };
//! ```
//!
//! ---
//!
//! ## Trait Operations
//! The macro generates implementations for `PartialOptionized` and (optionally) `Optionized`. Extension methods are provided via `Optionizable`.
//!
//! ### Downgrading & Loading
//! You can convert a full struct into its partial version (`downgrade`), or load partial data into a full struct (`load`).
//! ```rust
//! use optionize::{optionized, Optionizable};
//!
//! #[optionized]
//! #[derive(Debug, PartialEq)]
//! struct User {
//!     id: u32,
//!     name: String,
//! }
//!
//! let mut user = User { id: 1, name: "Alice".into() };
//!
//! // Downgrade to partial
//! let partial = user.downgrade();
//! assert_eq!(partial.id, Some(1));
//!
//! // Load from partial
//! let mut updated_user = User { id: 2, name: "Bob".into() };
//! updated_user.load(partial);
//! assert_eq!(updated_user.name, "Alice");
//! ```
//!
//! ### Patching & Merging
//! Combine partial states using `.merge(other)` or apply them to a full state using `.patch(subject)`.
//! ```rust
//! use optionize::{optionized, PartialOptionized};
//!
//! #[optionized]
//! struct Merging { a: u32, b: u32 }
//!
//! let mut p1 = MergingOptional { a: Some(1), b: None };
//! let p2 = MergingOptional { a: Some(2), b: Some(3) };
//!
//! p1.merge(p2);
//! assert_eq!(p1.a, Some(2)); // Overwritten
//! assert_eq!(p1.b, Some(3)); // Filled
//! ```
//!
//! ### Validation & Upgrading
//! Call `.validate()` to ensure all required fields are present. Call `.upgrade()` to convert it to the original type.
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(partial(upgradable))]
//! struct Validated { req: String }
//!
//! let p = ValidatedOptional { req: None };
//! assert!(p.validate().is_err());
//! ```

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

/// Generates an optionized version of a struct, replacing its fields with `Option<T>` where applicable,
/// and implements conversion and merge logic for partial updates and builders.
///
/// This macro generates a new struct (by default named `{OriginalName}Optional`) and implements the `PartialOptionized`
/// trait to allow seamless conversion, patching, and merging between the full and the partial struct.
///
/// ## Struct-level attributes
///
/// `#[optionize(name = "...", attrs(...), partial(...))]`
///
/// - `name`: Overrides the generated struct's name. Use `{}` as a placeholder for the original struct name.
/// - `attrs`: By default, the generated struct inherits all attributes from the original struct (except `#[optionize(...)]`).
///   If you provide `attrs(...)`, it **completely overrides** this behavior. You must list all attributes the generated struct should have.
///   For example, `#[optionize(attrs(derive(Debug)))]` makes the generated struct *only* derive `Debug`.
/// - `partial`: Configures the generated struct's upgrade and validation behavior.
///   - `upgradable`: Implements the `Optionized` trait, allowing the partial struct to be validated and "upgraded" to the full struct.
///   - `marked`: If the original struct has type parameters or lifetimes that are not used by the generated struct (e.g., due to skipped fields),
///     the generated struct will fail to compile. `marked` injects a `PhantomData` field to consume those generic parameters.
///     - For unit structs, this changes the generated struct to a tuple struct (or a named struct if `name` is specified).
///     - Use `marked(name = my_marker)` to explicitly name the injected `PhantomData` field.
///
/// ## Field-level attributes
///
/// `#[optionize(name = "...", attrs(...), flatten, skip(...), nest = "...")]`
///
/// - `name`: Renames the field in the generated struct. Use `{}` as a placeholder for the original field name or index.
/// - `attrs`: Similarly to the struct-level `attrs`, this overrides the attributes applied to the generated field.
/// - `flatten`: Instructs the macro **not** to wrap the field's type in `Option<T>`. The field will have the exact same type in the generated struct.
/// - `skip`: Removes the field entirely from the generated struct.
///   - **Note**: You can only use `skip` if the struct is marked as `upgradable` (since there must be a way to supply the missing value when upgrading).
///   - **Note**: If you skip a field that uses a generic type, you **must** use `marked` at the struct level to consume that generic type.
///   - `upgrade = expr`: Provides the expression used to instantiate this field when upgrading. If not provided, it defaults to `<FieldType as core::default::Default>::default()`.
/// - `nest = "Type"`: Delegates the optionization of this field to another type that implements `PartialOptionized` (and `Optionized` if `upgradable`).
///   Usually, this is used for nested struct fields that have themselves been `#[optionized]`.
///
/// ## Field Visibility
/// The generated struct and its fields **strictly retain the visibility** of the original struct and its fields.
pub use optionize_macros::optionized;

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub mod __private {
    pub extern crate alloc;

    pub use optionize_macros::Optionize;
}

/// Represents the relationship between a generated optionized struct and its target original struct.
/// Allows extracting partial data from a full struct, applying partial data to a full struct,
/// and merging two partial structs together.
pub trait PartialOptionized: Sized {
    type Subject;

    /// Consumes the subject and converts it into its optionized version.
    /// This acts as a downgrade, populating every field with `Some(value)`.
    fn optionize(subject: Self::Subject) -> Self;

    /// Patches the provided subject struct with values from this optionized struct.
    /// If a field is `Some`, it will overwrite the subject's corresponding field.
    /// If it is `None`, the subject's field remains unchanged.
    fn patch(self, subject: &mut Self::Subject);

    /// Merges another optionized struct into this one.
    /// By default, `Some` values from the `other` struct will overwrite values in `self`.
    /// `None` values from the `other` struct will leave `self` unchanged.
    fn merge(&mut self, other: Self);
}

/// Provides extension methods on the original subject struct to easily work with its
/// `PartialOptionized` counterpart without having to import and specify the partial type.
pub trait Optionizable: Sized {
    type Object: PartialOptionized<Subject = Self>;

    /// Loads values from the provided partial struct into `self`.
    /// Any `Some` field in the partial struct will overwrite the corresponding field in `self`.
    fn load(&mut self, object: Self::Object) {
        object.patch(self);
    }

    /// Converts `self` into its partial/optionized version.
    /// Every field in the resulting optionized struct will be populated.
    fn downgrade(self) -> Self::Object {
        Self::Object::optionize(self)
    }
}

/// Implemented on an optionized struct (if marked `upgradable`), allowing validation
/// of its completeness and a direct upgrade to the original subject struct.
#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be upgraded",
    label = "Nested type lacks upgrade logic",
    note = "Ensure the subject of `{Self}` is not annotated partial, or is annotated with `#[optionize(partial(upgradable))]`"
)]
pub trait Optionized: PartialOptionized {
    type Errors: IntoIterator<Item: core::error::Error + Send + Sync + 'static>;

    /// Validates that all fields inside the optionized struct that are required for upgrading
    /// contain a value (i.e. are `Some`).
    /// Returns a collection of errors if any fields are missing or if nested validations fail.
    fn validate(&self) -> Result<(), Self::Errors>;

    /// Upgrades the optionized struct into the full subject struct without validating.
    ///
    /// # Safety
    /// Calling this method when `validate()` would return an error results in undefined behavior
    /// because missing `Option::None` fields will be unwrapped without checks.
    unsafe fn upgrade_unchecked(self) -> Self::Subject;

    /// Validates and upgrades the optionized struct into the full subject struct.
    /// Returns `Ok(Self::Subject)` if all required fields are present, otherwise returns `Err(Self::Errors)`.
    fn upgrade(self) -> Result<Self::Subject, Self::Errors> {
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
