pub trait Optionized: Sized {
    type Subject;
    fn optionize(subject: Self::Subject) -> Self;
    fn patch(self, subject: &mut Self::Subject);
    fn merge(&mut self, other: Self);
}

pub trait Optionizable<O: Optionized<Subject = Self>>: Sized {
    fn load(&mut self, other: O) {
        other.patch(self);
    }
    fn downgrade(self) -> O {
        O::optionize(self)
    }
}

impl<T, O> Optionizable<O> for T where O: Optionized<Subject = T> {}

pub trait Upgradable: Optionized {
    type Error;
    fn upgrade(self) -> Result<Self::Subject, Self::Error>;
}
