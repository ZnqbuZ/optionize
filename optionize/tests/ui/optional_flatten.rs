use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(flatten, optional = Option)]
    value: u32,
}
