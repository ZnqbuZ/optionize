use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[cfg(all())]
    #[optionize(skip, flatten)]
    value: u32,
}
