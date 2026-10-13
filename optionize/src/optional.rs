/// A field wrapper that distinguishes a missing update from a supplied value.
///
/// The associated `Value` identifies the wrapped field type. The wrapper converts
/// to `Option<Value>` when consumed, and its references convert to `Option<&Value>`
/// or `Option<&mut Value>` when borrowed. These conversions must agree with
/// `some` and `NONE`, including when `Value` is
/// itself an `Option`: a supplied `None` is still a supplied value.
///
/// [`Option`] implements this trait. Local wrappers can implement the standard
/// `From` conversions and choose their own serialization behavior without adding
/// serialization dependencies to `optionize`.
///
/// Reference conversion bounds need to be repeated in generic helpers:
///
/// ```
/// use optionize::Optional;
///
/// fn supplied<Wrapper>(value: &Wrapper) -> bool
/// where
///     Wrapper: Optional,
///     for<'a> &'a Wrapper: Into<Option<&'a Wrapper::Value>>,
///     for<'a> &'a mut Wrapper: Into<Option<&'a mut Wrapper::Value>>,
/// {
///     Into::<Option<&Wrapper::Value>>::into(value).is_some()
/// }
///
/// assert!(supplied(&Some(None::<u32>)));
/// assert!(!supplied(&None::<Option<u32>>));
/// ```
pub trait Optional: Into<Option<Self::Value>>
where
    for<'a> &'a Self: Into<Option<&'a Self::Value>>,
    for<'a> &'a mut Self: Into<Option<&'a mut Self::Value>>,
{
    /// The supplied field value, including any application-level nullability.
    type Value;

    /// An omitted update. All three conversions must return `None`.
    const NONE: Self;

    /// A supplied update. All three conversions must preserve the value.
    fn some(value: Self::Value) -> Self;
}

impl<Value> Optional for Option<Value> {
    type Value = Value;

    const NONE: Self = None;

    #[inline]
    fn some(value: Value) -> Self {
        Some(value)
    }
}

// Keep each field's bounds distinct when value types differ only in lifetimes.
// The generated implementations use this adapter so reference conversion bounds
// stay inside the blanket implementation instead of leaking into every method.
#[doc(hidden)]
pub trait OptionalField<Value, const INDEX: usize>: Sized {
    const NONE: Self;
    fn some(value: Value) -> Self;
    fn into_option(self) -> Option<Value>;
    fn as_ref(&self) -> Option<&Value>;
    fn as_mut(&mut self) -> Option<&mut Value>;
}

impl<Value, Wrapper, const INDEX: usize> OptionalField<Value, INDEX> for Wrapper
where
    Wrapper: Optional<Value = Value>,
    for<'a> &'a Wrapper: Into<Option<&'a Value>>,
    for<'a> &'a mut Wrapper: Into<Option<&'a mut Value>>,
{
    const NONE: Self = <Self as Optional>::NONE;

    #[inline]
    fn some(value: Value) -> Self {
        <Self as Optional>::some(value)
    }

    #[inline]
    fn into_option(self) -> Option<Value> {
        self.into()
    }

    #[inline]
    fn as_ref(&self) -> Option<&Value> {
        self.into()
    }

    #[inline]
    fn as_mut(&mut self) -> Option<&mut Value> {
        self.into()
    }
}
