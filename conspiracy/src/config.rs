//! # Config
//!
//! A combination of patterns and general purpose implementations for safe, ergonomic configuration.
//!
//! ## Defining a Configuration
//!
//! See [`config_struct`] for full details.
//!
//! ```rust
//! # use conspiracy::config::{config_struct, full_serde, full_serde_as};
//! # use std::net::SocketAddr;
//! # use std::time::Duration;
//! # use serde_with::serde_as;
//! use serde_with::DurationSeconds;
//!
//! config_struct!(
//!     #[full_serde]
//!     pub struct MyAppConfig {
//!         pub database: #[full_serde] pub struct DatabaseConfig {
//!             pub connection_string: String,
//!             pub name: String,
//!         },
//!         pub web_server: #[full_serde_as] pub struct WebServerConfig {
//!             #[conspiracy(restart)]
//!             pub addr: SocketAddr,
//!             #[serde_as(as = "DurationSeconds")]
//!             #[serde(rename = "request_timeout_seconds")]
//!             pub request_timeout: Duration,
//!         },
//!         #[conspiracy(restart)]
//!         pub telemetry: #[full_serde] pub struct TelemetryConfig {
//!             pub disk_log_enabled: bool,
//!             pub remote_log_enabled: bool,
//!         }
//!     }
//! );
//! ```
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
//! # use conspiracy_macros::{config_struct, full_serde};
//! # use std::sync::Arc;
//! // Consider app level config:
//! config_struct!(
//!     #[full_serde]
//!     #[serde(rename_all = "camelCase")]
//!     pub struct AppConfig {
//!         pub sub_config:
//!             #[full_serde]
//!             #[serde(rename_all = "camelCase")]
//!             pub struct SubConfig {
//!                 pub max_connections: u32,
//!             },
//!         pub telemetry: bool,
//!     }
//! );
//!
//! let app_config_fetcher = shared_fetcher_from_static(
//!     Arc::new(
//!         serde_json::from_str::<AppConfig>(
//!             r#"{ "subConfig": { "maxConnections": 50 }, "telemetry": false }"#
//!         ).unwrap()
//!     )
//! );
//!
//! // Portions of code that depend only on `SubConfig` can do so
//! fn sub_app(config_fetcher: SharedConfigFetcher<SubConfig>) {
//!     let config = config_fetcher.latest_snapshot();
//! }
//!
//! // In order to convert a config fetcher to a sub config fetcher, simply:
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

/// Define a configuration as a set of nested structs. This reduces boilerplate and makes it easier
/// to maintain the struct definition of a config that you track against a file. Additionally, the
/// structs will have the necessary code generated to support integration with [`conspiracy::config`][crate::config]
/// types.
///
/// ```rust
/// # use conspiracy::config::{config_struct, full_serde_as, full_serde};
/// # use std::net::SocketAddr;
/// # use std::time::Duration;
/// use serde_with::{serde_as, DurationSeconds};
/// use serde::{Serialize, Deserialize};
/// config_struct!(
///     #[full_serde]
///     pub struct MyAppConfig {
///         pub database: #[full_serde] pub struct DatabaseConfig {
///             pub connection_string: String,
///             pub name: String,
///         },
///         pub web_server:
///             #[full_serde_as]
///             pub struct WebServerConfig {
///                 #[conspiracy(restart)]
///                 pub addr: SocketAddr,
///                 #[serde_as(as = "DurationSeconds")]
///                 #[serde(rename = "request_timeout_seconds")]
///                 pub request_timeout: Duration,
///         },
///         #[conspiracy(restart)]
///         pub telemetry: #[full_serde] pub struct TelemetryConfig {
///             pub disk_log_enabled: bool,
///             pub remote_log_enabled: bool,
///         }
///     }
/// );
/// ```
///
/// # Requirements
///
/// Every type in a config struct hierarchy must be unique. This is so that conversions from a
/// config to a sub-config aren't ambiguous.
///
/// Additionally, any type you use, that isn't itself being generated by the macro, must implement:
///
/// - [`Clone`]
///
/// # Attributes
///
/// The macro is compatible with any named struct definition syntax with named fields, including
/// attribute passthrough. Other crates like [`serde`](https://docs.rs/serde/latest/serde) and
/// [`serde_with`](https://docs.rs/serde_with/latest/serde_with/) already offer advanced options for
/// type conversions, defaults, remapping, etc. We recommend using those two crates in combination
/// with [`conspiracy`][crate].
///
/// Additionally, we provide field attributes:
///
/// | Attribute | Behavior |
/// |--|--|
/// | `#[conspiracy(restart)]` | Includes in the generated [`RestartRequired`]. When comparing two config snapshots, if this field changed the struct signals a need to restart. If your [`ConfigFetcher`] supports this, it will automatically gracefully restart your application. |
///
/// # Injection (Usage)
///
/// Configuration should always be a part of your signature, it shouldn't be accessed statically.
/// See [feature_control][crate::feature_control] for more information and to learn about the
/// facilities we offer for those scenarios.
///
/// As a result, to consume configuration you should accept either:
///
/// - A config snapshot
/// - A config fetcher
///
/// When storing state, typically a config fetcher is used. Functions frequently use either one.
///
/// ## Accepting Config Snapshots
///
/// Accept a config snapshot when updates aren't relevant.
///
/// ```rust
/// # struct TelemetryConfig { disk_logging: bool }
/// fn initialize_telemetry(config: &TelemetryConfig) {
///     if config.disk_logging {
///         // Do things
///     }
/// }
/// ```
///
/// Common types to use include (where `Config` is your config struct type):
///
/// - `&Config`
/// - `&Arc<Config>`
/// - `Arc<Config>`
///
/// ## Accepting Config Fetchers
///
/// Accept a [`ConfigFetcher`] when you need to respect updates and control when they're applied.
///
/// ```rust
/// # use std::net::SocketAddr;
/// use conspiracy::config::SharedConfigFetcher;
/// # struct DatabaseConfig { addr: SocketAddr }
/// fn handle_request(config: &SharedConfigFetcher<DatabaseConfig>) {
///     let db_addr = &config.latest_snapshot().addr;
///     // Connect to database
/// }
/// ```
///
/// Common types to use include (where `Config` is your config struct type):
///
/// - `&SharedConfigFetcher<Config>`
/// - `SharedConfigFetcher<Config>`
///
/// # Working with sub-configs
///
/// A config struct snapshot can be converted into a snapshot of any held sub-config. This is also
/// true for any [`ConfigFetcher`].
///
/// ## Convert to sub-config
///
/// ```rust
/// # use conspiracy_macros::config_struct;
/// # use std::sync::Arc;
/// use conspiracy_theories::config::AsField;
///
/// config_struct!(
///     pub struct Config {
///         sub_config: pub struct SubConfig {
///             foo: u32,
///         }
///     }
/// );
/// # let config = Arc::new(Config { sub_config: Arc::new(SubConfig { foo: 0 }) });
/// // Assume config: Arc<Config> exists
/// let sub_config: Arc<SubConfig> = config.share();
/// ```
///
/// ## Convert to sub-config fetcher
///
/// ```rust
/// # use conspiracy_macros::config_struct;
/// # use std::sync::Arc;
/// # use conspiracy::config::{as_shared_fetcher, shared_fetcher_from_static, SharedConfigFetcher};
/// # config_struct!(
/// #    pub struct Config {
/// #        sub_config: pub struct SubConfig {
/// #            foo: u32,
/// #        }
/// #    }
/// # );
/// # let config_fetcher = shared_fetcher_from_static(Arc::new(Config { sub_config: Arc::new(SubConfig { foo: 0 }) }));
/// // Assume config_fetcher: SharedConfigFetcher<Config> exists
/// let sub_config: SharedConfigFetcher<SubConfig> = as_shared_fetcher(&config_fetcher);
/// ```
///
/// # Mock Configs / Testing
///
/// Internally, generated config structs store nested config structs behind [`Arc`]. This is to
/// enable superior developer ergonomics. [`SharedConfigFetcher`] is defined as an opaque, `dyn`
/// type for type erasure. We also can spawn off sub-config fetchers. A sub-config fetcher needs
/// to return a snapshot (another [`Arc`]) which means a sub-config portion of a snapshot can
/// outlive the parent.
///
/// The downside to this approach is that the data can't easily be mutated, which is cumbersome in
/// test code. To alleviate this, a compact type `CompactFoo` is generated for every config struct
/// `Foo`. This type is identical except there is no internal [`Arc`] usage. You can convert between
/// compacted and "arcified" representations with:
///
/// - `.compact()` for `Foo -> CompactFoo`
/// - `.arcify()` for `CompactFoo -> Foo`
///
/// ## With Production Baseline
///
/// Often times tests can take arbitrary values and/or only need a subset of them to be specified or
/// otherwise custom vs what the current application config is. For example, you may have a feature
/// that's disabled in production, but under test it should use all the other settings. Whatever the
/// reason, using the production config as your baseline is usually a sensible choice:
///
/// ```rust
/// # use serde_with::serde_as;
/// use conspiracy::config::{config_struct, full_serde, full_serde_as};
/// use serde_with::DurationMilliSeconds;
///
/// #[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// enum FeatureVersion { V1, V2 }
///
/// config_struct!(
///     #[full_serde]
///     pub struct AppConfig {
///         use_feature_version: FeatureVersion,
///         feature_v2:
///             #[full_serde_as]
///             pub struct FeatureV2Config {
///                 max_threads: u8,
///                 #[serde_as(as = "DurationMilliSeconds")]
///                 #[serde(rename = "timeout_ms")]
///                 timeout: std::time::Duration
///                 // etc.
///         }
///     }
/// );
///
/// const PROD_CONFIG: &str = include_str!("./prod_config_demo.json");
/// fn prod_config_baseline() -> AppConfig {
///     serde_json::from_str(PROD_CONFIG).expect("Bundled config file")
/// }
///
/// let mut test_config = prod_config_baseline().compact();
/// test_config.use_feature_version = FeatureVersion::V2;
/// let test_config = test_config.arcify();
/// ```
///
/// This pattern can be combined with a pre-parsed const or `lazy_static!` to reduce boilerplate.
///
/// ## With Default
///
/// > Consider carefully if you really want defaults. Defaults can unwittingly lead to untested or even
/// > non-deterministic behaviors when corrupt inputs are given and a fallback to default occurs. They
/// > can also increase maintenance burden and increase the divergence between test configuration and
/// > production configuration.
///
/// Using [`default`][Default::default] requires that your config struct implements [`Default`].
/// This is most easily accomplished by using the derive macro in combination with [`config_struct!`]:
///
/// ```rust
/// # use conspiracy::config::config_struct;
/// config_struct!(
///     #[derive(Default)]
///     pub struct AppConfig {
///         module:
///             #[derive(Default)]
///             pub struct ModuleConfig {
///                 do_things: bool
///         }
///     }
/// );
///
/// let mut test_config = AppConfig::default().compact();
/// test_config.module.do_things = true;
/// let test_config = test_config.arcify();
/// ```
///
/// # Automatically Derived Traits
///
/// The generated types will also get automatic implementations for:
///
/// - Traits necessary to be compatible with the [`conspiracy::config`][crate::config] ecosystem:
///     - [`AsField`] conversions into all nested config structs (applies recursively)
///     - [`RestartRequired`]
/// - [`Clone`]
/// - [`serde::Deserialize`](https://docs.rs/serde/latest/serde/trait.Deserialize.html)
/// - [`serde::Serialize`](https://docs.rs/serde/latest/serde/trait.Serialize.html)
pub use conspiracy_macros::config_struct;
/// An alias for deriving serde, meant to replace the common config struct boilerplate:
///
/// ```rust
/// #[derive(serde::Serialize, serde::Deserialize)]
/// pub struct Foo {}
/// ```
pub use conspiracy_macros::full_serde;
/// An alias for deriving serde + serde_as, meant to replace the common config struct boilerplate:
///
/// ```rust
/// #[serde_with::serde_as]
/// #[derive(serde::Serialize, serde::Deserialize)]
/// pub struct Foo {}
/// ```
pub use conspiracy_macros::full_serde_as;
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
    Arc::new(WrappedFetcher {
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
/// // The instance itself is usually created by loading the production config
/// // or the default value if defined with defaults and then modifying as needed
/// // in the case of a test using `compact()` and `arcify()`.
/// let inner = Arc::new(config);
/// conspiracy::config::shared_fetcher_from_fn(move || inner.clone());
/// ```
pub fn shared_fetcher_from_static<T: Send + Sync + 'static>(
    config: Arc<T>,
) -> SharedConfigFetcher<T> {
    shared_fetcher_from_fn(move || config.clone())
}

/// Converts an owned [`ConfigFetcher`] into a [`SharedConfigFetcher`]
pub fn into_shared_fetcher<T: Send + Sync + 'static>(
    fetcher: impl ConfigFetcher<T> + Send + Sync + 'static,
) -> SharedConfigFetcher<T> {
    let fetcher = Arc::new(fetcher);
    Arc::new(WrappedFetcher {
        inner: move || fetcher.latest_snapshot(),
        phantom: PhantomData {},
    })
}

#[derive(Clone)]
pub(crate) struct WrappedFetcher<T, F: Fn() -> Arc<T>> {
    pub(crate) inner: F,
    pub(crate) phantom: PhantomData<T>,
}

impl<T, F: Fn() -> Arc<T>> ConfigFetcher<T> for WrappedFetcher<T, F> {
    fn latest_snapshot(&self) -> Arc<T> {
        (self.inner)()
    }
}
