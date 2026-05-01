pub trait PartialOptionized: Sized {
    type Subject;
    fn optionize(subject: Self::Subject) -> Self;
    fn patch(self, subject: &mut Self::Subject);
    fn merge(&mut self, other: Self);
}

pub trait Optionizable<O: PartialOptionized<Subject = Self>>: Sized where (): Sized {
    fn load(&mut self, other: O) {
        other.patch(self);
    }
    fn downgrade(self) -> O {
        O::optionize(self)
    }
}

impl<T, O> Optionizable<O> for T where O: PartialOptionized<Subject = T> {}

pub trait Upgradable<Subject>: Sized {
    type Error;
    fn upgrade(self) -> Result<Subject, (Self::Error, Self)>;
}

pub trait Optionized: PartialOptionized + Upgradable<Self::Subject> {}

impl<O> Optionized for O where O: PartialOptionized + Upgradable<Self::Subject> {}
