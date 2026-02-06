# CONTRACT_TURN.md — clippy

This contract defines the **turn** — clippy's fundamental unit of state.

All other contracts depend on this definition.

---

## Purpose

A turn is the complete output emitted by an agent between one prompt
and the next.

clippy exists to make turns detectable, capturable, and relayable.

---

## Core Invariant

A completed turn is:

| Property        | Meaning                                                              |
|-----------------|----------------------------------------------------------------------|
| **Detectable**  | clippy can identify when a turn has started and ended                |
| **Complete**    | A turn is recognized only after the agent signals readiness for input |
| **Addressable** | A turn can be referenced (opaque handle in v0; stable ID in v1+)    |
| **Relayable**   | A turn's content can be copied and injected elsewhere without loss   |

This invariant MUST hold across all versions.

---

## Definitions

**Agent**: The child process wrapped by a clippy PTY session.
clippy does not know or care what the agent is — only that it produces
output and eventually displays a prompt.

**Prompt**: A string emitted by the agent that signals it is waiting for
user input. Detected by matching against a user-configured regex pattern.

**Turn boundary**: The moment a prompt pattern is matched in the agent's
output stream. This marks the end of one turn and the beginning of a new
input opportunity.

**Turn content**: The agent output between the end of the user's submitted
input and the next turn boundary, exclusive of the prompt itself.

**Completed turn**: A turn whose closing boundary has been detected.
Incomplete turns are never exposed by clippy.

---

## Prompt-Pattern Matching

### Pattern format

Prompt patterns are **regular expressions** (Rust `regex` crate syntax).

Patterns are matched against the agent's output stream **after stripping
ANSI escape sequences**. Raw terminal control codes MUST NOT affect
prompt detection.

### Presets

clippy ships named presets for common agents:

| Preset     | Description               |
|------------|---------------------------|
| `claude`   | Claude Code CLI           |
| `aider`    | Aider CLI                 |
| `generic`  | Common `> ` style prompts |

> **DECISION: preset-patterns**
>
> Exact regex patterns for each preset MUST be determined by testing
> against real agent output. Patterns are placeholders until validated.

### Configuration

- Each session MUST specify a prompt pattern at launch time.
- A session MAY reference a preset by name or provide a custom regex.
- If no pattern is specified, the `generic` preset is used as fallback.
- Patterns are **immutable** for the lifetime of a session.

### Matching rules

1. The pattern is tested against each **line** of output (after ANSI
   stripping). Multi-line prompt patterns are not supported in v0.
   Patterns containing literal newlines MUST be rejected at
   configuration time with a diagnostic.
2. A match **anywhere in the line** constitutes a prompt detection.
3. Consecutive prompt matches without intervening agent output
   MUST NOT produce empty turns.

### First-prompt handling

When a session starts, the agent typically emits an initial prompt before
any user input. This first prompt match is a **session-ready** signal,
not a turn boundary. No turn is produced until:

1. The user submits input after the initial prompt, AND
2. The agent emits output, AND
3. A subsequent prompt is detected.

---

## Turn Content

### Boundaries

- **Start**: The first byte of agent output after the user's input
  has been submitted.
- **End**: The last byte of agent output before the line containing
  the detected prompt.

### Encoding

- Turn content is captured as **raw bytes**.
- clippy guarantees byte-for-byte fidelity of captured content.
- ANSI escape sequences are **preserved** in turn content.
  (Stripping applies only to prompt detection, not to capture.)
- Consumers MAY interpret content as UTF-8. clippy does not enforce
  or validate encoding.

### Exclusions

- The prompt line itself is **excluded** from turn content.
- Echoed user input is **excluded** from turn content.

> **DECISION: echo-stripping**
>
> The mechanism for excluding echoed user input (PTY echo tracking,
> input-aware buffering, or other) is an implementation detail.
> This contract requires only that the final turn content does not
> contain the user's input text.

### Content size

Turn content size is **unbounded** in v0. Implementations MAY impose
configurable limits. The contract does not specify a maximum.

---

## Completeness

A turn is **complete** if and only if:

1. A prompt was detected (establishing a response window).
2. The agent emitted output after the user's input.
3. A subsequent prompt was detected (closing the response window).

Consequences:

- If the agent never shows a subsequent prompt (crash, hang),
  **no completed turn is produced**.
- There is no timeout-based completion. Completion is strictly
  prompt-driven.

### Interruption

If the user interrupts the agent (e.g., Ctrl+C) and the agent then
shows a prompt, a turn IS produced — but it may contain partial output.

The turn MUST be marked with an **interrupted** flag.

- v0: The flag is informational. The turn is still captured and relayable.
- v1+: The flag becomes part of structured turn metadata
  (see CONTRACT_REGISTRY.md).

---

## Addressability

### v0

Turns are referenced by **position only**:

- "The latest completed turn for session S."
- No stable identifiers.
- No history — each new completed turn **replaces** the previous one.

### v1+

Turns receive stable identifiers. See CONTRACT_REGISTRY.md.

---

## Relayability

A turn's content can be:

- Read from the broker's per-session turn buffer.
- Written to the broker's global relay buffer.
- Injected into another session's PTY input.

Relay MUST preserve byte content exactly. No transformation, wrapping,
or formatting is applied by clippy during relay.

---

## Non-Guarantees

- clippy does not guarantee that turn content is semantically meaningful.
- clippy does not parse, interpret, or validate turn content.
- clippy does not detect or handle multi-turn conversations —
  each turn is independent.
- clippy makes no guarantees about agent behavior or correctness.

---

## Version History

| Version | Changes                                                                |
|---------|------------------------------------------------------------------------|
| v0      | Initial definition. Single latest turn per session. Opaque handle.     |
| v1      | Stable turn IDs. Metadata (timestamp, length, interrupted). Ring buffer. |
