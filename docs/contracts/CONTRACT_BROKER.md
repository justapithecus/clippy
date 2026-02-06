# CONTRACT_BROKER.md — clippy

This contract defines the **broker daemon** — the central coordinator
that manages sessions, stores turns, and executes capture/paste
operations.

Depends on: CONTRACT_TURN.md, CONTRACT_PTY.md

---

## Purpose

The broker is a long-running daemon that:

1. Tracks active PTY wrapper sessions.
2. Stores the latest completed turn per session.
3. Maintains a global relay buffer for cross-session transfer.
4. Executes capture and paste on behalf of hotkey clients.

The broker holds all shared state. PTY wrappers and hotkey clients
are stateless with respect to relay — they delegate to the broker.

---

## Definitions

**Session table**: The broker's in-memory map of active sessions,
keyed by Session ID.

**Session entry**: A record in the session table containing:
session ID, connection handle, prompt pattern, and the latest
completed turn (if any).

**Latest-turn buffer**: The single-slot turn store within each
session entry. Replaced on every new completed turn (v0).

**Relay buffer**: A single-slot global buffer holding the most
recently captured turn content. Written by capture, read by paste.

**Client**: Any process connected to the broker — either a PTY
wrapper or a hotkey client.

---

## Transport

### Socket path

The broker listens on a Unix domain socket at:

```
$XDG_RUNTIME_DIR/clippy/broker.sock
```

If `$XDG_RUNTIME_DIR` is unset, the broker MUST refuse to start and
emit a diagnostic. clippy does not fall back to `/tmp` or other
world-writable locations.

The broker MUST create the `clippy/` subdirectory if it does not exist.

### Socket permissions

The socket file MUST be created with mode `0700` on the parent
directory (user-only access). The broker MUST NOT listen on a
TCP socket or any network-accessible transport.

---

## Wire Protocol

### Framing

All messages use **length-prefixed framing**:

```
[4 bytes: payload length, big-endian u32]
[N bytes: MessagePack payload]
```

Maximum payload size: 16 MiB. Messages exceeding this limit MUST
be rejected and the connection closed.

### Encoding

Message payloads are **MessagePack** maps. Every message MUST
contain at minimum:

| Field  | Type   | Description                               |
|--------|--------|-------------------------------------------|
| `type` | string | Message type identifier                   |
| `id`   | u32    | Request identifier (unique per connection) |

Responses echo the `id` of the originating request.

### Handshake

The first message from any client MUST be a `hello`:

| Field      | Type   | Description              |
|------------|--------|--------------------------|
| `type`     | string | `"hello"`                |
| `id`       | u32    | `0`                      |
| `version`  | u32    | Protocol version (v0: 1) |
| `role`     | string | `"wrapper"` or `"client"` |

The broker responds with:

| Field     | Type   | Description                     |
|-----------|--------|---------------------------------|
| `type`    | string | `"hello_ack"`                   |
| `id`      | u32    | `0`                             |
| `status`  | string | `"ok"` or `"error"`             |
| `error`   | string | Present only if status is error |

If the protocol version is unsupported, the broker MUST respond
with an error and close the connection.

### Request / Response

After handshake, all communication is **request → response**.

- Clients send requests; the broker sends exactly one response per
  request.
- The broker MAY send **unsolicited commands** to wrapper connections
  (for paste injection). These use `id: 0` and do not expect a
  response.
- Requests with an unknown `type` MUST receive an error response,
  not silence.

---

## Session Management

### Register

Sent by a wrapper after handshake.

Request:

| Field     | Type   | Description                          |
|-----------|--------|--------------------------------------|
| `type`    | string | `"register"`                         |
| `id`      | u32    | Request ID                           |
| `session` | string | Session ID (from CONTRACT_PTY.md)    |
| `pid`     | u32    | Child process PID                    |
| `pattern` | string | Prompt pattern name or custom regex  |

Response: `status: "ok"` or error (duplicate session ID, etc.).

On success, the broker adds an entry to the session table.

### Deregister

Sent by a wrapper during clean shutdown.

Request:

| Field     | Type   | Description  |
|-----------|--------|--------------|
| `type`    | string | `"deregister"` |
| `id`      | u32    | Request ID   |
| `session` | string | Session ID   |

Response: `status: "ok"` (even if session was already removed).

On success, the broker removes the session entry and frees the
latest-turn buffer. If the relay buffer references a turn from
this session, the relay buffer is **not** cleared — the content
was already captured.

### Implicit deregister

If a wrapper's connection drops without a `deregister` message,
the broker MUST treat this as an implicit deregister. The session
entry is removed after connection close is detected.

---

## Turn Storage

### TurnCompleted

Sent by a wrapper when a new completed turn is detected.

Request:

| Field         | Type   | Description                        |
|---------------|--------|------------------------------------|
| `type`        | string | `"turn_completed"`                 |
| `id`          | u32    | Request ID                         |
| `session`     | string | Session ID                         |
| `content`     | binary | Turn content (raw bytes)           |
| `interrupted` | bool   | Whether the turn was interrupted   |

Response: `status: "ok"` or error (unknown session, etc.).

On success, the broker **replaces** the session's latest-turn
buffer with the new content.

### Storage guarantees

- The broker stores turn content as raw bytes, unmodified.
- The broker MUST NOT interpret, parse, or transform turn content.
- In v0, each session holds at most one turn (the latest).
- In v1+, storage is delegated to the turn registry
  (CONTRACT_REGISTRY.md).

---

## Capture Operation

Initiated by a hotkey client. Copies a session's latest turn into
the global relay buffer.

### Capture

Request:

| Field     | Type   | Description                    |
|-----------|--------|--------------------------------|
| `type`    | string | `"capture"`                    |
| `id`      | u32    | Request ID                     |
| `session` | string | Source session ID               |

Response:

| Field    | Type   | Description                             |
|----------|--------|-----------------------------------------|
| `type`   | string | `"response"`                            |
| `id`     | u32    | Matching request ID                     |
| `status` | string | `"ok"` or `"error"`                     |
| `error`  | string | Error reason (if status is error)       |
| `size`   | u32    | Byte size of captured content (if ok)   |

Semantics:

- The broker copies the session's latest-turn buffer into the
  relay buffer, replacing any previous relay content.
- The source session's latest-turn buffer is **not** cleared.
- If the session has no completed turn, the broker MUST return
  an error with reason `"no_turn"`.
- If the session does not exist, the broker MUST return an error
  with reason `"session_not_found"`.

---

## Paste Operation

Initiated by a hotkey client. Injects the relay buffer content
into a target session's PTY input.

### Paste

Request:

| Field     | Type   | Description              |
|-----------|--------|--------------------------|
| `type`    | string | `"paste"`                |
| `id`      | u32    | Request ID               |
| `session` | string | Target session ID        |

Response:

| Field    | Type   | Description                         |
|----------|--------|-------------------------------------|
| `type`   | string | `"response"`                        |
| `id`     | u32    | Matching request ID                 |
| `status` | string | `"ok"` or `"error"`                 |
| `error`  | string | Error reason (if status is error)   |

Semantics:

1. The broker reads the relay buffer content.
2. The broker sends an **inject command** to the target wrapper
   over its persistent connection.
3. The wrapper writes the injected bytes to the child's PTY
   master (indistinguishable from user input to the child).
4. The broker responds to the hotkey client with success.

Inject command (broker → wrapper, unsolicited):

| Field     | Type   | Description              |
|-----------|--------|--------------------------|
| `type`    | string | `"inject"`               |
| `id`      | u32    | `0` (unsolicited)        |
| `content` | binary | Bytes to write to PTY    |

The wrapper MUST write the injected bytes to the child's PTY input
promptly and without modification.

Error conditions:

- Relay buffer is empty: return error with reason `"buffer_empty"`.
- Target session does not exist: return error with reason
  `"session_not_found"`.
- Target wrapper connection is broken: return error with reason
  `"session_disconnected"`.

### Relay buffer persistence

- The relay buffer is **not** cleared after a paste operation.
  The same content can be pasted multiple times.
- The relay buffer is cleared only when overwritten by a new
  capture or when the broker shuts down.

---

## Session Query

### ListSessions

Request:

| Field  | Type   | Description        |
|--------|--------|--------------------|
| `type` | string | `"list_sessions"`  |
| `id`   | u32    | Request ID         |

Response:

| Field      | Type   | Description                  |
|------------|--------|------------------------------|
| `type`     | string | `"response"`                 |
| `id`       | u32    | Matching request ID          |
| `status`   | string | `"ok"`                       |
| `sessions` | array  | List of session descriptors  |

Each session descriptor:

| Field      | Type   | Description                       |
|------------|--------|-----------------------------------|
| `session`  | string | Session ID                        |
| `pid`      | u32    | Child PID                         |
| `has_turn` | bool   | Whether a completed turn exists   |

This message is available to any connected client. It is intended
for tooling and diagnostics, not for normal capture/paste flow.

---

## Daemon Lifecycle

### Startup

1. Verify `$XDG_RUNTIME_DIR` is set.
2. Create `$XDG_RUNTIME_DIR/clippy/` if it does not exist (mode 0700).
3. Attempt to bind the socket.
4. If bind fails (EADDRINUSE): check if the existing socket is live.
   - If live: exit with a diagnostic ("broker already running").
   - If stale: remove the socket file and retry bind.
5. Begin accepting connections.

### Running

- The broker runs indefinitely until terminated.
- The broker is single-threaded or async — the contract does not
  constrain the concurrency model, only that operations are
  serialized with respect to shared state (session table, relay
  buffer). No torn reads or writes.

### Shutdown

On SIGTERM or SIGINT:

1. Stop accepting new connections.
2. Close all active connections (wrappers will observe disconnect).
3. Remove the socket file.
4. Exit with code 0.

### State durability

The broker has **no persistence**. All state — the session table,
latest-turn buffers, and the relay buffer — is in-memory only and
lost on daemon exit.

This is intentional. clippy does not record, log, or persist agent
output unless the user explicitly opts in (v4+).

---

## Error Semantics

All error responses include a machine-readable `error` field:

| Error reason           | Meaning                                     |
|------------------------|---------------------------------------------|
| `session_not_found`    | The specified session ID is not registered   |
| `no_turn`              | The session has no completed turn            |
| `buffer_empty`         | The relay buffer has not been written to     |
| `session_disconnected` | The target wrapper's connection is broken    |
| `duplicate_session`    | A session with this ID is already registered |
| `version_mismatch`     | Protocol version not supported               |
| `unknown_type`         | Unrecognized message type                    |
| `payload_too_large`    | Message exceeds 16 MiB limit                |

Error responses MUST NOT close the connection unless the error is
a protocol-level failure (version mismatch, payload too large,
malformed framing).

---

## Non-Guarantees

- The broker does not authenticate clients. Access control is
  via socket permissions only.
- The broker does not encrypt IPC traffic. The socket is local
  and user-scoped.
- The broker does not guarantee message ordering across different
  client connections.
- The broker does not monitor child process health — it only
  tracks connection liveness.

---

## Version History

| Version | Changes                                                              |
|---------|----------------------------------------------------------------------|
| v0      | Initial definition. Single latest turn per session. Single relay slot. |
| v1      | Turn storage delegated to registry. Relay buffer references turn IDs.  |
| v2      | Session discovery delegated to resolvers. Resolver registration added. |
