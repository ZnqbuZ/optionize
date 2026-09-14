use patches::optionized;
#[optionized(crate = patches)]
#[optionize(attrs(.., -derive(Clone)))]
struct Config { enabled: bool }
