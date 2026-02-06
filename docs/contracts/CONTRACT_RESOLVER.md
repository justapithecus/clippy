# CONTRACT_RESOLVER.md — clippy

This contract defines the **resolver abstraction** — the pluggable
interface that extracts all platform-specific behavior into composable
adapters.

Introduced in: v2
Depends on: CONTRACT_BROKER.md, CONTRACT_HOTKEY.md

---

## Purpose

In v0–v1, platform-specific behavior (X11 focus detection, XGrabKey,
clipboard atoms) is hardcoded in the hotkey client and sinks.

v2 extracts these into three independent sub-interfaces. Platform
adapters implement one or more of them. The system composes adapters
at startup — no conditionals in core logic.

---

## Sub-Interfaces

The resolver abstraction consists of three independent capabilities.
Each is a separate interface. Platform adapters implement one or more.

### 1. SessionResolver

Maps the windowing environment to clippy sessions.

| Operation              | Description                                     |
|------------------------|-------------------------------------------------|
| `focused_session()`    | Return the Session ID of the focused session, or none |
| `discover_sessions()`  | Enumerate sessions visible to this resolver     |

**focused_session** replaces the X11-specific algorithm defined in
CONTRACT_HOTKEY.md. The hotkey client delegates to this method
instead of performing its own focus detection.

**discover_sessions** returns the sessions this resolver can see.
For X11, this is all sessions whose terminal windows are on the
display. For tmux, this is all sessions in panes of the current
tmux server.

### 2. HotkeyProvider

Registers global key bindings and delivers key events.

| Operation                    | Description                              |
|------------------------------|------------------------------------------|
| `register(bindings)`         | Grab the specified key combinations      |
| `poll() → Option<Action>`   | Non-blocking check for pending key event |
| `unregister()`               | Release all grabbed key combinations     |

**register** replaces `XGrabKey` usage in the hotkey client. The
hotkey client passes its configured bindings and the provider maps
them to the platform's grab mechanism.

**poll** returns the next pending action (capture, paste, or
extended actions), or none if no key event is queued. The hotkey
client's event loop calls this method.

**unregister** releases all grabs. Called on shutdown.

Errors during registration (conflicts, unsupported combos) are
reported per-binding. Partial success is acceptable — the same
rules from CONTRACT_HOTKEY.md apply.

### 3. ClipboardProvider

Reads and writes the system clipboard.

| Operation              | Description                              |
|------------------------|------------------------------------------|
| `write(content)`       | Set clipboard content to the given bytes |
| `read() → Result`     | Read current clipboard content           |

**write** replaces direct X11 CLIPBOARD atom manipulation in the
clipboard sink (CONTRACT_REGISTRY.md).

**read** is provided for completeness and potential future use
(e.g., pasting from system clipboard into a session).

---

## Composition

At startup, the system selects and composes adapters for each
sub-interface independently.

Example configurations:

| Environment       | SessionResolver | HotkeyProvider  | ClipboardProvider |
|-------------------|-----------------|-----------------|-------------------|
| Linux + X11       | X11Resolver     | X11Hotkey       | X11Clipboard      |
| Linux + X11 + tmux| TmuxResolver    | X11Hotkey       | X11Clipboard      |
| macOS             | MacResolver     | MacHotkey       | MacClipboard      |
| tmux-only (SSH)   | TmuxResolver    | TmuxHotkey      | TmuxClipboard     |

Adapters for the same sub-interface are **mutually exclusive** —
only one SessionResolver is active at a time. There is no fallback
chain or stacking.

### Selection

> **DECISION: adapter-selection**
>
> How adapters are selected at startup:
>
> - **Explicit configuration**: user specifies adapters in config.
>   Simplest, most predictable.
> - **Auto-detection**: probe the environment (is X11 available?
>   is tmux running?) and select adapters automatically.
>   More convenient but introduces inference.
>
> Recommendation: explicit configuration with a `"detect"` shorthand
> that runs auto-detection. Explicit overrides always win.

---

## Reference Adapter: X11

The X11 adapter set is the reference implementation for v2 and
codifies the behavior that was hardcoded in v0–v1.

### X11Resolver (SessionResolver)

**focused_session:**

1. Query `_NET_ACTIVE_WINDOW` on the root window.
2. Read `_NET_WM_PID` from the focused window.
3. Query the broker's session list.
4. Walk the process tree from each session's child PID upward.
5. Return the session whose child PID is a descendant of the
   window PID, or none.
6. If multiple sessions match, return an ambiguity error.

This is the same algorithm from CONTRACT_HOTKEY.md §Focus Detection,
extracted into the resolver.

**discover_sessions:**

1. Enumerate all X11 windows with `_NET_WM_PID`.
2. For each, check if any registered session PID is a descendant.
3. Return matching sessions.

### X11Hotkey (HotkeyProvider)

- Uses `XGrabKey` on the root window.
- Masks NumLock/CapsLock bits.
- Delivers key events via the X11 event queue.

Same behavior as CONTRACT_HOTKEY.md §Hotkey Registration.

### X11Clipboard (ClipboardProvider)

- Writes to the X11 `CLIPBOARD` selection.
- Reads from the X11 `CLIPBOARD` selection.
- Handles the X11 selection ownership protocol (the writer must
  serve selection requests until ownership is lost).

---

## Expected Adapters

These adapters are anticipated but not specified in detail until
implementation. Requirements listed here are constraints, not
full contracts.

### tmux

**TmuxResolver (SessionResolver):**

- Uses `tmux list-panes` and `tmux display-message` to enumerate
  panes and determine the active pane.
- Matches pane PIDs to registered session child PIDs.
- Works over a tmux control channel — does not require X11.

**TmuxHotkey (HotkeyProvider):**

- Binds keys via `tmux bind-key`.
- Delivers events via tmux hooks or a control-mode subscription.
- Only works within the tmux session — not system-global.

**TmuxClipboard (ClipboardProvider):**

- Reads and writes tmux paste buffers.
- Useful in headless/SSH environments without X11.

### macOS

**MacResolver (SessionResolver):**

- Uses Accessibility APIs or `NSWorkspace` to determine the
  frontmost terminal window.
- Maps to clippy sessions via PID matching.

**MacHotkey (HotkeyProvider):**

- Uses `CGEventTap` or the Carbon `RegisterEventHotKey` API.
- System-global key capture.

**MacClipboard (ClipboardProvider):**

- Uses `NSPasteboard` (or shells out to `pbcopy`/`pbpaste`).

---

## Broker Integration

### Resolver registration

The broker does not select or manage resolvers directly. The hotkey
client (or a future orchestrator process) composes the adapter set
and uses them to satisfy broker requests.

The broker's responsibilities are unchanged:

- Store sessions and turns.
- Execute capture and paste on request.
- Accept inject commands for paste delivery.

The broker does not import or depend on any resolver code.

### Hotkey client changes

In v2, the hotkey client:

1. Loads the configured adapter set.
2. Calls `HotkeyProvider.register()` instead of `XGrabKey`.
3. Calls `SessionResolver.focused_session()` instead of the
   X11 algorithm.
4. All other behavior (broker IPC, action serialization, lifecycle)
   remains unchanged from CONTRACT_HOTKEY.md.

The hotkey client becomes platform-agnostic. Platform knowledge
lives entirely in the adapter implementations.

### Sink changes

In v2, the `clipboard` sink (CONTRACT_REGISTRY.md) delegates to
`ClipboardProvider.write()` instead of manipulating X11 atoms
directly.

---

## Non-Guarantees

- The resolver abstraction does not guarantee that all adapters
  are feature-equivalent. Some platforms may lack capabilities
  (e.g., no global hotkeys in plain tmux without X11).
- Adapters are not hot-swappable. Changing adapters requires
  restarting the hotkey client.
- The resolver does not abstract PTY behavior — PTY allocation
  and I/O are platform-independent (POSIX) and remain in
  CONTRACT_PTY.md.
- The resolver does not abstract the broker protocol — IPC is
  Unix domain sockets on all platforms.

---

## Version History

| Version | Changes                                              |
|---------|------------------------------------------------------|
| v2      | Initial definition. Three sub-interfaces. X11 reference adapter. |
