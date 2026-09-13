use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[cfg_attr(all(), optionize(skip, flatten))]
    value: u32,
}
