use patches::optionized;

mod model {
    pub struct Service {
        pub port: u16,
    }

    pub trait Spec {
        type Config;
    }

    pub struct Concrete;

    impl Spec for Concrete {
        type Config = Service;
    }
}

#[optionized(crate = patches)]
#[optionize(subject = <model::Concrete as model::Spec>::Config)]
struct ServicePatch {
    port: Option<u16>,
}
