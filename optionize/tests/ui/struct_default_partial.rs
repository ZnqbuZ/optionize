use patches::optionized;

#[optionized(crate = patches)]
#[optionize(partial, default)]
#[derive(Default)]
struct Config {
    port: u16,
}
