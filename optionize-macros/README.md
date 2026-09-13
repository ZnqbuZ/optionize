# optionize-macros

Procedural macros for the `optionize` crate.

This crate implements the `#[optionized]` attribute, which generates partial versions of structs, wrapping ordinary fields in `Option<Value>`. It also generates trait implementations for applying and merging patches, retaining changes, and upgrading complete patches. The `Optionize` derive is an internal helper, not a standalone generation API.

**Note:** This crate should not be used directly. Please use the main [`optionize`](../optionize) crate, which re-exports these macros alongside the required traits.

For full documentation and examples, please see the [workspace README](../README.md).

## License

MIT License