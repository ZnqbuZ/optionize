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
optionize = "0.5"
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
- `#[optionize(object = "pb::Config<T>")]`: Use an existing struct through a complete type path. Write generic arguments explicitly. The subject must be local when the object is from another crate.
- `#[optionize(attrs(derive(Debug, Default)))]`: Replace the attributes inherited by the generated struct.
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
- `#[optionize(flatten)]`: Do not wrap the field in `Option<T>`. Patching always assigns the field, including `None`; nested fields delegate to their patch implementation.
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

Optionized structs implement `Optionized<Subject>`, allowing you to `upgrade()` them back into the full struct. If any required fields are missing (`None`), the generated implementation returns an `ErrorCollection`.

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
        Err(errors) => {
            // Call validate() before upgrade() if you need to retain an invalid partial.
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
- **`Optionized<Subject>`**: Provides `validate()`, `upgrade()`, and `unsafe upgrade_unchecked()`. `upgrade()` returns `Result<Subject, Self::Errors>` and consumes the partial on both success and failure.

### Migrating to 0.5

Replace `P: Optionized<Subject = S>` with `P: Optionized<S>`, and replace
`<P as Optionized>::Errors` with `<P as Optionized<S>>::Errors`. Manual implementations
remove `type Subject` and return `S` from `upgrade_unchecked`.

The shared trait now supports an external protobuf object with a local subject.
Uniqueness applies to `(object, subject)` pairs. Calls infer the target when its
complete type is uniquely determined. For multiple targets, annotate the upgraded
result or use `Optionized::<S>::validate(&partial)`. A generic parameter that only
appears on the subject still needs type information, even with one implementation.

An existing generic `object = "Patch"` should now spell its arguments explicitly,
for example `object = "pb::Patch<T>"`. Generated object names still inherit the
subject's generic parameters automatically.

## Crates in this workspace

- [`optionize`](optionize): Core traits (`PartialOptionized`, `Optionizable`, `Optionized`), error types, and re-exports.
- [`optionize-macros`](optionize-macros): Procedural macros (`#[optionized]` and `#[derive(Optionize)]`).

## License

MIT License
