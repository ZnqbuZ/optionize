use patches::optionized;
#[optionized(crate = patches)]
#[optionize(attrs("invalid"))]
struct Config { enabled: bool }
