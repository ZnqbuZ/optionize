# optionize

A Rust library providing macros and traits to easily generate and manage "optionized" versions of structs. An optionized struct has its fields wrapped in `Option<T>` (by default), which is extremely useful for configurations, builders, and partial updates (patching).

## Core Concepts

- **Downgrading**: Convert a complete struct into its optionized version.
- **Patching / Loading**: Apply an optionized struct onto a complete struct, updating only the `Some` fields.
- **Merging**: Merge two optionized structs together.
- **Retaining changes**: Remove updates that already match a complete or partial baseline.
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
- `#[optionize(object = pb::Config::<T>)]` or `#[optionize(object = "pb::{}Optional<T>")]`: Use an existing struct through an unquoted type path or a string template. In strings, `{}` is replaced with the subject's name. Write generic arguments explicitly; unquoted generic paths use `::<T>`. The subject must be local when the object is from another crate.
- `#[optionize(subject = model::Config)]` or `#[optionize(subject = "model::Config")]`: Treat the annotated struct as the optionized object of an existing subject. Write its fields as `Option<T>` yourself; aliases for `Option<T>` also work. The subject may come from another crate; see the example below.
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

- `#[optionize(name = "prefix_{}_suffix")]`: Rename a field in the optionized struct. `{}` will be replaced with the original field name. With `subject = ...`, this instead names the corresponding subject field.
- `#[optionize(flatten)]`: Do not wrap the field in `Option<T>`. Patching always assigns the field, including `None`; nested fields delegate to their patch implementation.
- `#[optionize(nest = NestedTypeOptional)]` or `#[optionize(nest = "NestedTypeOptional")]`: Recursively apply optionize logic to a nested optionized struct. With `subject = ...`, name the nested subject type instead. Allows deep patching, retaining changes, and upgrading.
- `#[optionize(skip)]` / `#[optionize(skip(upgrade = "expr"))]`: Completely omit the field from the generated optionized struct. Only allowed when `partial` is specified on the struct. When upgrading, it uses `Default::default()` or the provided `upgrade` expression. With `subject = ...`, declare the unmanaged subject field with its subject type; the macro removes that field from the local object.

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

### Retaining changes

```rust
use optionize::{optionized, Retain};

#[optionized]
struct Config {
    enabled: bool,
    socket_mark: Option<u32>,
}

let current = Config { enabled: true, socket_mark: Some(7) };
let mut patch = ConfigOptional {
    enabled: Some(true),
    socket_mark: Some(None),
};

assert!(patch.retain(&current));
assert_eq!(patch.enabled, None);
assert_eq!(patch.socket_mark, Some(None));

// A partial baseline uses exactly the same call.
let baseline = ConfigOptional {
    enabled: None,
    socket_mark: Some(None),
};
assert!(!patch.retain(&baseline));
assert_eq!(patch.socket_mark, None);
```

`retain` borrows the baseline and removes redundant updates in place, without
requiring `Clone`. It returns `true` when changes remain and `false` when the whole
patch can be omitted relative to that baseline. An unknown baseline field does
not remove an update. `Some(None)` remains a distinct operation that clears a
nullable field.

The macro provides `Retain` automatically when the compared fields support
`PartialEq`; no attribute enables it. The full subject does not need to implement
`PartialEq`, and these bounds do not restrict patching or upgrading. Nested
patches compare recursively. `skip` fields are ignored. `flatten` fields stay
stored because they cannot represent an omitted update, but equal values do not
count as remaining changes. Consequently, a `false` result does not necessarily
mean every stored field is `None`.

The baseline can be any `PartialOptionized` implementation with the same subject
and descriptor, including the subject itself. Existing protobuf objects
selected through `object = ...` use the same API.
Borrowed views are constructed recursively before comparison, including
flattened nested fields.

### Using an external subject

When the subject belongs to another crate, put the macro on your local patch
struct. The external subject needs no annotation or wrapper. For example, given
`model::Config { enabled: bool, name: String }` with public fields:

```rust,ignore
use optionize::{optionized, PartialOptionized, Retain};

#[optionized]
#[optionize(subject = model::Config)]
struct ConfigPatch {
    enabled: Option<bool>,
    name: Option<String>,
}

let current = model::Config {
    enabled: true,
    name: "node".into(),
};
let mut patch = ConfigPatch {
    enabled: Some(true),
    name: Some("node".into()),
};
assert!(!patch.retain(&current));

fn trim<B: PartialOptionized<model::Config, ConfigPatch>>(
    patch: &mut ConfigPatch,
    baseline: &B,
) -> bool {
    patch.retain(baseline)
}
```

The local `ConfigPatch` also serves as the descriptor used by the
mapping. Ordinary method calls infer it; generic bounds specify it as the second
trait parameter. The usual local-subject form defaults this parameter to the
subject, so `PartialOptionized<Config>` and `Optionized<Config>` remain sufficient.
Separate reverse mappings use their respective objects as descriptors; sharing a
subject alone does not make them mutually comparable.
In this reverse mapping, `nest = NestedSubject` names the nested subject;
the annotated field already supplies its nested object type.

## Traits Overview

- **`PartialOptionized<Subject, Descriptor = Subject>`**: Provides `optionize()`, `patch()`, `merge()`, and a borrowed field view. The subject itself has an identity mapping, so it can also serve as a complete baseline.
- **`Schema<Subject>`**: Defines `View<'a>` and `full_view()`. The macro generates the view type, including nested views, for the mapping descriptor.
- **`Retain<Subject, Descriptor = Subject>`**: Provides `retain(&mut self, &baseline) -> bool` when the mapped fields support comparison.
- **`Optionizable<Object, Descriptor = Self>`**: Automatically implemented for the subject. Provides `load()` and `downgrade()`.
- **`Optionized<Subject, Descriptor = Subject>`**: Provides `validate()`, `upgrade()`, and `unsafe upgrade_unchecked()`. `upgrade()` returns `Result<Subject, Self::Errors>` and consumes the partial on both success and failure.

### Migrating to 0.5

Replace `P: Optionized<Subject = S>` with `P: Optionized<S>`, and replace
`<P as Optionized>::Errors` with `<P as Optionized<S>>::Errors`. Manual implementations
remove `type Subject` and return `S` from `upgrade_unchecked`.

The shared traits support either an external protobuf object with a local subject
or an external subject with a local object. Uniqueness applies to
`(object, subject, descriptor)` combinations. Calls infer the target when its
complete type is uniquely determined. For multiple targets, annotate the upgraded
result or use `Optionized::<S>::validate(&partial)`. A generic parameter that only
appears on the subject still needs type information, even with one implementation.

An existing generic `object = "Patch"` should now spell its arguments explicitly,
for example `object = "pb::Patch<T>"`. Generated object names still inherit the
subject's generic parameters automatically.

`Diff` and `#[optionize(diff)]` have been removed. To compare two full values,
convert the next value into its patch with `downgrade()`, then call
`patch.retain(&baseline)`. An existing patch can call `retain` directly. Manual
implementations expose borrowed fields through `Schema::View<'a>` and
`PartialOptionized::view()`; the macros generate these automatically.

## Crates in this workspace

- [`optionize`](optionize): Core traits (`PartialOptionized`, `Optionizable`, `Optionized`, `Retain`), error types, and re-exports.
- [`optionize-macros`](optionize-macros): Procedural macros (`#[optionized]` and `#[derive(Optionize)]`).

## License

MIT License
