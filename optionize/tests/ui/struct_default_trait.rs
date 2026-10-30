use patches::optionized;

#[optionized(crate = patches)]
#[optionize(default)]
struct Config {
    port: u16,
}
