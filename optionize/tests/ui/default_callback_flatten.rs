use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(flatten, default = |_| 1)]
    value: u32,
}
