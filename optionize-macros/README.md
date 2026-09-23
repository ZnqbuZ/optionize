# optionize-macros

Procedural macros for the `optionize` crate.

This crate implements the `#[optionized]` attribute, which generates partial versions of structs, wrapping ordinary fields in `Option<Value>`. It also generates trait implementations for applying and merging patches, retaining changes, and upgrading complete patches. The `Optionize` derive is an internal helper, not a standalone generation API.

Use the main [`optionize`](https://crates.io/crates/optionize) crate in your application. It re-exports these macros alongside the required traits.

See the [project README](https://github.com/ZnqbuZ/optionize#readme) for a quick start and the [API documentation](https://docs.rs/optionize) for examples and attribute details.

## License

[MIT](https://github.com/ZnqbuZ/optionize/blob/HEAD/LICENSE)
