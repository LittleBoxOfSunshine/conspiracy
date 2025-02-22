use conspiracy::feature_control::FeatureEnabledError;
use conspiracy_macros::{
    define_features, feature_enabled, feature_enabled_or, feature_enabled_or_default,
    try_feature_enabled,
};

mod generated {
    use conspiracy_macros::define_features;

    define_features!(
        pub enum Features {
            Foo => true,
            Bar => false,
        }
    );
}

use generated::*;

// noinspection RsUnnecessaryQualifications
#[test]
fn no_global_registered_no_panic_under_cfg_test() {
    // We're under `#[cfg(test)]`, so this should return default rather than panic.
    let bar_feature: bool = feature_enabled!(generated::Features::Bar);
    assert_eq!(FeaturesState::default_bar(), bar_feature);

    let bar_feature: bool = feature_enabled_or_default!(Features::Bar);
    assert_eq!(FeaturesState::default_bar(), bar_feature);

    // Confirm the failure gets replaced with default, so default must be inverse
    let expected_bar = !FeaturesState::default_bar();
    assert_eq!(
        expected_bar,
        feature_enabled_or!(Features::Bar, expected_bar)
    );

    // Fully specified path
    assert_eq!(
        expected_bar,
        feature_enabled_or!(crate::generated::Features::Bar, expected_bar)
    );

    let foo_feature: Result<bool, FeatureEnabledError> = try_feature_enabled!(Features::Foo);
    assert!(foo_feature.is_err());

    // Compile fail example: "No variant Cat on Features"
    // try_feature_enabled(Features::Cat)

    define_features!(
        pub enum Features2 {
            Foo => true,
            Bar => false,
        }
    );

    assert!(feature_enabled!(Features2::Foo));
    assert!(!feature_enabled!(Features2::Bar));
}
