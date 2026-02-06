# CONTRACT_PTY.md — clippy

This contract defines the **PTY wrapper** — the component that wraps a
single agent session, mediates its I/O, and feeds output to the turn
detector.

Depends on: CONTRACT_TURN.md

---

## Purpose

The PTY wrapper allocates a pseudoterminal, runs an agent process on the
slave side, and sits transparently between the user and the agent. Its
only additions to the I/O path are:

1. Tapping output for turn detection.
2. Registering the session with the broker.

The agent MUST NOT be able to observe that clippy is present.

---

## Definitions

**Wrapper**: The clippy process that holds the master side of the PTY
pair and mediates I/O between the user's terminal and the child.

**Child**: The agent process running on the slave side of the PTY.

**Session**: The logical unit representing one wrapper + child pair,
from spawn to exit.

**Session ID**: An opaque, unique identifier assigned at spawn time.
Stable for the lifetime of the session. Used by the broker and hotkey
clients to address this session.

---

## I/O Transparency

### Output path (child → user)

All bytes written by the child to the PTY MUST be:

1. Forwarded to the user's terminal **unmodified and without delay**.
2. Copied to the turn detector for prompt-pattern analysis.

These two operations are independent. Turn detection MUST NOT block,
delay, or alter the output stream visible to the user.

### Input path (user → child)

All bytes from the user's terminal MUST be forwarded to the child's
PTY input **unmodified and without delay**.

Exception: the broker MAY inject bytes into the child's PTY input
during a **paste operation** (see CONTRACT_BROKER.md). Injected bytes
are indistinguishable from user input to the child.

### Invariant

The user's visible experience MUST be identical to running the agent
directly in the terminal, minus any out-of-band feedback from clippy
(see CONTRACT_HOTKEY.md).

clippy MUST NOT:

- Inject bytes into the output stream.
- Filter, modify, or reorder output bytes.
- Filter, modify, or reorder input bytes.
- Add latency perceptible to the user under normal operation.

---

## PTY Allocation

- The wrapper MUST allocate a new PTY pair (master/slave) per session.
- The slave side is set as the child's controlling terminal.
- The master side is held by the wrapper for I/O mediation.
- Initial terminal dimensions MUST match the user's terminal at
  spawn time.

---

## Terminal Management

### Raw mode

The wrapper MUST place the user's terminal into raw mode at startup
so that keystrokes are forwarded immediately (no line buffering,
no local echo, no signal generation by the terminal driver).

### Restoration

On exit — whether clean or abnormal — the wrapper MUST restore the
user's terminal to its original state (the settings captured before
entering raw mode).

If restoration fails, the wrapper SHOULD emit a diagnostic to stderr.

---

## Turn Detector Integration

The wrapper feeds a copy of all child output bytes to the turn detector.

The turn detector:

- Applies prompt-pattern matching per CONTRACT_TURN.md.
- Emits turn-boundary events when a prompt is detected.
- Extracts completed turn content (excluding prompt and echoed input).

When a completed turn is produced:

- The wrapper stores it in the session's **latest-turn buffer**
  (single slot, replaced on each new completed turn in v0).
- The wrapper notifies the broker that a new turn is available.

The turn detector runs **in-process** with the wrapper. It MUST NOT
introduce blocking I/O or unbounded memory growth in the output path.

---

## Session Identity

- Each session MUST have a unique Session ID assigned at spawn time.
- The ID MUST be unique across all concurrent sessions on the host.
- The ID is opaque to consumers — no semantics may be derived from
  its value.
- The ID is stable for the session's lifetime and MUST NOT be reused
  after the session exits.

> **DECISION: session-id-scheme**
>
> The generation scheme (UUID v4, monotonic counter with PID prefix,
> or other) is an implementation choice. The contract requires
> uniqueness and opacity only.

---

## Lifecycle

### Spawn

1. Capture the user's current terminal settings.
2. Allocate a PTY pair.
3. Fork the child process on the slave side.
4. Place the user's terminal into raw mode.
5. Register the session with the broker (CONTRACT_BROKER.md).
6. Begin I/O mediation and turn detection.

If the broker is unreachable at spawn time, the wrapper MUST still
run the child. Turn detection proceeds locally; turns are buffered
but not relayable until broker registration succeeds.

### Running

- I/O mediation continues until the child exits or the wrapper
  receives a termination signal.
- The wrapper monitors the child for exit (via `waitpid` or
  equivalent).

### Exit

1. The child process exits (or is killed).
2. The wrapper drains any remaining output from the PTY master.
3. The wrapper deregisters the session from the broker.
4. The wrapper restores the user's terminal settings.
5. The wrapper exits with the **child's exit code**.

If the child is terminated by a signal, the wrapper MUST exit in a
manner that preserves the signal information (e.g., re-raise the
signal after cleanup).

---

## Signal Forwarding

| Signal    | Behavior                                                  |
|-----------|-----------------------------------------------------------|
| SIGINT    | Forward to child process group.                           |
| SIGTERM   | Forward to child process group. Begin graceful shutdown.  |
| SIGHUP    | Forward to child process group.                           |
| SIGQUIT   | Forward to child process group.                           |
| SIGWINCH  | Update child PTY dimensions (see below). Do NOT forward.  |
| SIGTSTP   | Forward to child process group. Suspend wrapper.          |
| SIGCONT   | Forward to child process group. Resume wrapper.           |

Signals not listed above: forward to the child process group by default.

SIGWINCH is handled specially — it triggers a window-size update, not
signal delivery to the child. The child observes the resize via the
PTY dimension change, which causes the kernel to deliver SIGWINCH to
the child automatically.

---

## Window Size

When the user's terminal is resized (SIGWINCH received by the wrapper):

1. Read the new dimensions from the user's terminal.
2. Set the child PTY dimensions via `ioctl(TIOCSWINSZ)`.

This MUST happen promptly. The child's view of terminal dimensions
MUST track the user's actual terminal at all times.

---

## Environment

The child process MUST inherit the user's environment unmodified.

clippy MUST NOT set environment variables that reveal its presence.
The agent must remain unaware that it is wrapped.

> **DECISION: env-opt-in**
>
> A future version MAY add an opt-in environment variable
> (e.g., `CLIPPY_SESSION_ID`) for tools that want to cooperate
> with clippy. This MUST NOT be set by default.

---

## Non-Guarantees

- The wrapper does not manage the child's internal state.
- The wrapper does not interpret the child's output beyond
  prompt-pattern matching.
- The wrapper does not guarantee the child behaves correctly.
- The wrapper does not sandbox or isolate the child process.

---

## Version History

| Version | Changes                                                        |
|---------|----------------------------------------------------------------|
| v0      | Initial definition. Single latest-turn buffer. Broker registration. |
| v1      | Turn buffer becomes ring buffer (delegated to CONTRACT_REGISTRY.md). |
| v2      | Session discovery delegated to resolver (CONTRACT_RESOLVER.md).     |
