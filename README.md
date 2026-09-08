# optionize

A Rust library providing macros and traits to easily generate and manage "optionized" versions of structs. An optionized struct has its fields wrapped in `Option<Value>` (by default), which is extremely useful for configurations, builders, and partial updates (patching).

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
- `#[optionize(object = pb::Config::<Value>)]` or `#[optionize(object = "pb::{}Optional<Value>")]`: Use an existing struct through an unquoted type path or a string template. In strings, `{}` is replaced with the subject's name. Write generic arguments explicitly; unquoted generic paths use `::<Value>`. The subject must be local when the object is from another crate.
- `#[optionize(subject = model::Config)]` or `#[optionize(subject = "model::Config")]`: Treat the annotated struct as the optionized object of an existing subject. Write its fields as `Option<Value>` yourself; aliases for `Option<Value>` also work. The subject may come from another crate; see the example below.
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
- `#[optionize(flatten)]`: Do not wrap the field in `Option<Value>`. Patching always assigns the field, including `None`; nested fields delegate to their patch implementation.
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

The baseline can be the complete subject, the same patch type, or another type
implementing `Schema<Subject, Patch>`. The macro generates the schemas for the
subject and patch automatically. Existing protobuf objects selected through
`object = ...` use the same API.

`Schema::view()` borrows the mapped fields so `retain` can compare complete and
partial baselines without copying values. Views are constructed recursively,
including flattened nested fields. Ordinary callers only need `retain()`; the
view protocol is useful when writing a custom baseline or generic comparison
code. The subject does not implement `PartialOptionized`.

### Using an external subject

When the subject belongs to another crate, put the macro on your local patch
struct. The external subject needs no annotation or wrapper. For example, given
`model::Config { enabled: bool, name: String }` with public fields:

```rust,ignore
use optionize::{optionized, Retain, Schema};

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

fn trim<Baseline: Schema<model::Config, ConfigPatch>>(
    patch: &mut ConfigPatch,
    baseline: &Baseline,
) -> bool {
    patch.retain(baseline)
}
```

`ConfigPatch` owns the shared view in both mapping directions. It implements
`Schema<model::Config>`, whose second parameter defaults to `Self`, and the
subject implements `Schema<model::Config, ConfigPatch>`. Generic baseline bounds
name the patch explicitly, as in `trim` above; ordinary calls infer it.
`PartialOptionized<model::Config>` and `Optionized<model::Config>` only need the
subject parameter, including for external subjects.

Separate mappings use their respective objects as descriptors. To compare with
a different partial representation, implement `Schema<Subject, Patch>` for that
baseline and return the patch's shared view; a common subject alone is not enough.
In this reverse mapping, `nest = NestedSubject` names the nested subject;
the annotated field already supplies its nested object type.

## Traits Overview

- **`PartialOptionized<Subject>`**: Implemented for objects; provides `optionize()`, `patch()`, and `merge()`.
- **`Schema<Subject, Descriptor = Self>`**: Defines `View<'v>` and provides `view()`, which returns the descriptor's shared borrowed view. Generated objects own the view through `Schema<Subject>`; subjects expose complete baselines through `Schema<Subject, Object>`.
- **`Retain<Subject, Descriptor = Self>`**: Provides `retain(&mut self, &baseline) -> bool` when the mapped fields support comparison. The baseline implements `Schema<Subject, Descriptor>`.
- **`Optionizable<Object>`**: Automatically implemented for the subject. Provides `load()` and `downgrade()`.
- **`Optionized<Subject>`**: Provides `validate()`, `upgrade()`, and `unsafe upgrade_unchecked()`. `upgrade()` returns `Result<Subject, Self::Errors>` and consumes the partial on both success and failure.

### Migrating to 0.5

Replace `Patch: Optionized<Subject = Subject>` with `Patch: Optionized<Subject>`, and replace
`<Patch as Optionized>::Errors` with `<Patch as Optionized<Subject>>::Errors`. Manual implementations
remove `type Subject` and return `Subject` from `upgrade_unchecked`.

The shared traits support either an external protobuf object with a local subject
or an external subject with a local object. Uniqueness applies to
`(object, subject)` combinations for `PartialOptionized` and `Optionized`. Calls infer the target when its
complete type is uniquely determined. For multiple targets, annotate the upgraded
result or use `Optionized::<Subject>::validate(&partial)`. A generic parameter that only
appears on the subject still needs type information, even with one implementation.

An existing generic `object = "Patch"` should now spell its arguments explicitly,
for example `object = "pb::Patch<Value>"`. Generated object names still inherit the
subject's generic parameters automatically.

`Diff` and `#[optionize(diff)]` have been removed. To compare two full values,
convert the next value into its patch with `downgrade()`, then call
`patch.retain(&baseline)`. An existing patch can call `retain` directly. Manual
implementations expose borrowed fields through `Schema::View<'v>` and
`Schema::view()`; the macros generate these automatically.

`PartialOptionized`, `Optionized`, and `Optionizable` no longer take a descriptor
parameter. `PartialOptionized::view()` and the subject's identity implementation
have been removed. Keep update operations on the object and move borrowed view
construction into `Schema`.

`Schema<Subject, Descriptor = Self>` replaces its associated `Descriptor` type
with a trait parameter. The object implements `Schema<Subject>` and owns the
shared view. Complete subjects and other baselines implement
`Schema<Subject, Object>`, reuse `<Object as Schema<Subject>>::View<'v>`, and
construct that view in `view()`. `Schema::full_view()` is no longer needed.
`Retain` also defaults its descriptor to `Self`, so generic baseline bounds use
`Baseline: Schema<Subject, Object>` instead of `Baseline: PartialOptionized<Subject, Descriptor>`.
These rules apply to both local and external subjects.

## Crates in this workspace

- [`optionize`](optionize): Core traits (`PartialOptionized`, `Optionizable`, `Optionized`, `Retain`), error types, and re-exports.
- [`optionize-macros`](optionize-macros): Procedural macros (`#[optionized]` and `#[derive(Optionize)]`).

## License

MIT License
