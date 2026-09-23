# optionize

**Typed patches for Rust: generate, merge, prune, and apply.**

[![crates.io](https://img.shields.io/crates/v/optionize.svg)](https://crates.io/crates/optionize)
[![Documentation](https://docs.rs/optionize/badge.svg)](https://docs.rs/optionize)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/ZnqbuZ/optionize/blob/HEAD/LICENSE)

[API documentation](https://docs.rs/optionize) · [Detailed guide](https://github.com/ZnqbuZ/optionize/blob/HEAD/docs/guide.md)

Add `#[optionized]` to a struct to generate its partial counterpart and the code
to apply updates, merge inputs, remove redundant changes, and build complete
values. Your struct defines the fields; `optionize` keeps the patch type and its
operations in sync.

Use it for layered configuration, typed update requests, and assembling values
from partial inputs.

## Why optionize?

- **Nested updates that preserve untouched fields.** Patch one field inside a
  child struct with `nest`; merging, pruning, and validation recurse too.
- **Keep only actual changes.** `retain(&baseline)` removes redundant updates
  against a complete value or another patch, borrowing the baseline without
  cloning it. It also tells you when the entire patch can be omitted.
- **Explicit leave / clear / set semantics.** Nullable fields distinguish an
  omitted update from an explicit clear. Values such as `false`, `0`, and empty
  collections remain meaningful updates.
- **From partial inputs to a complete value.** Merge overrides, supply opt-in
  defaults, then `upgrade()`. Missing fields and nested failures are collected
  into structured errors.
- **Works with types you already have.** Map a local model to an existing patch
  type, including generated protobuf structs, or define a local patch for a
  subject from another crate.

Operations move values without requiring `Clone`. The runtime crate supports
**`no_std` with `alloc`**.

## Quick start

Requires **Rust 1.95 or later**.

```toml
[dependencies]
optionize = "0.5"
```

```rust
use optionize::{optionized, Optionizable, Retain};

#[optionized]
struct Config {
    host: String,
    port: u16,
    verbose: bool,
}

fn main() {
    let mut config = Config {
        host: "localhost".into(),
        port: 8080,
        verbose: true,
    };

    // ConfigOptional is generated with an Option<T> for each field.
    let mut patch = ConfigOptional {
        host: Some("localhost".into()),
        port: Some(9090),     // Change the port.
        verbose: Some(false), // false is a supplied value, too.
    };

    patch.retain(&config);         // Remove updates that already match.
    assert_eq!(patch.host, None);  // The host is now omitted.
    config.load(patch);            // Apply only the remaining updates.

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 9090);
    assert!(!config.verbose);
}
```

The generated type keeps your struct's visibility and field visibility. You can
rename it to `ConfigPatch` with `#[optionize(name = "{}Patch")]`.

## What you can do

| Operation | Example | Behavior |
| --- | --- | --- |
| Apply a patch | `config.load(patch)` | Update supplied fields in an existing value. |
| Combine patches | `patch.merge(overrides)` | Incoming supplied values win; omitted fields keep earlier updates. |
| Remove redundant updates | `patch.retain(&baseline)` | Keep changes relative to a complete or partial baseline. |
| Make a full patch | `config.downgrade()` | Consume a complete value and populate its managed patch fields. |
| Check completeness | `patch.validate()` | Borrow the patch and collect missing fields and nested failures. |
| Build a complete value | `patch.upgrade()` | Consume the patch and return a value or structured errors. |

Import `Optionizable` for `load` and `downgrade`, `PartialOptionized` for `merge`
and the patch-side `patch(&mut config)` method, `Optionized` for `validate` and
`upgrade`, and `Retain` for `retain`.

`retain` requires comparison support for the fields it compares; these bounds
do not restrict patching, merging, or upgrading.

### Layer inputs, then build

Merge a configuration file's values with command-line overrides, then check that
you have enough information to build the complete configuration. Parsing those
inputs is handled by your application.

```rust
use optionize::{optionized, Optionized, PartialOptionized};

#[optionized]
#[optionize(attrs(derive(Default)))]
struct Config {
    host: String,
    #[optionize(default = |_| 8080)]
    port: u16,
}

fn main() {
    let mut patch = ConfigOptional {
        host: Some("localhost".into()),
        ..Default::default()
    };
    patch.merge(ConfigOptional {
        host: Some("api.example.com".into()),
        ..Default::default()
    });

    let config: Config = patch.upgrade().unwrap();
    assert_eq!(config.host, "api.example.com");
    assert_eq!(config.port, 8080);
}
```

Defaults are opt-in and apply when upgrading. Deriving `Default` for the patch
creates omitted fields; it does not fill the complete configuration's defaults.
`upgrade()` consumes the patch even on failure. Use `validate()` first if you
want to inspect errors and keep editing the patch.

### Leave, clear, or set

A field such as `description: Option<String>` becomes
`description: Option<Option<String>>` in the patch:

| Patch value | Meaning |
| --- | --- |
| `None` | Leave the current description unchanged. |
| `Some(None)` | Clear the description. |
| `Some(Some(value))` | Set a new description. |

For ordinary wrapped fields, `Some(false)`, `Some(0)`, and `Some(Vec::new())`
are also explicit updates. During upgrading, an omitted field is missing unless
it has an upgrade default; an explicit clear is a supplied value.

### Patch nested structs

Use `nest` to update selected fields inside an existing child struct:

```rust
use optionize::{optionized, Optionizable};

#[optionized]
struct Database {
    host: String,
    port: u16,
}

#[optionized]
struct Config {
    #[optionize(nest = DatabaseOptional)]
    database: Database,
}

fn main() {
    let mut config = Config {
        database: Database { host: "localhost".into(), port: 5432 },
    };
    config.load(ConfigOptional {
        database: Some(DatabaseOptional { host: None, port: Some(5433) }),
    });

    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5433);
}
```

Nested patches also merge, retain changes, and validate recursively.

### Keep only changes

```rust
use optionize::{optionized, Retain};

#[optionized]
struct Config {
    enabled: bool,
    label: Option<String>,
}

fn main() {
    let current = Config { enabled: true, label: Some("worker".into()) };
    let mut patch = ConfigOptional {
        enabled: Some(true),
        label: Some(None),
    };

    assert!(patch.retain(&current)); // true means changes remain.
    assert_eq!(patch.enabled, None); // Already matches: omit this update.
    assert_eq!(patch.label, Some(None)); // Still needs to be cleared.
}
```

You can also pass another patch as the baseline. Unknown baseline fields do not
remove updates. To compare two complete values, downgrade the next value and
retain its changes against the current one.

## Customize the mapping

- **Existing types:** map a local struct to an existing patch with `object = ...`,
  or annotate a local patch with `subject = ...` for a type from another crate.
- **Names and attributes:** rename generated types and fields; select inherited
  attributes or supply derives just for the patch.
- **Field behavior:** skip unmanaged fields, use upgrade defaults, map values
  through `From`, or use `flatten` for fields that are always supplied.
- **Struct shapes:** named, tuple, and unit structs, including generics and lifetimes.

See the [detailed guide](https://github.com/ZnqbuZ/optionize/blob/HEAD/docs/guide.md)
for attribute syntax, external mappings, trait details, and migration notes.
The [API documentation](https://docs.rs/optionize) includes runnable examples.

## Behavior to know

- Ordinary collection fields are replaced as whole values. There are no built-in
  recursive patch implementations for standard containers such as `Vec` or maps.
- Serialization and deserialization are configured separately. If you serialize
  patches, configure your format to preserve the distinction between omitted
  fields and explicit clears.
- `validate()` checks completeness and nested upgrade errors. Application rules
  such as allowed port ranges belong in your own validation.

## Development

Use the `optionize` crate in your application; it re-exports the macros from
`optionize-macros`.

```sh
cargo test --workspace
```

For compiler diagnostics and source-range checks, run
`python3 scripts/check-diagnostics.py` with Rust 1.95 installed.

Bug reports and usage examples are welcome in
[GitHub issues](https://github.com/ZnqbuZ/optionize/issues). Please include a small
reproduction and your Rust version when reporting a problem.

## License

[MIT](https://github.com/ZnqbuZ/optionize/blob/HEAD/LICENSE)
