use std::{sync::Arc, time::Duration};

use conspiracy::config::{
    as_shared_fetcher, config_struct, shared_fetcher_from_fn, shared_fetcher_from_static, AsField,
    RestartRequired, SharedConfigFetcher,
};
use conspiracy_macros::arcify;
use serde_with::serde_as;

mod wrapper {
    use conspiracy_macros::config_struct;
    // Confirm pub(super) can be passed. This can't be used in the root module of the test.
    config_struct!(
        pub(crate) struct Foo {
            e: pub(super) struct Bar {
                f: struct Cow {
                    foo: u32,
                }
            }
        }
    );
}

config_struct!(
    pub struct ConfigA {
        foo: u32,
        bar: struct ConfigB {
            foo: u32,
            bar: pub struct ConfigC {
                foo: u32,
                bar: u32,
            }
        },
        d: pub(crate) struct ConfigD {
            e: pub struct ConfigE {
                f: struct ConfigF {
                    foo: String,
                }
            }
        }
    }
);

config_struct!(
    #[serde_as]
    #[serde(deny_unknown_fields)]
    pub struct WithAttributesTest {
        #[serde(default)]
        foo: u32,
        nested_no_attributes: pub struct NestedWithoutAttributes {
            #[restart]
            bar: u32,
            #[restart]
            nested_with_attributes:
                #[serde_as]
                #[serde(rename_all = "camelCase")]
                pub struct NestedWithAttributes {
                    #[serde_as(as = "DurationMilliseconds<u64>")]
                    timeout: Duration,
            },
            #[restart]
            only_struct_level_restart: pub struct OnlyStructLevelRestart {
                foo: u32,
            }
        },
        #[serde_as(as = "DurationSeconds<u64>")]
        timeout: Duration,
    }
);

fn with_attributes_base() -> WithAttributesTest {
    arcify!(WithAttributesTest {
        foo: 0,
        nested_no_attributes: NestedWithoutAttributes {
            bar: 0,
            nested_with_attributes: NestedWithAttributes {
                timeout: Default::default()
            },
            only_struct_level_restart: OnlyStructLevelRestart { foo: 0 }
        },
        timeout: Default::default(),
    })
}

#[test]
fn whole_struct_marked_and_changed_restart() {
    let config = with_attributes_base();
    let mut other_config = config.clone().compact();
    other_config
        .nested_no_attributes
        .only_struct_level_restart
        .foo = 50;
    let other_config = other_config.arcify();

    assert!(config.restart_required(&other_config));
}

#[test]
fn nested_config_field_changed_restart() {
    let config = with_attributes_base();
    let mut other_config = config.clone().compact();
    other_config.nested_no_attributes.bar = 50;
    let other_config = other_config.arcify();

    assert!(config.restart_required(&other_config));
}

#[test]
fn manual_construction() {
    let _test = ConfigA {
        foo: 5,
        bar: Arc::new(ConfigB {
            foo: 5,
            bar: Arc::new(ConfigC { foo: 0, bar: 0 }),
        }),
        d: Arc::new(ConfigD {
            e: Arc::new(ConfigE {
                f: Arc::new(ConfigF {
                    foo: "yo".to_string(),
                }),
            }),
        }),
    };
}

#[test]
fn arcify_basic() {
    arcify!(ConfigE {
        f: ConfigF {
            foo: "yo".to_string()
        },
    });
}

#[test]
fn arcify_idents_and_expressions() {
    let val = 5;

    arcify!(ConfigC {
        foo: val,
        bar: 2 + 2 + 1
    });
}

#[test]
fn arcify_complex() {
    sample_config();
}

fn sample_config() -> ConfigA {
    let val = 5;

    arcify!(ConfigA {
        foo: 1,
        bar: ConfigB {
            foo: val,
            bar: ConfigC {
                foo: 2 + 5,
                bar: if val > 0 { 2 } else { 1 }
            },
        },
        d: ConfigD {
            e: ConfigE {
                f: ConfigF {
                    foo: "yo".to_string()
                },
            },
        },
    })
}

#[test]
fn sub_config_conversion() {
    let sample = sample_config();

    convert_from_a(shared_fetcher_from_static(sample.clone()));
    uses_b(as_shared_fetcher(&shared_fetcher_from_static(sample)));
}

fn convert_from_a(a_fetcher: SharedConfigFetcher<ConfigA>) {
    uses_b(as_shared_fetcher(&a_fetcher));
    uses_c(as_shared_fetcher(&a_fetcher));
    uses_b(shared_fetcher_from_fn(move || {
        a_fetcher.latest_snapshot().share()
    }));
}

fn uses_b(b_fetcher: SharedConfigFetcher<ConfigB>) {
    std::thread::spawn(move || format!("{}", b_fetcher.latest_snapshot().foo));
}

fn uses_c(c_fetcher: SharedConfigFetcher<ConfigC>) {
    let mut c_compact = c_fetcher.latest_snapshot().compact();
    c_compact.foo += 1;
    let mock_config = c_compact.arcify();
    let mock_c_fetcher = shared_fetcher_from_static(mock_config);

    let _ = format!("{}", mock_c_fetcher.latest_snapshot().foo);
}
