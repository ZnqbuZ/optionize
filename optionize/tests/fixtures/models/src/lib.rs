//! External data types for testing trait implementations across crate boundaries.

use core::time::Duration;

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
pub struct Generic<Value> {
    pub value: Option<Value>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ErasedGeneric {
    pub value: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct Subject {
    pub enabled: bool,
    pub count: u32,
    pub label: String,
    pub optional: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct GenericSubject<Value> {
    pub value: Value,
}

#[derive(Debug, PartialEq)]
pub struct BorrowedSubject<'v, Value, const N: usize> {
    pub values: &'v [Value; N],
}

#[derive(Debug, PartialEq)]
pub struct DurationSubject {
    pub timeout: Duration,
}

#[derive(Debug, PartialEq)]
pub struct TupleSubject(pub u32, pub String);

#[derive(Debug, PartialEq)]
pub struct UnitSubject;

// Deliberately no PartialEq: nested operations compare their managed fields.
#[derive(Debug)]
pub struct NestedSubject {
    pub value: u32,
}

#[derive(Debug)]
pub struct ContainerSubject {
    pub nested: NestedSubject,
    pub flattened: NestedSubject,
}
