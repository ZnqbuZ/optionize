use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(flatten, default)]
    value: u32,
}
