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

#[derive(Debug, PartialEq)]
pub struct Subject {
    pub enabled: bool,
    pub count: u32,
    pub label: String,
    pub optional: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub struct GenericSubject<T> {
    pub value: T,
}

#[derive(Debug, PartialEq)]
pub struct BorrowedSubject<'a, T, const N: usize> {
    pub values: &'a [T; N],
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
