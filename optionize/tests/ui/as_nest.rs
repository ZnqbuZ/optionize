use patches::optionized;

#[optionized(crate = patches)]
struct Config {
    #[optionize(nest = ChildOptional, as = Child)]
    child: Child,
}
