//! # Feature Control
//!
//! A collection of macros and supporting types for defining and asserting feature state. A simple
//! quasi-enum defines the app's features and default states and assertion macros abstract away the
//! implementation to simple function calls accepting an associated enum variant. This provides many
//! benefits:
//!
//! - Strong static typing of features and all associated benefits. The compiler checks for errors,
//!    your IDE can give type + doc annotations, etc.
//! - Feature control ergonomics with static functions (more on this below).
//! - Reuses the config features offered by this crate enabling you to define performant, safe,
//!     dynamic determination of state at runtime without having to introduce a second set of
//!     semantics for these portions. The full feature set of the [`config`][crate::config] module
//!     is available including the ability to mix and match / customize implementations.
//! - Abstracts out the implementation of the global tracker state and asserting the tracker state
//!     generically. This means the same interface can be backed by hard-coded values, dynamic
//!     configuration, or any other custom implementation.
//!
//! # Defaults and Unit Testing
//!
//! Feature state is tracked using macros instead of functions to make testing less burdensome. When
//! you have static dependencies, the shared state interferes with testing and needs to be
//! determined automatically to avoid pain. This means that the production code path would have
//! defaults that could be silently picked up, which isn't desirable in workloads at scale. The
//! macros allow having a different behavior under test, use the default, vs in the non-test build.
//! You can also still explicitly opt into a default being used in the non-test build by opting to
//! use [`feature_enabled_or_default`].
//!
//! # Usage
//!
//! ```rust
//! use conspiracy::feature_control::{
//!     define_features, feature_enabled, feature_enabled_or,
//!     tracker::ConspiracyFeatureTracker
//! };
//!
//! define_features!(
//!     pub enum MyAppFeatures {
//!         OptimizedHashComputation => true,
//!         UseQuic => false,
//!     }
//! );
//!
//! // No global tracker registered yet, returns provided value.
//! feature_enabled_or!(MyAppFeatures::UseQuic, true);
//!
//! ConspiracyFeatureTracker::<MyAppFeatures>::from_default()
//!     .set_as_global_tracker()
//!     .unwrap();
//!
//! // Yields `true`. Note, would panic if called before global tracker was registered,
//! // except when compiled under `#[cfg(test)]` in which case it too returns the default.
//! feature_enabled!(MyAppFeatures::OptimizedHashComputation);
//! ```
//!
//! # Primer: Feature Control vs Configuration
//!
//! Feature control is not configuration. Configuration forms a part of your public interface. Not
//! doing so would be a hidden side effect and best and bad coupling at worst. But feature control
//! is different. Feature control is more analogous to compiling multiple variants of your
//! application, but that would be tedious to generate and distribute so instead you have switches
//! that can toggle between different versions. This might sound counter-intuitive or meaningless at
//! first, but it's a significant distinction.
//!
//! ## Difference in Concepts
//!
//! Consider these types of "configuration":
//!
//! 1. Whether to enable HTTPS.
//! 1. Whether to use a newly optimized, experimental implementation of a procedure or the older version.
//!
//! The first is an example of true configuration, while the latter is an example of feature
//! control. With HTTP vs HTTPS, that's a control over what the same client does. The same client is
//! capable of operating in both modes. With the optimization, this isn't one implementation with
//! different options, it is two distinct implementations. When we are satisfied with the new one,
//! the old one will be removed. We could just delete the old code, but at scale this isn't an
//! option.
//!
//! It can take weeks or months for complete production testing and a full deployment. It's not
//! practical to spin off different binaries and try to incrementally ship these out as a matrix
//! along with any other experimental work.
//!
//! ### Gray Areas
//!
//! Not all situations are cut and dry. What if we have a new feature for our application and a
//! switch to enable or disable it? For example, what about a web app with two endpoints:
//!
//! - `GET /foo`
//! - `GET /bar`
//!
//! Where `GET /bar` is a new feature we'd like to incrementally expose as our confidence in it
//! grows. For an HTTP endpoint, this is probably feature control. It's hard to imagine why you
//! would have it conditionally disabled in the future beyond as a "big red button" for devops.
//! Considering the final state of the feature rather than the temporary aspect of an experiment can
//! be instructive.
//!
//! It's also reasonable for a switch to start off a feature control and then graduate to
//! configuration. Adding new config switches is a breaking change. You may want to start off with
//! the feature approach, gain confidence, and then release a new permanent config switch and deal
//! with the associated impact to other code, config files, documentation, etc at that time.
//!
//! Another scenario (from day one or to graduate into) is having both. A config value is present
//! but a feature control approach is used to override configuration in a way that's simpler to
//! manage / doesn't require destructive edits. This is a pretty rare scenario though.
//!
//! Finally, one of the best ways to get a feeling for which option should be used is to consider
//! how the switch looks in code.
//!
//! ## Difference in Code
//!
//! Configuration is:
//!
//! - Part of the public interface
//! - Loosely coupled (a struct passed in to consuming code)
//! - Can be any value type
//! - Callers are aware of the concept and options available
//!
//! Feature control is:
//!
//! - Hidden, accessed statically, code is aware of the sourcing/
//! - Tightly coupled (statics rely on shared state)
//! - A finite enumeration of values (currently we only support booleans)
//! - Callers aren't aware this decision fork exists
//!
//! But both have in common:
//!
//! - May be layered
//! - May be conditional on environment
//!
//! At the end of the day ask yourself "should this be static?" and let that guide the decision.

use std::{
    any::Any,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

/// Define the features of your application as a quasi-enum of feature name + default value pairs.
/// This will generate a corresponding enum and other associated types that enable you to use
/// statically typed features and check their current state from static assertions.
///
/// # Basic Example
///
/// Define an enum like you normally would, but provide a default value for each feature:
///
/// ```rust
/// conspiracy_macros::define_features!(
///     pub enum Features {
///         OptimizedHashComputation => true,
///         UseQuick => false,
///     }
/// );
/// ```
///
/// # Best Practices
///
/// Other than the enum itself, don't attempt to work with the generated types directly. The other
/// `feature_` and `try_feature` prefixed macros simplify interacting with the generated code and
/// provide safety guarantees.
///
/// Generated code runs the risk of conflicts. Consider wrapping the generated code in a module:
///
/// ```rust
/// mod generated {
///     conspiracy_macros::define_features!(
///         pub enum Features {
///             Foo => false,
///             Bar => false,
///         }
///     );
/// }
///
/// use generated::Features;
/// ```
///
/// # See Also
///
/// - [`feature_enabled!`]
/// - [`feature_enabled_or!`]
/// - [`feature_enabled_or_default!`]
/// - [`try_feature_enabled!`]
pub use conspiracy_macros::define_features;
/// Assert the state of a feature that has been defined by [`define_features!`] from the registered
/// global tracker. If no global tracker was registered, a panic is raised.
///
/// ```rust
/// # use conspiracy::feature_control::{ define_features, set_global_tracker, tracker::ConspiracyFeatureTracker};
///
/// define_features!(pub enum Features { Foo => false });
///
/// ConspiracyFeatureTracker::<Features>::from_default()
///     .set_as_global_tracker()
///     .unwrap();
///
/// // Yields `false`
/// conspiracy::feature_control::feature_enabled!(Features::Foo);
/// ```
///
/// # Behavior Under `#[cfg(test)]`
///
/// However, if the code is compiled with `#[cfg(test)]`, the default value will be returned instead
/// of panicking. Static dependencies on shared state do not pair well with unit testing, so this
/// will prevent usage from poisoning testability without requiring you to abandon static typing or
/// accept unexpected behaviors.
///
/// # Why Panic?
///
/// In large scale systems with safe, incremental deployments, you're better off crashing (which
/// will prevent errors from being checked in or deployed) than having a backup behavior suddenly
/// applied. If you're not in such a situation, you can use [`feature_enabled_or_default!`] which
/// self documents that this behavior can occur in your code.
pub use conspiracy_macros::feature_enabled;
/// Assert the state of a feature that has been defined by [`define_features!`] from the registered
/// global tracker. If no global tracker was registered, provided value is returned.
///
/// ```rust
/// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
/// use conspiracy_macros::feature_enabled_or;
///
/// conspiracy::feature_control::define_features!(pub enum Features { Foo => false });
///
/// // No global tracker set up, so yields `true`
/// feature_enabled_or!(Features::Foo, true);
/// ```
pub use conspiracy_macros::feature_enabled_or;
/// Assert the state of a feature that has been defined by [`define_features!`] from the registered
/// global tracker. If no global tracker was registered, the default value provided to
/// the macro is returned instead.
///
/// ```rust
/// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
/// use conspiracy_macros::feature_enabled_or_default;
///
/// conspiracy::feature_control::define_features!(pub enum Features { Foo => false });
///
/// // No global tracker set up, so yields the default of `false`
/// feature_enabled_or_default!(Features::Foo);
/// ```
pub use conspiracy_macros::feature_enabled_or_default;
/// Assert the state of a feature that has been defined by [`define_features!`] from the registered
/// global tracker. If no global tracker was registered, return an error.
///
/// ```rust
/// # use conspiracy::feature_control::{set_global_tracker, tracker::ConspiracyFeatureTracker};
/// use conspiracy_macros::try_feature_enabled;
///
/// conspiracy::feature_control::define_features!(pub enum Features { Foo => false });
///
/// ConspiracyFeatureTracker::<Features>::from_default()
///     .set_as_global_tracker()
///     .unwrap();
///
/// // Yields `Ok(false)`
/// try_feature_enabled!(Features::Foo);
/// ```
pub use conspiracy_macros::try_feature_enabled;
pub use conspiracy_theories::feature::{AsFeature, FeatureSet, FeatureTracker};

pub mod tracker;

// Credit: This uses the same static initialization patterns as the tokio tracing crate.

static GLOBAL_TRACKER_INIT: AtomicUsize = AtomicUsize::new(UNINITIALIZED);
static mut GLOBAL_TRACKER: &'static dyn FeatureTracker = &NO_TRACKER;
static NO_TRACKER: tracker::NoTracker = tracker::NoTracker;

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

/// Registers a [`FeatureTracker`] as the global tracker used to statically assert feature state.
/// This can only be called once, subsequent calls will be rejected.
pub fn set_global_tracker<T: 'static, C: FeatureTracker + 'static>(
    tracker: C,
) -> Result<(), SetGlobalTrackerError> {
    let tracker = Box::new(tracker);

    unsafe {
        // SAFETY: No data-race, this is indirectly locked via the atomic GLOBAL_TRACKER_INIT
        // SAFETY: No memory issue, this is leaked onto heap satisfying 'static. Calling this
        // function multiple times isn't allowed, so this will never be "truly" leaked.
        set_global_tracker_from_ref(Box::into_raw(tracker))?;

        // Try validating the type. We expect a single type behind the opaque value. Checking here means
        // we're far more likely to catch at startup, which in turn makes it viable for the unwrap based
        // feature checks to be used safely.
        #[allow(static_mut_refs)] // Never mutated without guard via GLOBAL_TRACKER_INIT
        if GLOBAL_TRACKER.static_feature_state().is::<T>() {
            Ok(())
        } else {
            Err(SetGlobalTrackerError::BadCast(BadCastError(
                std::any::type_name::<T>().to_string(),
            )))
        }
    }
}

/// Implementation details of [`set_global_tracker`]. The caller **MUST** pass a valid pointer with
/// a `'static` lifetime.
///
/// This is refactored out to allow [`MockFeatureTracker`] to automatically initialize its singleton instance.
unsafe fn set_global_tracker_from_ref(
    tracker: *mut dyn FeatureTracker,
) -> Result<(), SetGlobalTrackerError> {
    // if `compare_exchange` returns Result::Ok(_), then `new` has been set and
    // `current`—now the prior value—has been returned in the `Ok()` branch.
    if GLOBAL_TRACKER_INIT
        .compare_exchange(
            UNINITIALIZED,
            INITIALIZING,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
    {
        let tracker = Box::new(tracker);
        // SAFETY: No data-race, this is indirectly locked via the atomic GLOBAL_TRACKER_INIT.
        // SAFETY: It is the responsibility of the caller to ensure valid memory is passed.
        GLOBAL_TRACKER = &**tracker;

        GLOBAL_TRACKER_INIT.store(INITIALIZED, Ordering::SeqCst);
        Ok(())
    } else {
        Err(SetGlobalTrackerError::GlobalTrackerAlreadySet)
    }
}

/// These functions are not intended to be used directly. Instead, use the macros in [`feature_control`][crate::feature_control].
pub mod macro_targets {
    use std::{any::Any, sync::Arc};

    use crate::feature_control::{feature_state_inner, global_tracker_set, FeatureEnabledError};

    /// Uses the global tracker previously set by [`set_global_tracker`][crate::feature_control::set_global_tracker]
    /// to determine if the feature is enabled.
    ///
    /// # Can Panic
    /// If a global tracker hasn't been set, this function will panic.
    ///
    /// ## Why not return [`Result`]?
    /// The purpose of this function is to be able to change an implementation as a hidden side effect
    /// of the feature control state. That is why the function is static, to avoid impacting the signature
    /// of functions that use it. Remember, the idea is that feature control is as if you've compiled
    /// multiple versions of your application that you'd like to be able to switch between
    ///
    /// ## Why not pick a default value?
    /// There is no sensible default value to be picked here. Does the user want a default? If so should
    /// it be default true or default false? It varies per-user, per-feature. Defaults are supported
    /// when creating a tracker, but that information still needs to be communicated to us by setting a
    /// global tracker.
    ///
    /// # Safety
    /// This is never intended to be called directly, it should only be called as an implementation
    /// detail of macro generated code. The underlying static for the feature tracker is a shared
    /// mutable reference as an optimization. Interacting with that state safely requires using a
    /// separate static atomic properly.
    pub unsafe fn feature_state_unchecked<T: Any + Send + Sync>() -> Arc<T> {
        feature_state_inner().expect("Bad cast")
    }

    /// Uses the global tracker previously set by [`set_global_tracker`][crate::feature_control::set_global_tracker]
    /// to determine if the feature is enabled. If no tracker was set, an error is returned.
    pub fn try_feature_state<T: Any + Send + Sync>() -> Result<Arc<T>, FeatureEnabledError> {
        if global_tracker_set() {
            unsafe { feature_state_inner() }
        } else {
            Err(FeatureEnabledError::NoGlobalTracker)
        }
    }
}

unsafe fn feature_state_inner<T: Any + Send + Sync>() -> Result<Arc<T>, FeatureEnabledError> {
    #[allow(static_mut_refs)] // Never mutated without guard via GLOBAL_TRACKER_INIT
    let state = GLOBAL_TRACKER.static_feature_state();
    Ok(state
        .downcast::<T>()
        .map_err(|_| BadCastError(std::any::type_name::<T>().to_string()))?)
}

/// Checks if [`set_global_tracker`] has already been called to determine if singleton should be
/// initialized.
fn global_tracker_set() -> bool {
    GLOBAL_TRACKER_INIT.load(Ordering::Relaxed) == INITIALIZED
}

/// Error returned when the type tracked by the global tracker doesn't match the type used asserting
/// the state of a feature (i.e. when the [`FeatureSet`] types are mismatched).
#[derive(thiserror::Error, Debug)]
#[error("Expected global state type `{0}`. This should be unreachable! The tracker's state type shouldn't vary"
)]
pub struct BadCastError(String);

/// Error returned when setting the global tracker fails.
#[derive(thiserror::Error, Debug)]
pub enum SetGlobalTrackerError {
    #[error(
        "A global tracker has already been set. `set_global_tracker` cannot be called multiple times"
    )]
    GlobalTrackerAlreadySet,
    #[error("{0:?}")]
    BadCast(#[from] BadCastError),
}

/// Error returned when the state of a feature could not be determined.
#[derive(thiserror::Error, Debug)]
pub enum FeatureEnabledError {
    #[error("No global tracker was set. `set_global_tracker` must be called first")]
    NoGlobalTracker,
    #[error("{0:?}")]
    BadCast(#[from] BadCastError),
}
