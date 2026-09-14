use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain, optionized};

#[optionized]
#[optionize(attrs(.., -derive, derive(Debug, Default, PartialEq)))]
#[derive(Debug, Clone, PartialEq)]
struct Config {
    enabled: bool,
    count: u32,
}

#[test]
fn attributes_replace_whole_derives_and_preserve_operations() {
    let mut subject = Config {
        enabled: true,
        count: 7,
    };
    let mut patch = ConfigOptional::default();
    patch.merge(ConfigOptional {
        enabled: Some(true),
        count: Some(0),
    });
    assert!(patch.retain(&subject));
    assert_eq!(
        patch,
        ConfigOptional {
            enabled: None,
            count: Some(0),
        }
    );
    subject.load(patch);
    assert_eq!(
        subject.downgrade().upgrade().unwrap(),
        Config {
            enabled: true,
            count: 0,
        }
    );
}

#[optionized]
#[optionize(attrs(+derive), attrs(), attrs(+derive, derive(Default)))]
#[derive(Debug, Clone, PartialEq)]
struct Selected {
    /// The value to update.
    #[optionize(attrs(+doc, doc = "Additional field documentation."))]
    value: u32,
}

#[test]
fn attributes_combine_inherited_and_added_derives_across_lists() {
    let mut patch = SelectedOptional::default();
    patch.merge(Selected { value: 3 }.downgrade());
    assert_eq!(patch.clone(), patch);
    assert_eq!(patch.upgrade().unwrap(), Selected { value: 3 });
}

#[optionized]
#[cfg_attr(
    all(),
    optionize(attrs(.., -derive, derive(Debug, Default, PartialEq)))
)]
#[derive(Debug, PartialEq)]
struct Tuple(
    #[cfg(any())]
    #[optionize(attrs(+))]
    MissingType,
    #[cfg_attr(all(), optionize(attrs(+doc)))]
    /// Whether the tuple is enabled.
    bool,
    #[cfg_attr(all(), cfg_attr(all(), optionize(attrs(.., -doc))))]
    /// The original label.
    String,
);

#[test]
fn attributes_handle_cfg_attr_selectors_and_filtered_tuple_fields() {
    let mut subject = Tuple(true, String::from("old"));
    let patch = TupleOptional(None, Some(String::from("new")));
    subject.load(patch);
    assert_eq!(subject, Tuple(true, String::from("new")));
    assert_eq!(
        subject.downgrade(),
        TupleOptional(Some(true), Some(String::from("new")))
    );
}

#[optionized]
#[optionize(
    attrs(.., -derive, derive(Debug, Default, PartialEq)),
    partial(marked(name = marker, attrs(.., doc = "The subject marker.")), upgradable)
)]
#[derive(Debug, PartialEq)]
struct Marked<Value: Default> {
    #[optionize(skip)]
    value: Value,
    enabled: bool,
}

#[test]
fn attributes_support_markers_for_skipped_generic_fields() {
    let patch = MarkedOptional::<String> {
        enabled: Some(false),
        marker: PhantomData,
    };
    assert_eq!(
        patch.upgrade().unwrap(),
        Marked {
            value: String::new(),
            enabled: false,
        }
    );
    assert_eq!(MarkedOptional::<String>::default().enabled, None);
}
