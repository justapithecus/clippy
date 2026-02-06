# ARCH_INDEX.md — clippy

This file is a **fast lookup table for agents** opening this repository.

It summarizes *what exists and where*, not how things are implemented.

Normative behavior is defined by contracts and architecture documents, not code.

Maintenance rule:
Update this file only when a new subsystem boundary becomes real.
Do not update for internal refactors.

---

## Root

- `README.md` — human-oriented overview
- `ROADMAP.md` — versioned capability roadmap
- `docs/ARCH_INDEX.md` — this file

---

## docs/

Agent-facing documentation.

- `ARCH_INDEX.md` — architectural navigation index
- `workflow.md` — explanatory: operator plane vs artifact plane workflow

### docs/contracts/

Normative subsystem contracts. These define what must be true.

- `CONTRACT_TURN.md` — turn object: core invariant, prompt-pattern detection, completeness
- `CONTRACT_PTY.md` — PTY wrapper: I/O transparency, session lifecycle, signals
- `CONTRACT_BROKER.md` — broker daemon: IPC protocol, session table, relay buffer
- `CONTRACT_HOTKEY.md` — hotkey client: global hotkeys, X11 focus detection, actions
- `CONTRACT_REGISTRY.md` — turn registry (v1): ring buffer, turn IDs, metadata, sinks
- `CONTRACT_RESOLVER.md` — resolver abstraction (v2): platform adapter sub-interfaces

---

## src/ (planned)

Implementation code.

Expected contents (v0+):
- PTY wrapper
- broker daemon
- hotkey clients
- platform adapters

Code structure is secondary to documented contracts.

---

## Architectural Notes

- clippy treats agent turns as first-class objects
- UI mechanisms (hotkeys, clipboard, terminals) are adapters
- Determinism and explicit control are preferred over inference

This index is intentionally minimal.
