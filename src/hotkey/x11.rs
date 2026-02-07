//! X11 integration — connection, key grabs, focus queries, event thread.
//!
//! Wraps `x11rb::rust_connection::RustConnection` for hotkey registration,
//! active window detection, and a polling event thread that feeds key
//! events to the main async loop. See CONTRACT_HOTKEY.md §132–156.

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, Atom, GrabMode, ModMask, Window};
use x11rb::rust_connection::RustConnection;

use super::HotkeyError;
use super::keybinding::Binding;

/// Lock modifier bits to mask during XGrabKey registration.
///
/// NumLock = Mod2 (bit 4), CapsLock = Lock (bit 1).
/// Each grab is registered 4 times with all combinations of these bits
/// so hotkeys fire regardless of lock state (CONTRACT_HOTKEY.md §138-139).
const LOCK_MASK: u16 = 0x0002; // LockMask (CapsLock)
const NUM_LOCK_MASK: u16 = 0x0010; // Mod2Mask (NumLock)
const LOCK_MASKS: [u16; 4] = [0, LOCK_MASK, NUM_LOCK_MASK, LOCK_MASK | NUM_LOCK_MASK];

/// Pre-interned X11 atoms for property queries.
struct Atoms {
    net_active_window: Atom,
    net_wm_pid: Atom,
}

/// X11 connection context for the hotkey client.
pub struct X11Context {
    conn: Arc<RustConnection>,
    screen_num: usize,
    root: Window,
    atoms: Atoms,
}

impl X11Context {
    /// Connect to the X11 display and intern required atoms.
    pub fn connect() -> Result<Self, HotkeyError> {
        let (conn, screen_num) = RustConnection::connect(None)
            .map_err(|e| HotkeyError::X11(format!("connect failed: {e}")))?;

        let root = conn.setup().roots[screen_num].root;

        // Intern atoms for focus detection.
        let net_active_window = xproto::intern_atom(&conn, false, b"_NET_ACTIVE_WINDOW")
            .map_err(|e| HotkeyError::X11(format!("intern_atom: {e}")))?
            .reply()
            .map_err(|e| HotkeyError::X11(format!("intern_atom reply: {e}")))?
            .atom;

        let net_wm_pid = xproto::intern_atom(&conn, false, b"_NET_WM_PID")
            .map_err(|e| HotkeyError::X11(format!("intern_atom: {e}")))?
            .reply()
            .map_err(|e| HotkeyError::X11(format!("intern_atom reply: {e}")))?
            .atom;

        Ok(Self {
            conn: Arc::new(conn),
            screen_num,
            root,
            atoms: Atoms {
                net_active_window,
                net_wm_pid,
            },
        })
    }

    /// Register a global key grab on the root window.
    ///
    /// Registers 4 grabs per binding (with/without NumLock/CapsLock).
    /// Returns `Ok(true)` on success, `Ok(false)` if the grab failed
    /// (another application holds it), `Err` on connection error.
    ///
    /// CONTRACT_HOTKEY.md §143-150: log conflict, continue with
    /// whatever bindings succeeded.
    pub fn grab_key(&self, binding: &Binding) -> Result<bool, HotkeyError> {
        let mut all_ok = true;

        for &lock_mask in &LOCK_MASKS {
            let mods = ModMask::from(binding.modifiers | lock_mask);

            let cookie = xproto::grab_key(
                &*self.conn,
                true, // owner_events
                self.root,
                mods,
                binding.keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )
            .map_err(|e| HotkeyError::X11(format!("grab_key send: {e}")))?;

            // Check for error reply (grab conflict).
            if let Err(e) = cookie.check() {
                tracing::warn!(
                    binding = %binding.raw,
                    lock_mask,
                    error = %e,
                    "XGrabKey failed — binding may conflict with another application"
                );
                all_ok = false;
            }
        }

        Ok(all_ok)
    }

    /// Unregister a global key grab from the root window.
    ///
    /// Ungrabs all 4 lock-mask variants. Best-effort — errors are logged.
    pub fn ungrab_key(&self, binding: &Binding) {
        for &lock_mask in &LOCK_MASKS {
            let mods = ModMask::from(binding.modifiers | lock_mask);

            if let Err(e) = xproto::ungrab_key(&*self.conn, binding.keycode, self.root, mods) {
                tracing::debug!(
                    binding = %binding.raw,
                    error = %e,
                    "XUngrabKey failed"
                );
            }
        }

        // Flush ungrab requests.
        if let Err(e) = self.conn.flush() {
            tracing::debug!(error = %e, "flush after ungrab failed");
        }
    }

    /// Query the active (focused) window's PID.
    ///
    /// 1. Read `_NET_ACTIVE_WINDOW` on root → window XID.
    /// 2. Read `_NET_WM_PID` on that window → PID.
    ///
    /// Returns `None` if either property is missing (e.g., focused
    /// window doesn't set `_NET_WM_PID`).
    pub fn get_active_window_pid(&self) -> Result<Option<u32>, HotkeyError> {
        // Step 1: Get the active window XID.
        let reply = xproto::get_property(
            &*self.conn,
            false,
            self.root,
            self.atoms.net_active_window,
            xproto::AtomEnum::WINDOW,
            0,
            1, // We need one 32-bit value.
        )
        .map_err(|e| HotkeyError::X11(format!("get_property _NET_ACTIVE_WINDOW: {e}")))?
        .reply()
        .map_err(|e| HotkeyError::X11(format!("get_property reply: {e}")))?;

        if reply.format != 32 || reply.value.len() < 4 {
            return Ok(None);
        }

        let window_id = u32::from_ne_bytes([
            reply.value[0],
            reply.value[1],
            reply.value[2],
            reply.value[3],
        ]);

        if window_id == 0 {
            return Ok(None);
        }

        // Step 2: Get the PID of the active window.
        let reply = xproto::get_property(
            &*self.conn,
            false,
            window_id,
            self.atoms.net_wm_pid,
            xproto::AtomEnum::CARDINAL,
            0,
            1,
        )
        .map_err(|e| HotkeyError::X11(format!("get_property _NET_WM_PID: {e}")))?
        .reply()
        .map_err(|e| HotkeyError::X11(format!("get_property reply: {e}")))?;

        if reply.format != 32 || reply.value.len() < 4 {
            return Ok(None);
        }

        let pid = u32::from_ne_bytes([
            reply.value[0],
            reply.value[1],
            reply.value[2],
            reply.value[3],
        ]);

        Ok(Some(pid))
    }

    /// Get a shared reference to the X11 connection.
    pub fn conn(&self) -> &Arc<RustConnection> {
        &self.conn
    }

    /// Get the X11 Setup (for keybinding resolution).
    pub fn setup(&self) -> &x11rb::protocol::xproto::Setup {
        self.conn.setup()
    }

    /// Get the screen number.
    pub fn screen_num(&self) -> usize {
        self.screen_num
    }
}

/// Spawn a dedicated thread that polls the X11 connection for events.
///
/// Uses `nix::poll()` on the X11 connection fd with a 100ms timeout.
/// When readable, drains all available events via `poll_for_event()`.
/// Checks the `stop` flag each iteration for clean shutdown.
///
/// Returns the receiver channel and the thread join handle.
pub fn spawn_event_thread(
    conn: Arc<RustConnection>,
    stop: Arc<AtomicBool>,
) -> (tokio::sync::mpsc::UnboundedReceiver<Event>, JoinHandle<()>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = std::thread::Builder::new()
        .name("x11-events".into())
        .spawn(move || {
            let raw_fd = conn.stream().as_raw_fd();

            while !stop.load(Ordering::Relaxed) {
                // SAFETY: raw_fd is the X11 connection fd, valid while conn is alive.
                let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
                let mut fds = [PollFd::new(borrowed, PollFlags::POLLIN)];

                match poll(&mut fds, PollTimeout::from(100u16)) {
                    Ok(0) => continue, // Timeout — check stop flag.
                    Ok(_) => {
                        // Drain all available events.
                        loop {
                            match conn.poll_for_event() {
                                Ok(Some(event)) => {
                                    if tx.send(event).is_err() {
                                        // Receiver dropped — shut down.
                                        return;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    tracing::error!(error = %e, "X11 connection error");
                                    return;
                                }
                            }
                        }
                    }
                    Err(nix::Error::EINTR) => continue,
                    Err(e) => {
                        tracing::error!(error = %e, "poll error on X11 fd");
                        return;
                    }
                }
            }
        })
        .expect("failed to spawn x11 event thread");

    (rx, handle)
}
