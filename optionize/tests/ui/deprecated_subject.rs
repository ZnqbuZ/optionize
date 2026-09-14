#![deny(deprecated)]

use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[deprecated]
    old: bool,
}

fn read(config: &Config) -> bool {
    config.old
}
