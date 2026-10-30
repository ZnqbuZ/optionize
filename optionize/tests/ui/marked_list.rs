use patches::optionized;

pub struct Patch {
    pub value: Option<u32>,
}

#[optionized(crate = patches)]
#[optionize(object = Patch, partial(marked(name = data)))]
struct Config {
    value: u32,
}
