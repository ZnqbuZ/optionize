# optionize

A Rust library providing macros and traits to easily generate and manage "optionized" versions of structs. An optionized struct has its fields wrapped in `Option<T>` (by default), which is extremely useful for configurations, builders, and partial updates (patching).

## Core Concepts

- **Downgrading**: Convert a complete struct into its optionized version.
- **Patching / Loading**: Apply an optionized struct onto a complete struct, updating only the `Some` fields.
- **Merging**: Merge two optionized structs together.
- **Upgrading**: Convert an optionized struct back into a complete struct. If any required fields are missing, it returns beautifully structured errors detailing exactly what is missing.

## Installation

Add `optionize` to your `Cargo.toml`:

```toml
[dependencies]
optionize = "0.1"
```

## Basic Example

```rust
use optionize::{optionized, Optionizable};

// This generates `ConfigOptional` which implements `PartialOptionized` and `Optionized`.
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

    // The optionized struct is named `<OriginalName>Optional` by default.
    let partial = ConfigOptional {
        host: None,
        port: Some(9090),
    };

    // Patch the original config with the partial update
    config.load(partial);

    assert_eq!(config.port, 9090);
    assert_eq!(config.host, "localhost");
    
    // You can also "downgrade" a full config to a partial one
    let partial_config = config.downgrade();
}
```

## Advanced Features & Customization

You can customize the generated struct and its fields using the `#[optionize(...)]` helper attribute.

### Struct Attributes

- `#[optionize(name = "CustomPrefix{}CustomSuffix")]`: Set the name of the generated optionized struct. `{}` will be replaced with the original struct name.
- `#[optionize(attrs(derive(Debug, Default)))]`: Add attributes (like `derive`) to the generated struct.
- `#[optionize(partial(upgradable))]`: By default, the generated struct implements both `PartialOptionized` and `Optionized`. If you only want partial updates and don't need upgrading, use `#[optionize(partial)]`. If you want both while using `partial` specific features (like `skip`), use `#[optionize(partial(upgradable))]`.
- `#[optionize(partial(marked))]`: Adds a `PhantomData` marker to the generated struct, typing it strictly to the original struct.

```rust
use optionize::optionized;

#[optionized]
#[optionize(
    name = "PartialConfig",
    attrs(derive(Debug, Default))
)]
struct Config {
    host: String,
}
```

### Field Attributes

- `#[optionize(name = "prefix_{}_suffix")]`: Rename a field in the optionized struct. `{}` will be replaced with the original field name.
- `#[optionize(flatten)]`: Do not wrap the field in `Option<T>`. Useful if the field is already an `Option` or you want to handle its absence explicitly.
- `#[optionize(nest = "NestedTypeOptional")]`: Recursively apply optionize logic to a nested optionized struct. Allows deep patching and deep upgrading.
- `#[optionize(skip)]` / `#[optionize(skip(upgrade = "expr"))]`: Completely omit the field from the generated optionized struct. Only allowed when `partial` is specified on the struct. When upgrading, it uses `Default::default()` or the provided `upgrade` expression.

#### Nested Structs Example

```rust
use optionize::{optionized, Optionized};

#[optionized]
#[derive(Debug)]
struct Inner {
    val: i32,
}

#[optionized]
#[derive(Debug)]
struct Outer {
    // Tell optionize to recurse into this struct when patching/upgrading
    #[optionize(nest = "InnerOptional")]
    inner: Inner,
}
```

### Upgrading and Error Handling

Optionized structs implement the `Optionized` trait, allowing you to `upgrade()` them back into the full struct. If any required fields are missing (`None`), it returns a detailed `UpgradeErrorCollection`.

```rust
use optionize::{optionized, Optionized};

#[optionized]
struct User {
    name: String,
    age: u8,
}

fn main() {
    let partial = UserOptional {
        name: Some("Alice".to_string()),
        age: None, // Missing required field
    };

    let result = partial.upgrade();
    
    match result {
        Ok(user) => println!("Upgraded!"),
        Err((errors, original_partial)) => {
            // `errors` implements Display for beautifully formatted error messages.
            // Notice that ownership of `original_partial` is given back to you on failure!
            println!("{}", errors);
        }
    }
}
```

The error collection formats missing fields and nested errors clearly, grouping them by the struct type:
```text
Upgrade failed with 1 error(s):
  `UserOptional` -> `User`
    - Missing required field: `age`
```

## Traits Overview

- **`PartialOptionized<Subject>`**: Provides `optionize()`, `patch()`, and `merge()`.
- **`Optionizable<Object>`**: Automatically implemented for the original struct. Provides `load()` and `downgrade()`.
- **`Optionized<Subject>`**: Provides `upgrade()`. Returns `Result<Subject, (UpgradeErrors, Self)>` where errors contain the specific missing fields or nested failures, and the original partial struct is returned in the `Err` variant so you don't lose the data.

## Crates in this workspace

- [`optionize`](optionize): Core traits (`PartialOptionized`, `Optionizable`, `Optionized`), error types, and re-exports.
- [`optionize-macros`](optionize-macros): Procedural macros (`#[optionized]` and `#[derive(Optionize)]`).

## License

MIT License