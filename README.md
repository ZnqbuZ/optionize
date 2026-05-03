# optionize

`optionize` is a Rust library providing macros and traits to easily generate and manage "optionized" versions of structs. An optionized struct has all its fields wrapped in `Option<T>`, which is extremely useful for configurations, builders, and partial updates (patching).

## Usage

Add `optionize` to your `Cargo.toml`:

```toml
[dependencies]
optionize = "0.1.0"
```

### Basic Example

```rust
use optionize::{optionized, Optionizable};

#[optionized]
#[derive(Debug, Clone)]
struct Config {
    host: String,
    port: u16,
}

fn main() {
    let mut config = Config {
        host: "localhost".to_string(),
        port: 8080,
    };

    // The optionized struct is generated as `ConfigOptional` by default.
    let partial = ConfigOptional {
        host: None,
        port: Some(9090),
    };

    // Patch the original config with the partial update
    config.load(partial);

    assert_eq!(config.port, 9090);
    assert_eq!(config.host, "localhost");
}
```

### Advanced Features

You can customize the generated struct and its fields using the `#[optionize(...)]` helper attribute.

```rust
use optionize::optionized;

#[optionized]
#[optionize(
    name = "PartialConfig",
    attrs(derive(Debug, Default))
)]
struct Config {
    #[optionize(name = "hostname")]
    host: String,
    
    // Do not wrap in Option
    #[optionize(flatten)]
    is_active: Option<bool>,
}
```

#### Struct Attributes

- `#[optionize(name = "CustomName")]`: Set the name of the generated optionized struct.
- `#[optionize(attrs(derive(Debug, Default)))]`: Add attributes to the generated struct.
- `#[optionize(partial(upgradable))]`: Useful when generating partial structs that should/shouldn't implement `Optionized`.

#### Field Attributes

- `#[optionize(name = "new_name")]`: Rename a field in the optionized struct.
- `#[optionize(flatten)]`: Do not wrap the field in `Option<T>` (useful if the field is already an `Option`).
- `#[optionize(nest = "OtherOptionizedType")]`: Indicate that this field should recursively use another optionized type.
- `#[optionize(skip(upgrade = "default_value()"))]`: Exclude the field from the optionized struct (only allowed with `partial`). Requires providing a default value expression for upgrading.

#### Upgrading from Partial

Optionized structs implement the `Optionized` trait, allowing you to `upgrade()` them back into the full struct. If any required fields are `None`, it returns a detailed `UpgradeErrorCollection`.

```rust
use optionize::Optionized;

let partial = ConfigOptional {
    host: Some("example.com".to_string()),
    port: None,
};

let result = partial.upgrade();
assert!(result.is_err());
```

## Crates in this workspace

- [`optionize`](optionize): Core traits (`PartialOptionized`, `Optionizable`, `Optionized`) and re-exports.
- [`optionize-macros`](optionize-macros): Procedural macros (`#[optionized]` and `#[derive(Optionize)]`).

## License

MIT License
