# CONTRACT_REGISTRY.md — clippy

This contract defines the **turn registry** — the structured,
inspectable history of completed turns that replaces the single-slot
buffer from v0.

Introduced in: v1
Depends on: CONTRACT_TURN.md, CONTRACT_BROKER.md

---

## Purpose

The turn registry promotes turns from ephemeral single-slot state
into addressable, metadata-bearing objects in a bounded history.

It also introduces the **sink abstraction**, which decouples turn
delivery from any single output mechanism (clipboard, file, PTY
injection).

---

## Definitions

**Ring buffer**: A fixed-capacity, per-session circular buffer of
completed turns. When full, the oldest turn is evicted.

**Turn ID**: A stable, globally unique identifier assigned to each
completed turn at the time of detection.

**Turn record**: A turn ID, its content (raw bytes), and associated
metadata. Stored in the ring buffer.

**Sink**: A named consumer that can receive turn content. Sinks are
the output side of the relay — they determine *where* a turn goes.

---

## Turn Identifiers

### Format

A Turn ID is a composite of session ID and a per-session monotonic
sequence number:

```
<session_id>:<seq>
```

- `session_id`: from CONTRACT_PTY.md.
- `seq`: unsigned integer, starting at 1, incremented for each
  completed turn within the session. Never reused within a session.

### Properties

| Property          | Guarantee                                      |
|-------------------|------------------------------------------------|
| **Unique**        | No two turns share an ID, globally             |
| **Stable**        | Once assigned, the ID never changes             |
| **Monotonic**     | Within a session, higher seq = more recent      |
| **Opaque to consumers** | Consumers MUST NOT parse or derive meaning from the ID structure beyond ordering within a session |

### Relay buffer

The relay buffer (CONTRACT_BROKER.md) stores a **turn reference**
in v1+: the Turn ID plus a copy of the content bytes. This enables
consumers to query metadata for the relayed turn.

---

## Turn Metadata

Each turn record carries metadata assigned at detection time:

| Field         | Type     | Description                                  |
|---------------|----------|----------------------------------------------|
| `turn_id`     | string   | Turn ID (session_id:seq)                     |
| `timestamp`   | u64      | Unix epoch millis when the turn was completed |
| `byte_length` | u32      | Size of turn content in bytes                |
| `interrupted`  | bool     | Turn was terminated by user interruption     |
| `truncated`   | bool     | Turn content was truncated due to size limit |

Metadata is immutable once assigned. It is stored alongside the
turn content in the ring buffer.

---

## Ring Buffer

### Per-session

Each session in the broker's session table maintains its own ring
buffer. The ring replaces the single-slot latest-turn buffer from v0.

### Capacity

> **DECISION: ring-default-depth**
>
> The default ring buffer depth must balance memory usage against
> useful history. Recommended default: **32 turns per session**.
> MUST be user-configurable.

### Behavior

- New completed turns are appended to the head of the ring.
- When the ring is full, the oldest turn (tail) is evicted
  silently. No notification is emitted for eviction.
- The ring MUST NOT block turn detection. If a turn is completed,
  it is stored (or the oldest is evicted to make room)
  unconditionally.

### Content size limit

> **DECISION: max-turn-size**
>
> A per-turn content size limit MAY be enforced to bound memory.
> If enforced:
>
> - Content exceeding the limit is truncated to the limit.
> - The `truncated` metadata flag is set to `true`.
> - The limit MUST be configurable.
> - Recommended default: **4 MiB per turn**.

### Latest-turn shorthand

The "latest completed turn" for a session is the head of the ring
buffer. All v0 operations that reference "the latest turn" resolve
to the ring head. No API changes are required for this alias.

---

## Backward Compatibility

v0 behavior is a degenerate case of the registry:

| v0 concept           | v1 equivalent                        |
|----------------------|--------------------------------------|
| Single latest-turn buffer | Ring buffer with depth ≥ 1      |
| Opaque turn handle   | Turn ID (session_id:seq)             |
| Relay buffer (bytes) | Relay buffer (Turn ID + bytes)       |
| Capture              | Capture (unchanged semantics)        |
| Paste                | Paste via injection sink (unchanged) |

The broker protocol is extended (not replaced). v0 message types
remain valid. New fields are additive.

---

## Protocol Extensions

The following broker message types are added or modified in v1.

### TurnCompleted (modified)

Response now includes the assigned Turn ID:

| Field     | Type   | Description            |
|-----------|--------|------------------------|
| `turn_id` | string | Assigned Turn ID       |

(All existing fields from CONTRACT_BROKER.md remain.)

### Capture (modified)

Response now includes:

| Field     | Type   | Description                       |
|-----------|--------|-----------------------------------|
| `turn_id` | string | Turn ID of the captured turn      |

### GetTurn (new)

Retrieve a specific turn by ID.

Request:

| Field     | Type   | Description  |
|-----------|--------|--------------|
| `type`    | string | `"get_turn"` |
| `id`      | u32    | Request ID   |
| `turn_id` | string | Turn ID      |

Response:

| Field         | Type   | Description                          |
|---------------|--------|--------------------------------------|
| `status`      | string | `"ok"` or `"error"`                  |
| `content`     | binary | Turn content (if ok)                 |
| `timestamp`   | u64    | Completion time (if ok)              |
| `byte_length` | u32    | Content size (if ok)                 |
| `interrupted` | bool   | Interrupted flag (if ok)             |
| `truncated`   | bool   | Truncated flag (if ok)               |

Error: `"turn_not_found"` if the turn has been evicted or the ID
is invalid.

### ListTurns (new)

List recent turns for a session.

Request:

| Field     | Type   | Description        |
|-----------|--------|--------------------|
| `type`    | string | `"list_turns"`     |
| `id`      | u32    | Request ID         |
| `session` | string | Session ID         |
| `limit`   | u32    | Max turns to return (optional, default: all in ring) |

Response:

| Field   | Type  | Description                    |
|---------|-------|--------------------------------|
| `status`| string| `"ok"` or `"error"`            |
| `turns` | array | Turn descriptors, newest first |

Each turn descriptor:

| Field         | Type   | Description         |
|---------------|--------|---------------------|
| `turn_id`     | string | Turn ID             |
| `timestamp`   | u64    | Completion time     |
| `byte_length` | u32    | Content size        |
| `interrupted` | bool   | Interrupted flag    |
| `truncated`   | bool   | Truncated flag      |

Content is **not** included in list responses. Use `GetTurn` to
retrieve content for a specific turn.

### CaptureByID (new)

Capture a specific turn (not just the latest) into the relay buffer.

Request:

| Field     | Type   | Description           |
|-----------|--------|---------------------|
| `type`    | string | `"capture_by_id"`     |
| `id`      | u32    | Request ID            |
| `turn_id` | string | Turn ID to capture    |

Response: same as `capture`.

This allows users or tools to relay any turn still in the ring,
not only the most recent one.

---

## Sink Abstraction

### Concept

A sink is a named output channel that receives turn content. Sinks
decouple "what was captured" from "where it goes."

In v0, the only delivery mechanism is PTY injection (paste). In v1,
delivery is generalized to sinks.

### Sink interface

Every sink MUST implement:

| Operation          | Description                                  |
|--------------------|----------------------------------------------|
| `name() → string`  | Unique sink identifier                       |
| `deliver(content, metadata) → Result` | Write turn content to the sink's destination |

Sinks are **synchronous** from the broker's perspective. The broker
calls `deliver` and waits for the result before responding to the
client.

### Built-in sinks (v1)

| Sink name   | Destination                          | Notes                          |
|-------------|--------------------------------------|--------------------------------|
| `inject`    | Target session's PTY input           | Same as v0 paste behavior      |
| `clipboard` | X11 clipboard selection (CLIPBOARD)  | Sets the X11 CLIPBOARD atom    |
| `file`      | Write to a user-specified file path  | Overwrites target file         |

### Sink selection

The paste hotkey (CONTRACT_HOTKEY.md) defaults to the `inject` sink,
preserving v0 behavior.

Additional sinks MAY be triggered by:

- Additional configurable hotkey bindings.
- CLI commands to the broker.

### Deliver message (new)

A generalized delivery message replaces the single-mechanism paste
for non-default sinks.

Request:

| Field     | Type   | Description                          |
|-----------|--------|--------------------------------------|
| `type`    | string | `"deliver"`                          |
| `id`      | u32    | Request ID                           |
| `sink`    | string | Sink name                            |
| `session` | string | Target session ID (for `inject` sink)|
| `path`    | string | File path (for `file` sink)          |

Required fields per sink:

| Sink        | Required fields          | Optional fields |
|-------------|--------------------------|-----------------|
| `inject`    | `session`                | —               |
| `clipboard` | —                        | —               |
| `file`      | `path`                   | —               |

Missing required fields for the target sink MUST produce an error
with reason `"missing_field"`. Unrecognized fields are ignored.

Response: `status: "ok"` or error.

The v0 `paste` message type remains valid as shorthand for
`deliver` with `sink: "inject"`.

---

## Non-Guarantees

- The registry does not persist turns across broker restarts.
- Evicted turns are gone — there is no recovery mechanism.
- The registry does not index or search turn content.
- Sink delivery is best-effort — sink failures are reported
  but do not affect turn storage.

---

## Version History

| Version | Changes                                                     |
|---------|-------------------------------------------------------------|
| v1      | Ring buffer, turn IDs, metadata, sink abstraction.          |
| v2      | Clipboard sink becomes resolver-aware (platform-agnostic).  |
