//! X11 integration — NumLock detection, event thread, and shared constants.
//!
//! Provides low-level X11 utilities used by the resolver X11 adapters:
//! - `LOCK_MASK`: CapsLock modifier bit
//! - `detect_numlock_mask()`: dynamic NumLock modifier detection
//! - `spawn_event_thread()`: polling event thread for key events
//!
//! The former `X11Context` (connection, grabs, focus queries) has been
//! replaced by the resolver adapters in `resolver::x11::*`.
//! See CONTRACT_HOTKEY.md §132–156, CONTRACT_RESOLVER.md.

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{self, Keysym};
use x11rb::rust_connection::RustConnection;

/// CapsLock modifier bit (always LockMask, bit 1).
pub(crate) const LOCK_MASK: u16 = 0x0002;

/// XK_Num_Lock keysym for dynamic modifier detection.
const XK_NUM_LOCK: Keysym = 0xff7f;

/// Detect which modifier bit corresponds to NumLock by querying the
/// X11 modifier mapping and keyboard mapping.
///
/// Falls back to Mod2 (0x0010) if detection fails — this is the most
/// common mapping and matches xmodmap defaults.
pub(crate) fn detect_numlock_mask(conn: &RustConnection) -> u16 {
    const FALLBACK: u16 = 0x0010; // Mod2Mask

    let mod_reply = match xproto::get_modifier_mapping(conn) {
        Ok(cookie) => match cookie.reply() {
            Ok(r) => r,
            Err(_) => return FALLBACK,
        },
        Err(_) => return FALLBACK,
    };

    let keycodes_per_mod = mod_reply.keycodes_per_modifier() as usize;
    if keycodes_per_mod == 0 {
        return FALLBACK;
    }

    // Resolve XK_Num_Lock → set of keycodes via keyboard mapping.
    let setup = conn.setup();
    let min_kc = setup.min_keycode;
    let max_kc = setup.max_keycode;
    let count = max_kc - min_kc + 1;

    let kb_reply = match xproto::get_keyboard_mapping(conn, min_kc, count) {
        Ok(cookie) => match cookie.reply() {
            Ok(r) => r,
            Err(_) => return FALLBACK,
        },
        Err(_) => return FALLBACK,
    };

    let syms_per_code = kb_reply.keysyms_per_keycode as usize;
    if syms_per_code == 0 {
        return FALLBACK;
    }

    // Collect keycodes that produce XK_Num_Lock.
    let mut numlock_keycodes: Vec<u8> = Vec::new();
    for i in 0..count as usize {
        let base = i * syms_per_code;
        for j in 0..syms_per_code {
            if kb_reply.keysyms.get(base + j) == Some(&XK_NUM_LOCK) {
                numlock_keycodes.push(min_kc + i as u8);
                break;
            }
        }
    }

    // Scan modifier map: 8 rows × keycodes_per_modifier.
    // Row 0 = Shift, 1 = Lock, 2 = Control, 3 = Mod1, ..., 7 = Mod5.
    // Modifier mask bit for row i = 1 << i.
    for modifier_idx in 0..8usize {
        let row_start = modifier_idx * keycodes_per_mod;
        for k in 0..keycodes_per_mod {
            if let Some(&keycode) = mod_reply.keycodes.get(row_start + k)
                && keycode != 0
                && numlock_keycodes.contains(&keycode)
            {
                return 1u16 << modifier_idx;
            }
        }
    }

    // NumLock not found in modifier map.
    FALLBACK
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
