//! Included [`FeatureTracker`] implementations.

use std::{any::Any, marker::PhantomData, sync::Arc};

use conspiracy_theories::config::ConfigFetcher;

use crate::feature_control::{
    set_global_tracker, FeatureSet, FeatureTracker, SetGlobalTrackerError,
};

/// A general purpose [`FeatureTracker`] with support for:
/// - Set using:
///     - The type's default values
///     - State value (which is generated from a builder)
pub struct ConspiracyFeatureTracker<T: FeatureSet, F: ConfigFetcher<T::State>> {
    state_fetcher: F,
    phantom: PhantomData<T>,
}

/// A general purpose [`ConfigFetcher`] that supplies a static [`FeatureSet`] value as a snapshot.
pub struct StaticFetcher<T: FeatureSet> {
    state: Arc<T::State>,
}

impl<T: FeatureSet> ConfigFetcher<T::State> for StaticFetcher<T> {
    fn latest_snapshot(&self) -> Arc<T::State> {
        self.state.clone()
    }
}

impl<T: FeatureSet> ConspiracyFeatureTracker<T, StaticFetcher<T>> {
    /// Initialize using the default value of `T`.
    pub fn from_default() -> Self {
        Self::from_static(T::State::default())
    }

    /// Use the generated state builder to create apply custom, static values:
    ///
    /// ```rust
    /// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
    /// use conspiracy::feature_control::tracker::StaticFetcher;
    ///
    /// conspiracy::feature_control::define_features!(pub enum Features { Foo => false });
    ///
    /// let state = Features::builder()
    ///     .foo(true)
    ///     .build();
    ///
    /// let result = ConspiracyFeatureTracker::<Features, StaticFetcher<Features>>::from_static(state)
    ///     .set_as_global_tracker();
    /// ```
    pub fn from_static(state: T::State) -> Self {
        Self {
            state_fetcher: StaticFetcher {
                state: Arc::new(state),
            },
            phantom: PhantomData,
        }
    }
}

impl<T: FeatureSet, F: ConfigFetcher<T::State> + 'static> ConspiracyFeatureTracker<T, F> {
    /// Convenience function for applying the tracker as the global default rather than having to
    /// specify the generics matching generated types:
    ///
    /// ```rust
    /// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
    /// use conspiracy::feature_control::tracker::StaticFetcher;
    ///
    /// conspiracy::feature_control::define_features!(pub enum Features { Foo => true });
    ///
    /// let result = set_global_tracker::<FeaturesState, ConspiracyFeatureTracker<Features, StaticFetcher<Features>>>(
    ///     ConspiracyFeatureTracker::from_static(FeaturesState::default()),
    /// );
    /// ```
    pub fn set_as_global_tracker(self) -> Result<(), SetGlobalTrackerError> {
        set_global_tracker::<T::State, Self>(self)
    }
}

impl<T: FeatureSet, F: ConfigFetcher<T::State> + 'static> FeatureTracker
    for ConspiracyFeatureTracker<T, F>
{
    fn static_feature_state(&self) -> Arc<dyn Any + Send + Sync> {
        self.state_fetcher.latest_snapshot()
    }
}

/// Implementation detail of the global tracker state. This is the initial state before [`set_global_tracker`]
/// is called. This is used to force a panic in [`feature_enabled`] when [`set_global_tracker`] was
/// never called.
pub(super) struct NoTracker;

const PANIC_MESSAGE: &str =
    "No global tracker found, must be initialized with `set_global_tracker`";
impl FeatureTracker for NoTracker {
    fn static_feature_state(&self) -> Arc<dyn Any + Send + Sync> {
        panic!("{}", PANIC_MESSAGE)
    }
}
