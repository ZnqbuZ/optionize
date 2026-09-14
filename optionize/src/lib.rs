//! `optionize` generates structs for partial configuration, updates, and builders.
//! The complete struct is the **subject**; its partial representation is the
//! **object**. By default, `#[optionized]` generates `SubjectOptional`, wrapping
//! each field in `Option<Value>` and preserving the subject's visibility.
//! The library supports `no_std` with `alloc`.
//!
//! - [Applying, merging, and converting patches](#applying-merging-and-converting-patches)
//! - [Missing, clearing, and setting values](#missing-clearing-and-setting-values)
//! - [Upgrade defaults](#upgrade-defaults)
//! - [Names and inherited attributes](#names-and-inherited-attributes)
//! - [Partial structs and skipped fields](#partial-structs-and-skipped-fields)
//! - [Generics, markers, tuples, and units](#generics-markers-tuples-and-units)
//! - [Flattened fields](#flattened-fields)
//! - [Nested patches](#nested-patches)
//! - [Existing objects and external subjects](#existing-objects-and-external-subjects)
//! - [Retaining changes](#retaining-changes)
//! - [Validation and errors](#validation-and-errors)
//! - [Crate paths](#crate-paths)
//!
//! ## Applying, merging, and converting patches
//!
//! Import the traits for the operations you use. [`PartialOptionized`] provides
//! `optionize`, `patch`, and `merge` on objects; [`Optionizable`] provides `load`
//! and `downgrade` on subjects. [`Optionized`] adds validation and upgrading.
//!
//! ```rust
//! use optionize::{optionized, Optionizable, Optionized, PartialOptionized};
//!
//! #[optionized]
//! #[derive(Debug, PartialEq)]
//! struct Config {
//!     host: String,
//!     port: u16,
//! }
//!
//! let mut config = Config { host: "localhost".into(), port: 8080 };
//! let mut patch = ConfigOptional { host: Some("gateway".into()), port: None };
//! patch.merge(ConfigOptional { host: None, port: Some(0) });
//! // Incoming None preserves the existing update; Some(0) is a real update.
//! assert_eq!(patch.host.as_deref(), Some("gateway"));
//! patch.patch(&mut config);
//! assert_eq!(config, Config { host: "gateway".into(), port: 0 });
//!
//! // Loading is the subject-side spelling of patching.
//! config.load(ConfigOptional { host: None, port: Some(9090) });
//! assert_eq!(config.host, "gateway");
//!
//! // Downgrading consumes the subject and fills every managed field.
//! let patch: ConfigOptional = config.downgrade();
//! assert_eq!(patch.port, Some(9090));
//! assert!(patch.validate().is_ok());
//! let config: Config = patch.upgrade().unwrap();
//! assert_eq!(config.port, 9090);
//!
//! // The object-side constructor performs the same conversion as downgrade().
//! let patch = ConfigOptional::optionize(config);
//! assert_eq!(patch.host.as_deref(), Some("gateway"));
//! ```
//!
//! These operations move values; neither the subject nor its fields need `Clone`.
//! For generic helpers and objects with several subjects, see [`Optionized`].
//!
//! ## Missing, clearing, and setting values
//!
//! A nullable subject field `Option<Value>` becomes `Option<Option<Value>>`.
//! The outer option records whether an update was supplied:
//!
//! | Patch value | Effect on the subject | Complete enough to upgrade? |
//! | --- | --- | --- |
//! | `None` | Leave the field unchanged | No |
//! | `Some(None)` | Clear the field | Yes |
//! | `Some(Some(value))` | Set the field | Yes |
//!
//! ```rust
//! use optionize::{optionized, Optionizable, Optionized};
//!
//! #[optionized]
//! struct Config { socket_mark: Option<u32> }
//!
//! let mut config = Config { socket_mark: Some(7) };
//! config.load(ConfigOptional { socket_mark: None });
//! assert_eq!(config.socket_mark, Some(7));
//! config.load(ConfigOptional { socket_mark: Some(None) });
//! assert_eq!(config.socket_mark, None);
//! config.load(ConfigOptional { socket_mark: Some(Some(0)) });
//! assert_eq!(config.socket_mark, Some(0));
//!
//! assert!(ConfigOptional { socket_mark: None }.validate().is_err());
//! assert!(ConfigOptional { socket_mark: Some(None) }.upgrade().unwrap().socket_mark.is_none());
//! ```
//!
//! Ordinary fields have no implicit defaults during upgrading: `None` is missing
//! even when the field's type implements `Default`, unless explicitly configured
//! with `#[optionize(default)]` or `default = callback`. Deriving `Default` for an
//! object makes wrapped fields `None`; flattened fields retain their type's default
//! values and still participate in updates. It does not fill the subject's defaults.
//!
//! ## Upgrade defaults
//!
//! `default` uses the subject field's `Default::default()` when its update is
//! absent. `default = callback` instead accepts a function or non-capturing closure
//! with signature `fn(&Object) -> FieldType`. The parameter is the actual object,
//! including when `object = ...` or `subject = ...` selects an existing type.
//! Callbacks receive an immutable reference and never require the object to be cloned.
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! struct Config {
//!     name: String,
//!     #[optionize(default = |object| object.name.as_ref().unwrap().len())]
//!     name_length: usize,
//!     #[optionize(default = reusable)]
//!     reusable: bool,
//!     #[optionize(default)]
//!     retries: u32,
//! }
//! fn reusable(_: &ConfigOptional) -> bool { true }
//!
//! let patch = ConfigOptional {
//!     name: Some("node".into()), name_length: None, reusable: Some(false), retries: None,
//! };
//! patch.validate().unwrap(); // Does not execute the callbacks.
//! let config = patch.upgrade().unwrap();
//! assert_eq!(config.name_length, 4);
//! assert!(!config.reusable); // A supplied false is preserved.
//! assert_eq!(config.retries, 0);
//! ```
//!
//! Upgrading first validates the supplied fields. If validation fails, no default
//! callbacks run. Otherwise, required defaults are evaluated once in declaration
//! order, before any fields are moved out of the object. Every callback sees the
//! original patch: defaults computed for earlier fields are not written back into
//! it. Callbacks must handle other omitted defaulted fields themselves.
//!
//! Defaults only apply during upgrading. They do not change `patch`, `load`,
//! `merge`, `retain`, Serde behavior, or the object's `Default` implementation.
//! A supplied `Some(None)` is an explicit clear and does not trigger a default:
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//! #[optionized]
//! struct Config {
//!     #[optionize(default = |_| Some("fallback".into()))]
//!     note: Option<String>,
//! }
//! assert_eq!(ConfigOptional { note: None }.upgrade().unwrap().note.as_deref(), Some("fallback"));
//! assert_eq!(ConfigOptional { note: Some(None) }.upgrade().unwrap().note, None);
//! ```
//!
//! A nested default returns the **complete child subject**, not its patch. It runs
//! only when the whole child is absent; a supplied child still validates normally:
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//! #[optionized]
//! struct Endpoint { port: u16 }
//! #[optionized]
//! struct Config {
//!     #[optionize(nest = EndpointOptional, default = |_| Endpoint { port: 8080 })]
//!     endpoint: Endpoint,
//! }
//! assert_eq!(ConfigOptional { endpoint: None }.upgrade().unwrap().endpoint.port, 8080);
//! assert!(ConfigOptional { endpoint: Some(EndpointOptional { port: None }) }.upgrade().is_err());
//! ```
//!
//! `default` cannot be combined with `flatten`, because flattened fields are
//! always supplied. Standard defaults add `FieldType: Default` only to the upgrade
//! implementation; other operations remain available without that bound.
//! Callbacks are infallible functions, but may panic. They should not invalidate
//! nested objects through shared interior state: nested values are checked again
//! when consumed, and invalidation during construction causes a panic.
//!
//! ## Names and inherited attributes
//!
//! `name = "{}Patch"` changes the generated type name; `{}` expands to the subject's
//! name. On a named field, `{}` expands to that field's name. Tuple fields cannot
//! be renamed. Attributes after `#[optionized]` are inherited by default.
//!
//! `attrs(...)` specifies attributes on the generated type or field. With ordinary
//! attributes only, it replaces the inherited attributes; `attrs()` clears them.
//! Repeated lists are combined, so an empty list does not reset other lists.
//! The original subject's attributes remain in place.
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(name = "{}Patch", attrs(derive(Debug, Default)), attrs(), attrs(derive(Clone)))]
//! #[derive(PartialEq)]
//! struct Config {
//!     /// The listening port on the complete configuration.
//!     #[optionize(name = "set_{}", attrs(doc = "An optional port update."))]
//!     port: u16,
//! }
//!
//! let mut patch = ConfigPatch::default();
//! assert_eq!(patch.set_port, None);
//! patch.set_port = Some(8080);
//! assert_eq!(patch.clone().upgrade().unwrap().port, 8080);
//! ```
//!
//! To reuse original attributes, include selectors alongside ordinary attributes:
//!
//! | Item | Meaning |
//! | --- | --- |
//! | `+path` | Inherit original attributes with this exact path. |
//! | `..` | Inherit all original attributes. |
//! | `-path` | Exclude matching attributes from the inherited selection. |
//! | Ordinary attribute | Append this attribute after the inherited selection. |
//!
//! Selectors operate only on the original attributes: `-derive` does not remove
//! an explicitly added `derive(...)`. Inherited attributes keep their original
//! order; repeated selectors do not copy the same attribute twice, while multiple
//! original `doc` attributes are all preserved. Explicit attributes follow in
//! their written order across all lists and are not merged or deduplicated.
//! Paths match exactly, without name resolution; a selector that matches nothing
//! has no effect. `-path` alone does not imply inheriting everything else.
//!
//! This example retains the type's documentation while replacing its entire
//! derive list, and retains the field's documentation while appending another line:
//!
//! ```rust
//! use optionize::optionized;
//!
//! #[optionized]
//! /// Connection settings.
//! #[derive(Clone)]
//! #[optionize(attrs(.., -derive, derive(Debug, Default)))]
//! struct Config {
//!     /// The listening port.
//!     #[optionize(attrs(+doc, doc = "Omit to keep the current port."))]
//!     port: u16,
//! }
//!
//! let patch = ConfigOptional::default();
//! assert_eq!(format!("{patch:?}"), "ConfigOptional { port: None }");
//! let config = Config { port: 8080 };
//! assert_eq!(config.clone().port, 8080);
//! ```
//!
//! Each selector accepts an attribute path only. `+derive` or `-derive` selects
//! or excludes whole `derive(...)` attributes; selecting individual derived
//! traits is not supported. Replace the whole list as above instead:
//!
//! ```compile_fail
//! use optionize::optionized;
//! #[optionized]
//! #[derive(Debug, Clone)]
//! #[optionize(attrs(.., -derive(Clone)))]
//! struct Config { port: u16 }
//! ```
//!
//! Place derives **after** `#[optionized]` if the object should inherit them.
//! A derive that has already run before the macro is not inherited:
//!
//! ```compile_fail,E0277
//! use optionize::optionized;
//! #[derive(Clone)]
//! #[optionized]
//! struct Config { port: u16 }
//! fn needs_clone<Value: Clone>() {}
//! needs_clone::<ConfigOptional>();
//! ```
//!
//! ## Partial structs and skipped fields
//!
//! By default, objects implement both [`PartialOptionized`] and [`Optionized`].
//! `partial` disables validation and upgrading. It also permits `skip`, which
//! removes a field from the object and leaves that subject field untouched when
//! patching. A skipped field need not implement `Default` when upgrading is disabled.
//!
//! ```rust
//! use optionize::{optionized, Optionizable};
//!
//! struct Connection(u32);
//! #[optionized]
//! #[optionize(partial)]
//! struct Config {
//!     port: u16,
//!     #[optionize(skip)]
//!     connection: Connection,
//! }
//!
//! let mut config = Config { port: 8080, connection: Connection(7) };
//! config.load(ConfigOptional { port: Some(9090) });
//! assert_eq!(config.port, 9090);
//! assert_eq!(config.connection.0, 7);
//! ```
//!
//! A partial-only object cannot be upgraded:
//!
//! ```compile_fail
//! use optionize::{optionized, Optionized};
//! #[optionized]
//! #[optionize(partial)]
//! struct Config { port: u16 }
//! ConfigOptional { port: Some(8080) }.upgrade();
//! ```
//!
//! Use `partial(upgradable)` to retain upgrading while allowing skipped fields.
//! During upgrading, a skipped field uses `Default::default()` or the callback
//! supplied by `#[optionize(skip, default = callback)]`. It follows the same
//! initialization rules as ordinary upgrade defaults and can inspect the original
//! object. `skip` can be combined with `default`, but not other field options.
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(partial(upgradable))]
//! struct Config {
//!     port: u16,
//!     #[optionize(skip)]
//!     connections: Vec<u32>,
//!     #[optionize(skip, default = |_| 3)]
//!     retries: u8,
//! }
//!
//! let patch = ConfigOptional { port: Some(8080) };
//! assert!(patch.validate().is_ok());
//! let config = patch.upgrade().unwrap();
//! assert!(config.connections.is_empty());
//! assert_eq!(config.retries, 3);
//! ```
//!
//! ## Generics, markers, tuples, and units
//!
//! Field `cfg` and `cfg_attr` conditions are evaluated before mapping fields,
//! including attributes enabled by `cfg_attr`. Removed fields need no available
//! type and do not contribute validation or equality bounds. Tuple indices are
//! determined after conditional fields are removed.
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//! #[optionized]
//! struct Config {
//!     #[cfg(any())]
//!     platform: UnavailableType,
//!     #[cfg_attr(all(), optionize(name = "active"))]
//!     enabled: bool,
//! }
//! let config = ConfigOptional { active: Some(true) }.upgrade().unwrap();
//! assert!(config.enabled);
//! ```
//!
//! Generated objects preserve the subject's generic parameters and bounds,
//! including lifetimes and const parameters. Borrows may refer to local values:
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! struct Window<'v, Value, const SIZE: usize> {
//!     values: &'v [Value; SIZE],
//! }
//! let values = [1, 2, 3];
//! let patch = WindowOptional { values: Some(&values) };
//! assert_eq!(patch.upgrade().unwrap().values, &values);
//! ```
//!
//! If skipping fields leaves a type or lifetime parameter unused, add
//! `partial(marked)` to inject a `PhantomData` field. Named structs use an available
//! `_marker` name by default; `marked(name = ...)` chooses one explicitly.
//! `marked(attrs(...))` uses the same attribute-selection rules, starting from
//! the marker's default `#[doc(hidden)]` attribute. For example,
//! `marked(attrs(.., allow(dead_code)))` keeps it and adds another attribute.
//!
//! ```rust
//! use core::marker::PhantomData;
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! #[optionize(partial(upgradable, marked(name = data, attrs(doc = "Tracks the data type."))))]
//! struct Config<Data: Default> {
//!     port: u16,
//!     #[optionize(skip)]
//!     cache: Data,
//! }
//!
//! let patch = ConfigOptional::<Vec<u8>> { port: Some(8080), data: PhantomData };
//! assert!(patch.upgrade().unwrap().cache.is_empty());
//! ```
//!
//! Tuple fields keep their order, with skipped fields removed. A marker is appended
//! to a tuple object and cannot be given a name. Unit subjects produce unit objects
//! unless marked: then the object has a tuple marker, or a named marker when
//! `marked(name = ...)` is specified.
//!
//! ```rust
//! use core::marker::PhantomData;
//! use optionize::{optionized, Optionized};
//!
//! #[optionized]
//! struct Address(String, u16);
//! #[optionized]
//! struct Empty;
//! #[optionized]
//! #[optionize(partial(upgradable, marked))]
//! struct Tagged;
//!
//! let address = AddressOptional(Some("localhost".into()), Some(8080)).upgrade().unwrap();
//! assert_eq!((address.0.as_str(), address.1), ("localhost", 8080));
//! let _: Empty = EmptyOptional.upgrade().unwrap();
//! let _: Tagged = TaggedOptional(PhantomData).upgrade().unwrap();
//! ```
//!
//! ## Flattened fields
//!
//! `flatten` omits the extra `Option` wrapper. Such fields always participate in
//! patching and merging, including when their own value is `None`. They cannot
//! represent an omitted update and do not need a presence check during upgrading.
//!
//! ```rust
//! use optionize::{optionized, Optionizable, PartialOptionized};
//!
//! #[optionized]
//! struct Config {
//!     #[optionize(flatten)]
//!     socket_mark: Option<u32>,
//!     port: u16,
//! }
//! let mut config = Config { socket_mark: Some(7), port: 8080 };
//! let mut patch = ConfigOptional { socket_mark: Some(9), port: None };
//! patch.merge(ConfigOptional { socket_mark: None, port: None });
//! config.load(patch);
//! assert_eq!(config.socket_mark, None);
//! assert_eq!(config.port, 8080);
//! ```
//!
//! ## Nested patches
//!
//! `nest = ChildOptional` delegates operations to the nested object's traits.
//! Strings such as `nest = "ChildOptional"` are also accepted. Without `nest`, a
//! supplied child replaces the whole child. With `nest`, merging and patching
//! preserve omitted child fields. `flatten` and `nest` can be combined: the child
//! patch is always present, but it still applies its own partial-update rules.
//!
//! ```rust
//! use optionize::{optionized, Optionizable, PartialOptionized, Retain};
//!
//! #[optionized]
//! struct Endpoint { host: String, port: u16 }
//! #[optionized]
//! struct Config {
//!     #[optionize(nest = EndpointOptional)]
//!     primary: Endpoint,
//!     #[optionize(flatten, nest = "EndpointOptional")]
//!     fallback: Endpoint,
//! }
//!
//! let mut config = Config {
//!     primary: Endpoint { host: "primary".into(), port: 8080 },
//!     fallback: Endpoint { host: "fallback".into(), port: 8081 },
//! };
//! let mut patch = ConfigOptional {
//!     primary: Some(EndpointOptional { host: Some("gateway".into()), port: None }),
//!     fallback: EndpointOptional { host: None, port: Some(9091) },
//! };
//! patch.merge(ConfigOptional {
//!     primary: Some(EndpointOptional { host: None, port: Some(9090) }),
//!     fallback: EndpointOptional { host: None, port: None },
//! });
//! config.load(patch);
//! assert_eq!(config.primary.host, "gateway");
//! assert_eq!(config.primary.port, 9090);
//! assert_eq!(config.fallback.host, "fallback");
//! assert_eq!(config.fallback.port, 9091);
//!
//! // Comparison also recurses; Endpoint itself does not need PartialEq.
//! let mut patch = ConfigOptional {
//!     primary: Some(EndpointOptional { host: None, port: Some(9090) }),
//!     fallback: EndpointOptional { host: None, port: Some(9091) },
//! };
//! assert!(!patch.retain(&config));
//! assert!(patch.primary.is_none());
//! assert!(patch.fallback.port.is_none());
//! ```
//!
//! Upgrading validates every supplied child recursively. An absent optional child
//! is a missing field; a flattened child is always validated. In a partial baseline,
//! an absent child is unknown, so even an explicitly supplied empty child patch is
//! retained when compared with it.
//!
//! ## Existing objects and external subjects
//!
//! At least one side of a mapping must be local to the crate defining it.
//! The external side needs public accessible fields and no optionize annotations.
//! This supports generated protobuf structs in either direction. The modules below
//! stand in for types that can instead be imported from another crate.
//!
//! ### Using an existing object
//!
//! `object = ...` generates trait implementations for an existing object instead
//! of generating another struct. Its fields must match the mapping's types, names,
//! `flatten`, `skip`, and `nest` choices. It cannot be combined with struct-level
//! `name`, `attrs`, `subject`, or `partial(marked)`.
//!
//! `object` and `subject` both accept an unquoted type path or a string template.
//! In strings, `{}` is replaced with the annotated struct's name. Generic arguments
//! must be explicit: unquoted paths use `::<Value>` because attribute values are
//! parsed as expressions; strings may use `<Value>`.
//!
//! ```rust
//! use optionize::{optionized, Optionizable, Optionized};
//! mod wire {
//!     pub struct ConfigPatch<Value> { pub value: Option<Value> }
//! }
//!
//! // A reusable pattern also works when this attribute is applied by another macro.
//! #[optionized]
//! #[optionize(object = "wire::{}Patch<Value>")]
//! struct Config<Value> { value: Value }
//!
//! let patch: wire::ConfigPatch<String> = Config { value: "node".into() }.downgrade();
//! let config: Config<String> = patch.upgrade().unwrap();
//! assert_eq!(config.value, "node");
//! ```
//!
//! The direct path `object = wire::ConfigPatch::<Value>` selects the same type.
//! Lifetimes and const arguments must likewise be written in an existing type path;
//! only generated object types inherit those arguments automatically.
//! `nest` accepts the same quoted or direct type-path syntax, but no `{}` substitution.
//!
//! ### Using an external subject
//!
//! Put the macro on the local object and specify `subject = ...`. Declare ordinary
//! object fields as `Option<Value>` yourself; type aliases for `Option` also work.
//! Flattened fields retain their declared types. In this direction, a field's `name`
//! selects the **subject** field, and `nest` names the nested **subject** type.
//!
//! ```rust
//! use optionize::{optionized, Optionizable, Optionized, Retain};
//! mod model {
//!     pub struct Endpoint { pub port: u16 }
//!     pub struct Config<Value> { pub enabled: bool, pub data: Value, pub endpoint: Endpoint }
//! }
//!
//! #[optionized]
//! #[optionize(subject = model::Endpoint)]
//! struct EndpointPatch { port: Option<u16> }
//!
//! #[optionized]
//! #[optionize(subject = model::Config::<Value>)]
//! struct ConfigPatch<Value> {
//!     #[optionize(name = "enabled")]
//!     active: Option<bool>,
//!     data: Option<Value>,
//!     #[optionize(nest = model::Endpoint)]
//!     endpoint: Option<EndpointPatch>,
//! }
//!
//! let mut config = model::Config {
//!     enabled: false,
//!     data: String::from("node"),
//!     endpoint: model::Endpoint { port: 8080 },
//! };
//! config.load(ConfigPatch { active: Some(true), data: None, endpoint: None });
//! assert!(config.enabled);
//! assert_eq!(config.endpoint.port, 8080);
//! let mut patch = ConfigPatch { active: Some(true), data: None, endpoint: None };
//! assert!(!patch.retain(&config));
//! let patch: ConfigPatch<String> = config.downgrade();
//! assert_eq!(patch.upgrade().unwrap().data, "node");
//! ```
//!
//! With `subject`, a `skip` field declares an unmanaged subject field using its
//! subject type; the macro removes it from the local object. The same `partial`
//! and upgrade-default rules apply. `subject` cannot be combined with `object`,
//! struct-level `name`, or struct-level `attrs`. The object alone implements
//! [`PartialOptionized`]; the subject receives [`Optionizable`] methods and a
//! [`Schema`] for borrowed comparisons.
//!
//! ## Retaining changes
//!
//! [`Retain::retain`] removes updates already represented by a complete or partial
//! baseline, borrowing values without requiring `Clone`. It returns `true` when
//! updates remain, or `false` when the entire patch can be omitted relative to that
//! baseline. Unknown baseline fields do not remove updates.
//!
//! ```rust
//! use optionize::{optionized, Retain};
//!
//! #[optionized]
//! struct Config { enabled: bool, socket_mark: Option<u32> }
//!
//! let current = Config { enabled: true, socket_mark: Some(7) };
//! let mut patch = ConfigOptional { enabled: Some(true), socket_mark: Some(None) };
//! assert!(patch.retain(&current));
//! assert_eq!(patch.enabled, None);
//! assert_eq!(patch.socket_mark, Some(None)); // Still needs to clear the current value.
//!
//! let unknown = ConfigOptional { enabled: None, socket_mark: None };
//! assert!(patch.retain(&unknown));
//! assert_eq!(patch.socket_mark, Some(None)); // Unknown does not mean already cleared.
//!
//! let cleared = ConfigOptional { enabled: None, socket_mark: Some(None) };
//! assert!(!patch.retain(&cleared));
//! assert_eq!(patch.socket_mark, None);
//! ```
//!
//! The macro provides `Retain` automatically when the compared fields implement
//! `PartialEq`. The subject itself need not implement `PartialEq`. Nested patches
//! compare their fields recursively, and skipped fields do not participate.
//! Equality follows each field's `PartialEq` implementation; for example, a floating
//! point `NaN` compares unequal to itself and remains in the patch.
//!
//! Flattened fields stay stored because they cannot represent absence. Their
//! equality still affects the return value: **`false` does not imply that every
//! stored field is `None`**.
//!
//! ```rust
//! use optionize::{optionized, Retain};
//! #[optionized]
//! struct Config {
//!     #[optionize(flatten)]
//!     enabled: bool,
//! }
//! let mut patch = ConfigOptional { enabled: false };
//! assert!(!patch.retain(&Config { enabled: false }));
//! assert!(!patch.enabled); // The flattened value is still present.
//! assert!(patch.retain(&Config { enabled: true }));
//! ```
//!
//! A generic helper can accept either a full or partial baseline through
//! `Schema<Subject, Patch>`. The macro generates both schemas. Neither callers nor
//! generic helpers need to construct views:
//!
//! ```rust
//! use optionize::{optionized, Retain, Schema};
//!
//! fn retain_changes<Patch, Subject, Baseline>(patch: &mut Patch, baseline: &Baseline) -> bool
//! where
//!     Patch: Retain<Subject>,
//!     Baseline: Schema<Subject, Patch>,
//! {
//!     patch.retain(baseline)
//! }
//!
//! #[derive(PartialEq)]
//! struct Token(String); // Deliberately has no Clone implementation.
//! #[optionized]
//! struct Config { token: Token }
//!
//! let current = Config { token: Token("same".into()) };
//! let mut patch = ConfigOptional { token: Some(Token("same".into())) };
//! assert!(!retain_changes(&mut patch, &current));
//! let mut patch = ConfigOptional { token: Some(Token("new".into())) };
//! assert!(retain_changes(&mut patch, &ConfigOptional { token: None }));
//! assert_eq!(patch.token.unwrap().0, "new");
//! ```
//!
//! A different baseline type may implement `Schema<Subject, Patch>` without
//! implementing patch operations. Its view must be the one owned by `Patch`;
//! sharing the subject type alone does not make two object schemas interchangeable.
//! Views borrow the known fields and recursively construct nested views. For a
//! custom read-only baseline, start with `Default::default()` (all fields unknown)
//! and fill the known `v_<subject_field>` entries. View fields have the same
//! visibility as the annotated fields; tuple fields use `v_0`, `v_1`, and so on.
//!
//! ```rust
//! use optionize::{optionized, Retain, Schema};
//!
//! #[optionized]
//! struct Config { enabled: bool, port: u16 }
//! struct Status { active: bool }
//!
//! impl Schema<Config, ConfigOptional> for Status {
//!     type View<'v> = <ConfigOptional as Schema<Config>>::View<'v>;
//!     fn view<'s>(&'s self) -> <ConfigOptional as Schema<Config>>::View<'s>
//!     where
//!         Config: 's,
//!     {
//!         let mut view: <ConfigOptional as Schema<Config>>::View<'s> = Default::default();
//!         view.v_enabled = Some(&self.active);
//!         view
//!     }
//! }
//!
//! let mut patch = ConfigOptional { enabled: Some(true), port: Some(8080) };
//! assert!(patch.retain(&Status { active: true }));
//! assert_eq!(patch.enabled, None);
//! assert_eq!(patch.port, Some(8080)); // Status knows nothing about the port.
//! ```
//!
//! Comparison requirements do not restrict other operations:
//!
//! ```rust
//! use optionize::{optionized, Optionized};
//! struct Connection;
//! #[optionized]
//! struct Config { connection: Connection }
//! let _: Config = ConfigOptional { connection: Some(Connection) }.upgrade().unwrap();
//! ```
//!
//! Calling `retain` still requires comparison support:
//!
//! ```compile_fail
//! use optionize::{optionized, Retain};
//! struct Connection;
//! #[optionized]
//! struct Config { connection: Connection }
//! let mut patch = ConfigOptional { connection: Some(Connection) };
//! patch.retain(&Config { connection: Connection });
//! ```
//!
//! This also applies to nested fields, even when a particular patch omits that child:
//!
//! ```compile_fail
//! use optionize::{optionized, Retain};
//! struct Connection;
//! #[optionized]
//! struct Child { connection: Connection }
//! #[optionized]
//! struct Parent {
//!     #[optionize(nest = ChildOptional)]
//!     child: Child,
//! }
//! let mut patch = ParentOptional { child: None };
//! patch.retain(&Parent { child: Child { connection: Connection } });
//! ```
//!
//! ## Validation and errors
//!
//! `validate()` borrows the patch and collects all missing fields and nested
//! validation failures in an [`ErrorCollection`]. `upgrade()` validates and then
//! consumes the patch **on success or failure**. Validate first when an invalid
//! patch must remain available for correction. `upgrade_unchecked()` is only safe
//! when validation would succeed; ordinary callers should use `upgrade()`.
//!
//! ```rust
//! use optionize::{optionized, Error, FieldInfo, Optionized, PartialOptionized};
//!
//! #[optionized]
//! struct Endpoint { port: u16 }
//! #[optionized]
//! struct Config {
//!     #[optionize(name = "label")]
//!     name: String,
//!     note: Option<String>,
//!     #[optionize(nest = EndpointOptional)]
//!     endpoint: Endpoint,
//! }
//!
//! let mut patch = ConfigOptional {
//!     label: None,
//!     note: Some(None), // Explicitly cleared: this nullable field is complete.
//!     endpoint: Some(EndpointOptional { port: None }),
//! };
//! let errors = patch.validate().unwrap_err();
//! assert_eq!(errors.len(), 2);
//! assert!(errors.iter().any(|error| matches!(error,
//!     Error::Missing { field: FieldInfo::Renamed { original: "name", optionized: "label" }, .. }
//! )));
//! let nested = errors.iter().find(|error| matches!(error, Error::Nested { .. })).unwrap();
//! assert!(std::error::Error::source(nested).is_some());
//! // Display groups errors by mapping; iteration permits structured reporting.
//! assert!(errors.to_string().contains("Missing required field"));
//!
//! patch.merge(ConfigOptional {
//!     label: Some("node".into()),
//!     note: None,
//!     endpoint: Some(EndpointOptional { port: Some(8080) }),
//! });
//! let config = patch.upgrade().unwrap();
//! assert_eq!(config.name, "node");
//! assert_eq!(config.note, None);
//! assert_eq!(config.endpoint.port, 8080);
//! ```
//!
//! Each error identifies the subject/object mapping through [`TypeInfo`] and the
//! field through [`FieldInfo`], including both names when renamed. A nested error
//! wraps an individual child error accessible through [`core::error::Error::source`].
//!
//! ## Crate paths
//!
//! Renamed Cargo dependencies are detected automatically. When using a re-export or
//! another custom path, provide it to **`optionized`**, not the `optionize` helper:
//!
//! ```rust
//! use optionize as patches;
//! use patches::Optionized;
//!
//! #[patches::optionized(crate = patches)]
//! struct Config { port: u16 }
//!
//! assert_eq!(ConfigOptional { port: Some(8080) }.upgrade().unwrap().port, 8080);
//! ```

#![no_std]

extern crate alloc;
extern crate self as optionize;

/// Generates an optionized version of a struct, replacing its fields with `Option<Value>` where applicable,
/// and implements conversion and merge logic for partial updates and builders.
///
/// This macro generates a new struct (by default named `{OriginalName}Optional`) and implements the `PartialOptionized`
/// trait to allow seamless conversion, patching, and merging between the full and the partial struct.
/// It also provides [`Retain`] automatically when the compared fields support
/// equality; no additional attribute is needed.
///
/// ## Struct-level attributes
///
/// Configure the mapping with `#[optionize(name = "...", attrs(...), partial(...))]`,
/// optionally selecting either `object = ...` or `subject = ...`.
/// Use `#[optionized(crate = path)]` to override automatic crate-path detection.
///
/// - `name`: Overrides the generated struct's name. Use `{}` as a placeholder for the original struct name.
/// - `object`: Uses a user-defined optionized struct instead of generating one. The macro will only generate
///   trait implementations and will expect your object struct to match the fields it would normally generate.
///   Accepts a string template (`"pb::{}Optional<Value>"`) or an unquoted type path
///   (`pb::Config::<Value>`). In strings, `{}` is replaced with the subject's name.
///   Generic arguments must be explicit; unquoted generic paths use `::<Value>`.
///   This cannot be combined with `subject`, `name`, `attrs`, or `partial(marked)`.
/// - `subject`: Treats the annotated local struct as the object of an existing
///   subject, which may come from another crate. Declare ordinary object fields
///   as `Option<Value>` (aliases also work). Accepts the same string-template and
///   unquoted-path syntax as `object`. Generic update bounds use
///   `PartialOptionized<Subject>` and `Optionized<Subject>`. For `retain`, generic
///   baseline bounds use `Schema<Subject, Object>`; ordinary calls infer the mapping.
///   This cannot be combined with `object`, struct-level `name`, or struct-level `attrs`.
/// - `attrs`: By default, the generated struct inherits all attributes from the original struct (except `#[optionize(...)]`).
///   With ordinary attributes only, `attrs(...)` replaces those attributes;
///   for example, `attrs(derive(Debug))` makes the generated struct only derive `Debug`.
///   Use `+path` to select original attributes by exact path, `..` to select all,
///   and `-path` to exclude attributes from that selection. Ordinary attributes are
///   appended afterward. Thus `attrs(.., -derive, derive(Debug))` replaces only the
///   original derive lists, preserving other original attributes.
///   Selectors accept whole attribute paths, not individual entries inside `derive(...)`.
///   Repeated lists are combined; `attrs()` alone clears inherited attributes but
///   does not reset other lists. Inherited attributes keep their order and are
///   copied at most once per original attribute; explicit attributes are not deduplicated.
/// - `partial`: Disables the validation and upgrading generated by default, while allowing skipped fields.
///   - `upgradable`: Implements the `Optionized` trait, allowing the partial struct to be validated and "upgraded" to the full struct.
///   - `marked`: If the original struct has type parameters or lifetimes that are not used by the generated struct (e.g., due to skipped fields),
///     the generated struct will fail to compile. `marked` injects a `PhantomData` field to consume those generic parameters.
///     - For unit structs, this changes the generated struct to a tuple struct (or a named struct if `name` is specified).
///     - Use `marked(name = my_marker)` to explicitly name the injected `PhantomData` field.
///     - `marked(attrs(...))` applies the same selection rules to the marker's default `#[doc(hidden)]` attribute.
///
/// ## Field-level attributes
///
/// `#[optionize(name = "...", attrs(...), flatten, skip, default = callback, nest = "...")]`
///
/// - `name`: Renames a named field in the generated struct. Use `{}` as a placeholder for its original name.
///   Tuple fields cannot be renamed.
///   With `subject = ...`, names the corresponding subject field instead.
/// - `attrs`: Applies the same attribute-selection rules as struct-level `attrs`
///   to the generated field. For example, `attrs(+doc)` inherits only its documentation.
/// - `flatten`: Instructs the macro **not** to wrap the field's type in `Option<Value>`. The field will have the exact same type in the generated struct.
/// - `skip`: Removes the field entirely from the generated struct.
///   - Requires `partial` or `partial(upgradable)` on the struct.
///   - With `subject = ...`, declare the unmanaged subject field with its subject type;
///     the macro removes it from the local object.
///   - If skipping fields leaves a generic parameter unused in a generated object, use `marked` to consume it.
///   - Uses the same `default` initializer as ordinary fields, implicitly defaulting to `Default::default()`.
/// - `default`: Fills a missing field during upgrading with its subject type's `Default::default()`.
///   `default = callback` accepts `fn(&Object) -> FieldType`, including non-capturing closures.
///   Skipped fields always use the initializer. Callbacks run after initial validation, before any
///   fields are moved, and see the original object. Explicit clears never trigger defaults.
///   Defaults do not affect patching, merging, or retaining, and cannot be combined with `flatten`.
/// - `nest = Type` or `nest = "Type"`: Delegates this field to a nested object that implements
///   `PartialOptionized` (and `Optionized` when upgrading is enabled). With `subject = ...`,
///   name the nested subject instead; the declared field already supplies its object type.
///   Generic arguments must be explicit. String paths for `nest` do not substitute `{}`.
///
/// ## Field Visibility
/// The generated struct and its fields **strictly retain the visibility** of the original struct and its fields.
pub use optionize_macros::optionized;

#[cfg(test)]
mod tests;

mod error;
mod retain;

pub use error::{Error, ErrorCollection, FieldInfo, TypeInfo};
pub use retain::{Retain, Schema};

#[doc(hidden)]
pub mod __private {
    pub extern crate alloc;

    pub use crate::retain::*;
    pub use optionize_macros::{Optionize, Prepare, discard, expand};
}

/// Relates a partial object to its complete subject.
/// Allows extracting partial data from a full struct, applying partial data to a full struct,
/// and merging two partial structs together.
///
/// The macro implements this trait for the optionized object. Complete subjects
/// expose borrowed baselines through [`Schema`] and receive [`Optionizable`]
/// extension methods. They do not implement this partial-update trait:
///
/// ```compile_fail,E0277
/// use optionize::{optionized, PartialOptionized};
/// #[optionized]
/// struct Config { enabled: bool }
/// fn accept_partial<Patch: PartialOptionized<Config>>(_: Patch) {}
/// accept_partial(ConfigOptional { enabled: Some(true) });
/// accept_partial(Config { enabled: true });
/// ```
pub trait PartialOptionized<Subject>: Sized {
    /// Consumes the subject and converts it into its optionized version.
    /// Generated implementations populate managed fields, recursively optionizing
    /// nested fields. Flattened fields keep their unwrapped form; skipped fields
    /// are discarded.
    fn optionize(subject: Subject) -> Self;

    /// Patches the provided subject struct with values from this optionized struct.
    /// If a field is `Some`, it will overwrite the subject's corresponding field.
    /// If it is `None`, the subject's field remains unchanged. Generated nested
    /// fields patch recursively; flattened fields always participate, and skipped
    /// subject fields remain unchanged.
    fn patch(self, subject: &mut Subject);

    /// Merges another optionized struct into this one.
    /// By default, `Some` values from the `other` struct will overwrite values in `self`.
    /// `None` values from the `other` struct will leave `self` unchanged.
    /// Generated nested fields merge recursively; flattened fields always participate.
    fn merge(&mut self, other: Self);
}

/// Provides extension methods on the original subject struct to easily work with its
/// [`PartialOptionized`] counterpart. The object type is usually inferred from
/// the supplied patch or expected return type.
pub trait Optionizable<Object: PartialOptionized<Self>>: Sized {
    /// Loads values from the provided partial struct into `self`.
    /// Delegates to [`PartialOptionized::patch`], including nested and flattened field behavior.
    fn load(&mut self, object: Object) {
        object.patch(self);
    }

    /// Converts `self` into its partial/optionized version.
    /// Delegates to [`PartialOptionized::optionize`], populating managed fields.
    /// Skipped subject fields are discarded.
    fn downgrade(self) -> Object {
        Object::optionize(self)
    }
}

/// Validates a partial object's completeness and upgrades it to the full subject.
///
/// The macro implements this trait by default. `partial` disables it;
/// `partial(upgradable)` restores it while allowing skipped fields.
///
/// `Subject` is a trait parameter so a crate can implement this trait for an
/// external object (e.g. generated protobuf) when the subject is local. There is
/// one implementation per `(Self, Subject)` combination, allowing several
/// subjects per object. Calls infer the subject when the complete type is uniquely
/// determined. A generic helper names the subject in its bound and can use the
/// corresponding associated error type:
///
/// ```rust
/// use optionize::{optionized, Optionized};
///
/// fn upgrade<Patch, Subject>(patch: Patch) -> Result<Subject, Patch::Errors>
/// where
///     Patch: Optionized<Subject>,
/// {
///     patch.upgrade()
/// }
///
/// #[optionized]
/// struct Config<Value> { value: Value }
/// let config = upgrade(ConfigOptional { value: Some(String::from("node")) }).unwrap();
/// assert_eq!(config.value, "node");
/// ```
///
/// If an object has several subjects, select one through the result type for
/// `upgrade()`, or use `Optionized::<Subject>::validate(&object)` for validation:
///
/// ```rust
/// use optionize::{optionized, Optionized};
/// struct Patch { value: Option<u32> }
/// #[optionized]
/// #[optionize(object = Patch)]
/// struct First { value: u32 }
/// #[optionized]
/// #[optionize(object = Patch)]
/// struct Second { value: u32 }
///
/// let patch = Patch { value: Some(7) };
/// Optionized::<First>::validate(&patch).unwrap();
/// let first: First = patch.upgrade().unwrap();
/// assert_eq!(first.value, 7);
/// let second = Optionized::<Second>::upgrade(Patch { value: Some(9) }).unwrap();
/// assert_eq!(second.value, 9);
/// ```
///
/// An unconstrained validation call cannot select between those subjects:
///
/// ```compile_fail,E0283
/// # use optionize::{optionized, Optionized};
/// # struct Patch { value: Option<u32> }
/// # #[optionized]
/// # #[optionize(object = Patch)]
/// # struct First { value: u32 }
/// # #[optionized]
/// # #[optionize(object = Patch)]
/// # struct Second { value: u32 }
/// Patch { value: Some(1) }.validate().unwrap();
/// ```
///
/// Neither can an upgrade whose result has no known type:
///
/// ```compile_fail,E0283
/// # use optionize::{optionized, Optionized};
/// # struct Patch { value: Option<u32> }
/// # #[optionized]
/// # #[optionize(object = Patch)]
/// # struct First { value: u32 }
/// # #[optionized]
/// # #[optionize(object = Patch)]
/// # struct Second { value: u32 }
/// let _ = Patch { value: Some(1) }.upgrade().unwrap();
/// ```
///
/// Even one generic implementation needs type information if a subject parameter
/// cannot be determined from the object. Here `Value` exists only on `Config<Value>`:
///
/// ```compile_fail,E0282
/// use core::marker::PhantomData;
/// use optionize::{optionized, Optionized};
/// struct Patch { value: Option<u32> }
/// #[optionized]
/// #[optionize(object = Patch, partial(upgradable))]
/// struct Config<Value> {
///     value: u32,
///     #[optionize(skip)]
///     marker: PhantomData<Value>,
/// }
/// let _ = Patch { value: Some(1) }.upgrade().unwrap();
/// ```
///
/// Provide the missing argument through `Config<Value>` or the trait parameter:
///
/// ```rust
/// # use core::marker::PhantomData;
/// # use optionize::{optionized, Optionized};
/// # struct Patch { value: Option<u32> }
/// # #[optionized]
/// # #[optionize(object = Patch, partial(upgradable))]
/// # struct Config<Value> {
/// #     value: u32,
/// #     #[optionize(skip)]
/// #     marker: PhantomData<Value>,
/// # }
/// let config: Config<String> = Patch { value: Some(7) }.upgrade().unwrap();
/// assert_eq!(config.value, 7);
/// Optionized::<Config<String>>::validate(&Patch { value: Some(7) }).unwrap();
/// ```
#[diagnostic::on_unimplemented(
    message = "The type `{Self}` cannot be upgraded to `{Subject}`",
    label = "Nested type lacks upgrade logic",
    note = "Ensure the subject of `{Self}` is not annotated partial, or is annotated with `#[optionize(partial(upgradable))]`"
)]
pub trait Optionized<Subject>: PartialOptionized<Subject> {
    /// Validation failures; generated implementations use [`ErrorCollection`].
    /// Custom collections may carry non-static borrows. Their yielded errors must
    /// be `Send + Sync + 'static` so nested validation can store them as sources.
    type Errors: IntoIterator<Item: core::error::Error + Send + Sync + 'static>;

    /// Validates that all fields inside the optionized struct that are required for upgrading
    /// contain a value (i.e. are `Some`).
    /// Returns a collection of errors if any fields are missing or if nested validations fail.
    fn validate(&self) -> Result<(), Self::Errors>;

    /// Constructs the full subject from an object that has already been validated.
    /// Generated implementations prepare field defaults before moving fields and
    /// still check nested objects when consuming them, since callbacks can affect
    /// shared interior state.
    ///
    /// # Safety
    /// The object must pass `Optionized::<Subject>::validate()` before this call.
    /// Implementations may rely on that guarantee for unchecked field extraction.
    unsafe fn upgrade_unchecked(self) -> Subject;

    /// Validates and upgrades the optionized struct into the full subject struct.
    /// Returns `Ok(Subject)` if all required fields are present, otherwise returns `Err(Self::Errors)`.
    /// Consumes the object in either case; call [`Self::validate`] first to keep an
    /// invalid patch available for correction.
    fn upgrade(self) -> Result<Subject, Self::Errors> {
        self.validate()?;
        Ok(unsafe { self.upgrade_unchecked() })
    }
}
