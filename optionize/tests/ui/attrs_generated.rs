use patches::optionized;
#[optionized(crate = patches)]
#[optionize(attrs(.., -derive, derive(Missing)))]
struct Config { enabled: bool }
