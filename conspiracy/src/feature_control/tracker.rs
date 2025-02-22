//! Included [`FeatureTracker`] implementations.

use std::{any::Any, sync::Arc};

use arc_swap::ArcSwap;

use crate::feature_control::{
    set_global_tracker, FeatureSet, FeatureTracker, SetGlobalTrackerError,
};

/// A general purpose [`FeatureTracker`] with support for:
/// - Set using:
///     - The type's default values
///     - State value (which is generated from a builder)
pub struct ConspiracyFeatureTracker<T: FeatureSet> {
    state: ArcSwap<T::State>,
}

impl<T: FeatureSet> ConspiracyFeatureTracker<T> {
    pub fn from_default() -> Self {
        Self {
            state: ArcSwap::new(Arc::new(T::State::default())),
        }
    }

    /// Use the generated state builder to create apply custom, static values:
    ///
    /// ```rust
    /// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
    /// conspiracy::feature_control::define_features!(pub enum Features { Foo => false });
    ///
    /// let state = Features::builder()
    ///     .foo(true)
    ///     .build();
    ///
    /// let result = ConspiracyFeatureTracker::<Features>::from_static(state)
    ///     .set_as_global_tracker();
    /// ```
    pub fn from_static(state: T::State) -> Self {
        Self {
            state: ArcSwap::new(Arc::new(state)),
        }
    }

    /// Convenience function for applying the tracker as the global default rather than having to
    /// specify the generics matching generated types:
    ///
    /// ```rust
    /// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
    /// conspiracy::feature_control::define_features!(pub enum Features { Foo => true });
    ///
    /// let result = set_global_tracker::<FeaturesState, ConspiracyFeatureTracker<Features>>(
    ///     ConspiracyFeatureTracker::from_static(FeaturesState::default()),
    /// );
    /// ```
    pub fn set_as_global_tracker(self) -> Result<(), SetGlobalTrackerError> {
        set_global_tracker::<T::State, Self>(self)
    }
}

impl<T: FeatureSet> FeatureTracker for ConspiracyFeatureTracker<T> {
    fn static_feature_state(&self) -> Arc<dyn Any + Send + Sync> {
        self.state.load().clone()
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
