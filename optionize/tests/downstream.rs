//! Mappings generated in a Rust 2021 crate that denies lints on generated code.

use core::marker::PhantomData;

use optionize::{Optionizable, Optionized, PartialOptionized, Retain};
use optionize_test_downstream::external::{Peer, WirePatch};
use optionize_test_downstream::{
    Config, ConfigOptional, Endpoint, EndpointOptional, Interval, IntervalOptional, Pair,
    PairOptional, PeerPatch, Seconds, Wire,
};

fn endpoint(port: u16) -> Endpoint {
    Endpoint {
        host: "localhost".into(),
        port,
    }
}

fn config() -> Config<'static, u8> {
    Config {
        name: "node",
        label: None,
        verbose: false,
        endpoint: endpoint(80),
        fallback: endpoint(81),
        timeout: Seconds(5),
        retries: 1,
        data: 7,
    }
}

#[test]
fn generated_mappings_run_every_operation() {
    let mut patch = ConfigOptional {
        name: Some("node"),
        label: Some(Some("edge".into())),
        verbose: true,
        endpoint: Some(EndpointOptional {
            host: None,
            port: Some(80),
        }),
        fallback: EndpointOptional {
            host: None,
            port: Some(82),
        },
        timeout: Some(9),
        retries: None,
        _marker: PhantomData,
    };
    assert!(patch.retain(&config()));
    assert_eq!(patch.name, None);
    assert_eq!(patch.endpoint, None);
    assert_eq!(patch.label, Some(Some("edge".into())));

    patch.merge(ConfigOptional {
        name: Some("edge"),
        label: None,
        verbose: true,
        endpoint: None,
        fallback: EndpointOptional {
            host: None,
            port: None,
        },
        timeout: None,
        retries: Some(2),
        _marker: PhantomData,
    });
    let mut config = config();
    config.load(patch);
    assert_eq!(config.name, "edge");
    assert_eq!(config.label.as_deref(), Some("edge"));
    assert!(config.verbose);
    assert_eq!(config.fallback, endpoint(82));
    assert_eq!(config.timeout, Seconds(9));
    assert_eq!(config.retries, 2);

    let mut patch: ConfigOptional<u8> = config.downgrade();
    patch.retries = None;
    let config: Config<u8> = patch.upgrade().unwrap();
    assert_eq!(config.retries, 3);
    assert_eq!(config.data, 0);
    assert_eq!(config.timeout, Seconds(9));
}

#[test]
fn conversion_functions_run_in_both_directions() {
    let patch = Interval {
        every: core::time::Duration::from_secs(2),
    }
    .downgrade();
    assert_eq!(patch, IntervalOptional { every: Some(2) });
    let interval: Interval = patch.upgrade().unwrap();
    assert_eq!(interval.every.as_secs(), 2);
}

#[test]
fn tuple_and_external_mappings_run_every_operation() {
    let pair: Pair = PairOptional(Some(1), Some("one".into())).upgrade().unwrap();
    assert_eq!(pair, Pair(1, "one".into()));
    assert!(PairOptional(None, None).validate().is_err());

    let mut peer = Peer {
        address: "localhost".into(),
        weight: Some(1),
    };
    let mut patch = PeerPatch {
        host: Some("localhost".into()),
        weight: Some(None),
    };
    assert!(patch.retain(&peer));
    assert_eq!(patch.host, None);
    peer.load(patch);
    assert_eq!(peer.weight, None);
    let patch: PeerPatch = peer.downgrade();
    let peer: Peer = patch.upgrade().unwrap();
    assert_eq!(peer.address, "localhost");

    let patch: WirePatch = Wire { value: 4 }.downgrade();
    assert_eq!(patch.value, Some(4));
    let wire: Wire = patch.upgrade().unwrap();
    assert_eq!(wire, Wire { value: 4 });
}
