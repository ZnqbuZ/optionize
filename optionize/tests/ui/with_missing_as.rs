use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(with(from = u32::from))]
    value: u32,
}
