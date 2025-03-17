use conspiracy::feature_control::{
    tracker::{ConspiracyFeatureTracker, StaticFetcher},
    SetGlobalTrackerError,
    SetGlobalTrackerError::GlobalTrackerAlreadySet,
};
use conspiracy_macros::feature_enabled;

mod generated {
    use conspiracy_macros::define_features;

    define_features!(
        pub enum Features {
            Foo => false,
            Bar => false,
        }
    );

    // Assert various valid enum syntaxes generate valid code.
    define_features!(
        enum EmptyFeatures {}
    );
    define_features!(enum SingleFeature { Foo => true, });
    define_features!(enum SingleFeatureNoTrailingComma { Foo => true });
    define_features!(
        enum MultipleFeaturesNoTrailingComma {
            Foo => true,
            Bar => true
        }
    );
}

use crate::generated::{Features, FeaturesState};

#[test]
fn global_registered_overrides_defaults_always_ok() {
    // Inverses confirm our mock state is being used rather than unwittingly returning defaults as
    // a result of being under `#[cfg(test)]`
    set_inverse_defaults_global().unwrap();

    // Second set will be rejected
    let failure = set_inverse_defaults_global().unwrap_err();
    assert!(std::matches!(failure, GlobalTrackerAlreadySet));

    let expected_foo = !FeaturesState::default_foo();
    let expected_bar = !FeaturesState::default_bar();

    assert_eq!(expected_foo, feature_enabled!(Features::Foo));
    assert_eq!(expected_bar, feature_enabled!(Features::Bar));
}

fn set_inverse_defaults_global() -> Result<(), SetGlobalTrackerError> {
    let state = Features::builder()
        .foo(!FeaturesState::default_foo())
        .bar(!FeaturesState::default_bar())
        .build();

    ConspiracyFeatureTracker::<Features, StaticFetcher<Features>>::from_static(state)
        .set_as_global_tracker()
}
