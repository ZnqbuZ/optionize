use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(as = u64, with(from = |seconds, extra| seconds))]
    value: u32,
}
