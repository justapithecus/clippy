//! HotkeyProvider trait — global key binding registration and event delivery.
//!
//! See CONTRACT_RESOLVER.md §HotkeyProvider.

use tokio::sync::mpsc::UnboundedReceiver;

use super::ResolverError;

/// A key binding specification.
///
/// Wraps the user-provided string (e.g. `"Super+Shift+C"`). Parsing
/// and resolution to platform-specific keycodes is the provider's
/// responsibility.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// Key binding specification string (e.g. `"Super+Shift+C"`).
    pub spec: String,
}

/// Platform-agnostic hotkey action.
///
/// Replaces the X11-specific `Action` enum in the hotkey client.
/// Classification logic lives inside the `HotkeyProvider` adapter.
#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    /// Capture the focused session's current turn.
    Capture,
    /// Paste the last captured turn into the focused session.
    Paste,
    /// Capture to system clipboard.
    Clipboard,
}

/// Result of a successful `HotkeyProvider::register()` call.
pub struct HotkeyRegistration {
    /// Receiver for platform-agnostic hotkey events.
    ///
    /// The provider's event loop classifies raw platform events into
    /// `HotkeyEvent` values and sends them on this channel.
    pub events: UnboundedReceiver<HotkeyEvent>,
    /// Number of key bindings that were successfully grabbed.
    ///
    /// Partial success is acceptable — the hotkey client checks this
    /// against zero to decide whether to proceed.
    pub bindings_ok: u32,
}

/// Registers global key bindings and delivers classified hotkey events.
///
/// Platform adapters implement this trait to abstract key grab mechanisms
/// away from the hotkey client. The hotkey client calls `register()` at
/// startup and reads from the returned `HotkeyRegistration::events`
/// channel.
///
/// Implementations MUST expose a mechanism for efficient event-loop
/// integration (e.g. a pollable file descriptor) internally, so the
/// event channel is driven without busy-polling. The specific mechanism
/// is platform-dependent.
pub trait HotkeyProvider {
    /// Register key bindings and start delivering events.
    ///
    /// Parses the binding specs, grabs keys via the platform mechanism,
    /// and spawns an event thread/task that classifies raw events into
    /// `HotkeyEvent` values on the returned channel.
    ///
    /// `clipboard` is optional — if `None`, only capture and paste
    /// bindings are registered.
    fn register(
        &mut self,
        capture: &KeyBinding,
        paste: &KeyBinding,
        clipboard: Option<&KeyBinding>,
    ) -> Result<HotkeyRegistration, ResolverError>;

    /// Release all grabbed key bindings and stop the event thread.
    fn unregister(&mut self);
}
