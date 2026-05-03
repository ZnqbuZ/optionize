# optionize-macros

Procedural macros for the `optionize` crate.

This crate provides the `#[optionized]` attribute and `Optionize` derive macro, which automatically generate "optional" versions of your structs, where every field (by default) is wrapped in `Option<T>`. It also implements the necessary traits for applying partial configurations, merging them, and upgrading them back to the original type.

**Note:** This crate should not be used directly. Please use the main [`optionize`](../optionize) crate, which re-exports these macros alongside the required traits.

For full documentation and examples, please see the [workspace README](../README.md).

## License

MIT License