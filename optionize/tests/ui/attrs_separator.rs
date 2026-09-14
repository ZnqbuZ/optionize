use patches::optionized;
#[optionized(crate = patches)]
#[optionize(attrs(.. derive(Debug)))]
struct Config { enabled: bool }
