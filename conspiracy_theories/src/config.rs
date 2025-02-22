use std::sync::Arc;

/// Fetches the current state of configuration as a shared atomic snapshot. Implementors of this
/// trait use atomic copy on write semantics to optimize reads as far as possible. On typical
/// processor architectures, this will be a lock-free read. Snapshots are cheap because they are
/// shared and can be cloned without locking.
///
/// Callers should apply config updates by discarding their snapshot and requesting a new one when
/// they've reached a transactional boundary. For example:
///
/// - In a polling thread, get a new snapshot at the beginning of each iteration, keeping it for the full iteration.
/// - In an HTTP web server, get a new snapshot at the beginning of processing a request and keeping it until a response is returned.
///
/// In this way, callers get high performance and more importantly *consistent* (atomic) application
/// of config updates.
pub trait ConfigFetcher<T> {
    /// Get a shared copy of the currently active configuration state.
    fn latest_snapshot(&self) -> Arc<T>;
}

/// Express a config snapshot as sub-config snapshot. The purpose of this is that code can depend on
/// the subset of an app level config that's actually relevant to them. This leads to better
/// separation of concerns, lower coupling, and less boilerplate in testing without having to give
/// up the safety and consumption ease of use of static typing.
pub trait AsField<T> {
    /// Share a copy of a sub-config.
    fn share(&self) -> Arc<T>;
}
