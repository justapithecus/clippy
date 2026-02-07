//! SessionResolver trait — maps the windowing environment to clippy sessions.
//!
//! See CONTRACT_RESOLVER.md §SessionResolver.

use crate::ipc::protocol::SessionDescriptor;

use super::ResolverError;

/// Resolves which clippy session, if any, has focus in the windowing
/// environment.
///
/// Platform adapters implement this trait to abstract focus detection
/// away from the hotkey client. The hotkey client calls
/// `focused_session()` instead of performing its own focus detection.
pub trait SessionResolver {
    /// Return the session ID of the focused session, or `None` if no
    /// registered session owns the focused window.
    ///
    /// `sessions` is the current list of broker-registered sessions,
    /// typically obtained via `broker.list_sessions()`.
    fn focused_session(
        &self,
        sessions: &[SessionDescriptor],
    ) -> Result<Option<String>, ResolverError>;
}
