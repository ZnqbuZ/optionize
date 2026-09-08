//! External data types for testing trait implementations across crate boundaries.

#[derive(Debug, Default, PartialEq)]
pub struct Single {
    pub enabled: Option<bool>,
    pub label: Option<String>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Shared {
    pub value: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct Generic<T> {
    pub value: Option<T>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ErasedGeneric {
    pub value: Option<u32>,
}
