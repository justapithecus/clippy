# CONTRACT_HOTKEY.md — clippy

This contract defines the **hotkey client** — the component that
registers global hotkeys, detects which session has focus, and
translates key events into broker operations.

Depends on: CONTRACT_BROKER.md

---

## Purpose

The hotkey client is a standalone process that:

1. Grabs global key combinations via X11.
2. On key press, resolves the focused clippy session.
3. Sends capture or paste requests to the broker.
4. Reports success or failure to the user.

The hotkey client is the only component that interacts with the
window system. It connects to the broker as a `"client"` role.

---

## Definitions

**Hotkey**: A global key combination registered with the window system.
When pressed in any window, the event is delivered to the hotkey client
instead of the focused application.

**Focused session**: The clippy PTY wrapper session whose terminal
window currently has X11 input focus.

**Action**: A named operation triggered by a hotkey. v0 defines two
actions: `capture` and `paste`.

---

## Actions

### Capture

Triggered by the capture hotkey.

1. Resolve the focused session (see Focus Detection below).
2. Send a `capture` request to the broker with the focused session ID.
3. Report the result to the user.

If focus resolution fails (no clippy session in the focused window),
the action is a no-op with an error notification.

### Paste

Triggered by the paste hotkey.

1. Resolve the focused session.
2. Send a `paste` request to the broker with the focused session ID.
3. Report the result to the user.

If focus resolution fails, the action is a no-op with an error
notification.

---

## Default Bindings

> **DECISION: default-bindings**
>
> Default key combinations must be chosen to minimize conflicts with
> common desktop environments, terminal emulators, and agents.
>
> Recommended candidates:
>
> | Action  | Candidate              |
> |---------|------------------------|
> | Capture | `Super+Shift+C`        |
> | Paste   | `Super+Shift+V`        |
>
> Final defaults MUST be validated against KDE/Plasma (primary v0
> target), Konsole, and common agent keybindings before shipping.

### Configuration

- Bindings MUST be user-configurable.
- Configuration source and format are implementation details.
- The hotkey client MUST support at minimum two independently
  configurable bindings: one for capture, one for paste.
- Modifier keys allowed: Shift, Control, Alt (Mod1), Super (Mod4).
- Bindings MUST require at least one modifier key. Bare keys
  (e.g., F1 alone) MUST NOT be accepted as global hotkeys.

---

## Focus Detection (v0 — X11)

Focus detection determines which clippy session, if any, occupies
the currently focused terminal window.

### Algorithm

1. Query `_NET_ACTIVE_WINDOW` on the X11 root window to obtain
   the focused window's XID.
2. Read the `_NET_WM_PID` property from that window to obtain
   the window-owning process PID.
3. Request the session list from the broker (`list_sessions`).
4. For each registered session, walk the process tree upward from
   the session's child PID. If any ancestor matches the window
   PID, the session is a candidate.
5. If exactly **one** session matches: that is the focused session.
6. If **zero** sessions match: no clippy session has focus.
   The action is a no-op.
7. If **multiple** sessions match (e.g., multiple wrappers in
   split panes of the same terminal): the action fails with
   an ambiguity error.

### Limitations

- Requires `_NET_WM_PID` to be set by the terminal emulator.
  Most modern X11 terminals set this. If absent, resolution fails.
- Requires `/proc` for process tree traversal (Linux-specific).
- Wayland is not supported in v0. The X11 algorithm does not
  apply under Wayland compositors.

### Future

In v2, focus detection is extracted into the resolver interface
(CONTRACT_RESOLVER.md). The hotkey client delegates to the resolver
and becomes platform-agnostic.

---

## Hotkey Registration (X11)

### Grab

- The client registers hotkeys using `XGrabKey` on the root window.
- Grabs are established at startup after connecting to the broker.
- The NumLock and CapsLock modifier bits MUST be masked so that
  hotkeys fire regardless of lock state.

### Conflict handling

If `XGrabKey` fails (another application holds the grab):

- The client MUST log a diagnostic identifying the conflicting
  binding.
- The client MUST continue running with whatever bindings did
  succeed.
- If **no** bindings succeed, the client MUST exit with a non-zero
  exit code and a clear error message.

### Ungrab

On shutdown, the client MUST call `XUngrabKey` for all registered
bindings before disconnecting from X11.

---

## Feedback

When an action completes, the user MUST receive feedback indicating
success or failure.

> **DECISION: feedback-mechanism**
>
> The feedback channel for v0 has not been finalized. Options:
>
> | Mechanism             | Pros                         | Cons                        |
> |-----------------------|------------------------------|-----------------------------|
> | Desktop notification  | Visible, standard            | Async, can be noisy         |
> | Urgency hint on window | Subtle, in-band             | Easy to miss                |
> | Audible bell          | Immediate                    | Disruptive, not universal   |
> | stderr log only       | Simple, zero UI              | Invisible during normal use |
>
> At minimum, errors MUST be surfaced in a way the user can observe
> without checking logs. Success feedback MAY be silent if errors are
> clearly distinguishable.

---

## Client Lifecycle

### Startup

1. Connect to the broker (Unix domain socket).
2. Complete the `hello` handshake with `role: "client"`.
3. Open a connection to the X11 display.
4. Register global hotkeys via `XGrabKey`.
5. Enter the event loop.

If the broker is unreachable at startup, the client MUST exit with
a non-zero exit code and a diagnostic. The hotkey client does not
operate independently of the broker.

### Running

- Listen for X11 key events.
- On key event: execute the bound action (capture or paste).
- Actions are serialized — the client MUST NOT dispatch a new
  action while a previous one is awaiting a broker response.

### Shutdown

On SIGTERM or SIGINT:

1. Ungrab all hotkeys.
2. Close the X11 connection.
3. Disconnect from the broker.
4. Exit with code 0.

### Broker disconnect

If the broker connection drops while the client is running:

- The client MUST ungrab hotkeys and exit with a non-zero
  exit code.
- The client MUST NOT silently continue with non-functional
  hotkeys.

---

## Non-Guarantees

- The hotkey client does not manage sessions or turns.
- The hotkey client does not read or modify turn content.
- The hotkey client does not persist any state.
- Focus detection is best-effort — edge cases (nested terminals,
  split panes, multiple monitors) may produce incorrect or
  ambiguous results.

---

## Version History

| Version | Changes                                                                 |
|---------|-------------------------------------------------------------------------|
| v0      | Initial definition. X11-only. Two actions (capture, paste).             |
| v2      | Focus detection delegated to resolver. Client becomes platform-agnostic. |
