use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(default = mutable_default)]
    value: u32,
}

fn mutable_default(_: &mut ConfigOptional) -> u32 { 7 }
