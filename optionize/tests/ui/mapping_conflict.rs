use patches::optionized;

#[optionized(crate = patches)]
#[optionize(subject = Config, object = Patch)]
struct Patch {
    value: Option<u32>,
}
