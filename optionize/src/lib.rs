pub trait Optionized {
    type Target;
    fn patch(self, target: &mut Self::Target);
    fn merge(&mut self, other: Self);
}

pub trait Upgradable: Optionized {
    type Error;
    fn upgrade(self) -> Result<Self::Target, Self::Error>;
}
