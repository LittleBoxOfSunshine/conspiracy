use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use conspiracy::config::{as_shared_fetcher, into_shared_fetcher, SharedConfigFetcher};
use conspiracy_macros::config_struct;
use conspiracy_theories::config::ConfigFetcher;

config_struct!(
    struct Foo {
        val: u32,
        bar: struct Bar {
            val: u32,
        }
    }
);

struct GatedFetcher {
    config1: Arc<Foo>,
    config2: Arc<Foo>,
    counter: Arc<AtomicU32>,
    cut_over: u32,
}

impl ConfigFetcher<Foo> for GatedFetcher {
    fn latest_snapshot(&self) -> Arc<Foo> {
        self.counter.fetch_add(1, Ordering::SeqCst);

        if self.counter.load(Ordering::SeqCst) > self.cut_over {
            self.config2.clone()
        } else {
            self.config1.clone()
        }
    }
}

fn make_fetcher() -> GatedFetcher {
    GatedFetcher {
        config1: Arc::new(Foo {
            val: 0,
            bar: Arc::new(Bar { val: 0 }),
        }),
        config2: Arc::new(Foo {
            val: 1,
            bar: Arc::new(Bar { val: 1 }),
        }),
        counter: Arc::new(AtomicU32::new(0)),
        cut_over: 2,
    }
}

// Just confirms the test type does what is expected
#[test]
fn gated_fetcher_rolls_over() {
    let fetcher = into_shared_fetcher(make_fetcher());

    assert_eq!(0, fetcher.latest_snapshot().val);
    assert_eq!(0, fetcher.latest_snapshot().val);
    assert_eq!(1, fetcher.latest_snapshot().val);
}

// Confirms the sub_fetcher impacts fetcher, and isn't just working off copied data
#[test]
fn gated_fetcher_sub_fetcher_increases_counter() {
    let fetcher = into_shared_fetcher(make_fetcher());
    let sub_fetcher: SharedConfigFetcher<Bar> = as_shared_fetcher(&fetcher);

    assert_eq!(0, sub_fetcher.latest_snapshot().val);
    assert_eq!(0, sub_fetcher.latest_snapshot().val);
    assert_eq!(1, sub_fetcher.latest_snapshot().val);
}

// Actual scenario test
#[test]
fn sub_fetcher_observes_changes() {
    let fetcher = into_shared_fetcher(make_fetcher());
    let sub_fetcher: SharedConfigFetcher<Bar> = as_shared_fetcher(&fetcher);

    let snapshot = fetcher.latest_snapshot();
    assert_eq!(0, snapshot.val);
    assert_eq!(0, snapshot.bar.val);
    assert_eq!(0, sub_fetcher.latest_snapshot().val);

    let snapshot_n_plus_1 = fetcher.latest_snapshot();
    assert_eq!(1, snapshot_n_plus_1.val);
    assert_eq!(1, snapshot_n_plus_1.bar.val);
    // Confirm the change is reflected in the nested fetcher
    assert_eq!(1, sub_fetcher.latest_snapshot().val);
}
