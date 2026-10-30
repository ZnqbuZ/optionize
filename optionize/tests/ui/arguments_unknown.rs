use patches::optionized;

#[optionized(crate = patches, unknown)]
#[optionize(name = "{}Patch")]
struct Config {
    #[optionize(flatten)]
    enabled: bool,
}
