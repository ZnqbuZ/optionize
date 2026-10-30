use patches::optionized;

#[optionized(crate = patches)]
#[optionize(partial)]
struct Config {
    #[optionize(default = |_| 1)]
    value: u32,
}
