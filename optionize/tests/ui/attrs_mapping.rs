use patches::optionized;
struct Patch { enabled: Option<bool> }
#[optionized(crate = patches)]
#[optionize(object = Patch, attrs())]
struct Config { enabled: bool }
