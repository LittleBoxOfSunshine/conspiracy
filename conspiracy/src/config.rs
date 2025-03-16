//! # Config
//!
//! A combination of patterns and general purpose implementations for safe, ergonomic configuration.
//!
//! ## Defining a Configuration
//!
//! See [`config_struct`].
//!
//! ## Config Fetcher Pattern
//!
//! The config fetcher pattern is a strategy for ensuring atomic, lock-free config updates. If a
//! piece of code needs to apply config changes over time, it depends on [`ConfigFetcher`] rather
//! than on the configuration struct itself. Usually this is done as an [`Arc`] (in which case you
//! can use the provided alias [`SharedConfigFetcher`] which includes the necessary trait bounds).
//!
//! Config fetchers return snapshot, an [`Arc`] to the config struct it tracks. This means the
//! implementor can leverage [copy-on-write](https://en.wikipedia.org/wiki/Copy-on-write) with the
//! critical section being a single pointer update. On most modern hardware, pointer swaps can be
//! done using lock-free atomics.
//!
//! For callers, this allows them to respond to updates without the need for signaling or holding
//! locks. When it is an appropriate time for an update to be applied, the caller requests a new
//! snapshot. This is a very cheap, lock-free operation. The caller will then hold on to that
//! snapshot until they hit the end of their quasi-transactional scope. This ensures that updates
//! are atomic, you don't want config `N` to be applied during the first half of an operation and
//! config `N+1` to be applied during the second half (this would be non-deterministic behavior).
//!
//! Example "transactional" scopes:
//!
//! - A polling thread would request a new snapshot each time it wakes up
//! - An HTTP server would request a snapshot during setup and share the fetcher with any request handlers. Request handlers would request a snapshot at the beginning of processing each request.
//!
//! One potential downside to this approach is that you can't do in place updates of data, however
//! the tiny, tiny, benefit that produces is far offset by penalties required for synchronizing data
//! across threads required to do so. Related, this means you can have multiple copies configuration
//! in memory at the same time. However, the impact here is negligible. Configs aren't rapidly
//! cycled, so generally speaking the limit will be at most 2 copies. If you're using files, you
//! likely have already staged a second copy during the deserialize stage anyway. Finally, configs
//! aren't that large.
//!
//! This approach works well in the vast majority of circumstances.
//!
//! ## Consuming Configurations
//!
//! One of the key advantages of conspiracy is the ability to depend on the narrow subset of an app
//! level configuration that is relevant to a piece of code, instead of tightly coupling it to the
//! rest of the application's concepts or relying on dynamic typing.
//!
//! ```rust
//! # use conspiracy::config::{as_shared_fetcher, shared_fetcher_from_static, SharedConfigFetcher};
//! # use conspiracy_macros::{arcify, config_struct};
//! // Consider app level config:
//! config_struct!(
//!     struct AppConfig {
//!         sub_config: struct SubConfig {
//!             polling_rate_seconds: u32,
//!         },
//!         telemetry: bool,
//!     }
//! );
//!
//! // With values, as a shared fetcher:
//! let app_config_fetcher = shared_fetcher_from_static(
//!     arcify!(AppConfig {
//!         sub_config: SubConfig {
//!             polling_rate_seconds: 30,
//!         },
//!         telemetry: false,
//!     })
//! );
//!
//! // Portions of code that depend only on `SubConfig` can do so
//! fn sub_app(config_fetcher: SharedConfigFetcher<SubConfig>) {
//!     let config = config_fetcher.latest_snapshot();
//! }
//!
//! // In order to convert the global config to the sub config, simply:
//! sub_app(as_shared_fetcher(&app_config_fetcher));
//! ```
//!
//! This is useful for composing the application at boot time, e.g. when you have an [axum Router](https://docs.rs/axum/latest/axum/struct.Router.html)
//! composed of nested routers, the shared sub-config fetcher can be a part of the input to
//! building the corresponding nested router.
//!
//! > Conspiracy does not presently offer a facility for defining sub configs separately and merging
//! > them up the composition levels. Your best option is to generate you configurations in a common
//! > base crate in multi-crate projects. Applications commonly already have such a crate, and if
//! > not the present mechanism still prevents this dependency form leaking into the code that is
//! > consuming configuration.

use std::{marker::PhantomData, sync::Arc};

/// Define an instance of a nested struct as if the types are not wrapped in [`Arc`].
/// Intended to reduce boilerplate when providing values to [`shared_fetcher_from_static`].
///
/// ```rust
/// # use std::sync::Arc;
/// # use conspiracy_macros::arcify;
/// let _foo = arcify!(Foo {
///     val: 0,
///     bar: Bar {
///         val: 1,
///     }
/// });
///
/// struct Foo {
///     val: u32,
///     bar: Arc<Bar>,
/// }
///
/// struct Bar {
///     val: u32,
/// }
/// ```
pub use conspiracy_macros::arcify;
/// Define a configuration as a set of nested structs. This reduces boilerplate and makes it easier
/// to maintain the struct definition of a config that you track against a file. Additionally, the
/// structs will have the necessary code generated to support integration with `conspiracy` types.
///
/// ```rust
/// # use conspiracy::config::config_struct;
/// # use std::net::SocketAddr;
/// config_struct!(
///     pub struct MyAppConfig {
///         database: struct DatabaseConfig {
///             connection_string: String,
///             name: String,
///         },
///         web_server: struct WebServerConfig {
///             addr: SocketAddr,
///         },
///     }
/// );
/// ```
///
/// # Requirements
///
/// Any type you use, that isn't itself being generated by the macro, must implement:
///
/// - [`Clone`]
/// - [`serde::Deserialize`]
/// - [`serde::Serialize`]
///
/// # Also Generates
///
/// - Trait implementations generated necessary to make it possible for a [`ConfigFetcher`] of type `A` to be used as a [`ConfigFetcher`] of sub-config type `B`.
/// - [`Clone`], [`serde::Deserialize`], and [`serde::Serialize`] implementations.
pub use conspiracy_macros::config_struct;

/// TODO: Doc comments
pub use conspiracy_macros::RestartRequired;
pub use conspiracy_theories::config::{AsField, ConfigFetcher, RestartRequired};

/// A shared instance of a `ConfigFetcher` that can be converted in sub-config fetchers and shared
/// across threads.
pub type SharedConfigFetcher<T> = Arc<dyn ConfigFetcher<T> + Send + Sync>;

/// Creates a [`SharedConfigFetcher`] for the sub-config of the given fetcher.
///
/// More formally, this generates a [`SharedConfigFetcher<T2>`] from a [`SharedConfigFetcher<T>`]
/// where `T2` is a sub-config meaning struct `T` has a field of type `T2` and `T` implements [`AsField<T2>`]
pub fn as_shared_fetcher<T, T2, F>(fetcher: &Arc<F>) -> SharedConfigFetcher<T2>
where
    F: ConfigFetcher<T> + ?Sized + Send + Sync + 'static,
    T: AsField<T2>,
    T2: Send + Sync + 'static,
{
    let clone = fetcher.clone();
    shared_fetcher_from_fn(move || {
        let snapshot: Arc<T> = clone.latest_snapshot();
        let inner: Arc<T2> = snapshot.share();
        inner
    })
}

/// Constructs a [`SharedConfigFetcher`] from a closure that returns a new snapshot.
pub fn shared_fetcher_from_fn<
    T: Send + Sync + 'static,
    F: Fn() -> Arc<T> + Send + Sync + 'static,
>(
    fetcher: F,
) -> SharedConfigFetcher<T> {
    Arc::new(BoxedFetcher {
        inner: fetcher,
        phantom: PhantomData {},
    })
}

/// Constructs a [`SharedConfigFetcher`] from a static value.
///
/// This is a convenience function for the common pattern:
///
/// ```rust
/// # use std::sync::Arc;
/// # conspiracy::config::config_struct!(struct Config { foo: u32 });
/// # let config = Config { foo: 0 };
/// // Where config is some arbitrary config struct defined by `config_struct!`
/// // The instance itself is usually created by `arcify!`
/// let inner = Arc::new(config);
/// conspiracy::config::shared_fetcher_from_fn(move || inner.clone());
/// ```
pub fn shared_fetcher_from_static<T: Send + Sync + 'static>(config: T) -> SharedConfigFetcher<T> {
    let inner = Arc::new(config);
    shared_fetcher_from_fn(move || inner.clone())
}

/// Converts an owned [`ConfigFetcher`] into a [`SharedConfigFetcher`]
pub fn into_shared_fetcher<T: Send + Sync + 'static>(
    fetcher: impl ConfigFetcher<T> + Send + Sync + 'static,
) -> SharedConfigFetcher<T> {
    let fetcher = Arc::new(fetcher);
    Arc::new(BoxedFetcher {
        inner: move || fetcher.latest_snapshot(),
        phantom: PhantomData {},
    })
}

#[derive(Clone)]
struct BoxedFetcher<T, F: Fn() -> Arc<T>> {
    inner: F,
    phantom: PhantomData<T>,
}

impl<T, F: Fn() -> Arc<T>> ConfigFetcher<T> for BoxedFetcher<T, F> {
    fn latest_snapshot(&self) -> Arc<T> {
        (self.inner)()
    }
}
