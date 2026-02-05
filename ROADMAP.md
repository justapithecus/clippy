# ROADMAP.md — Clippy

This document describes the planned evolution of **clippy**, an agent operator
tool for capturing and relaying completed AI assistant turns with minimal
friction.

The roadmap is versioned by *capability*, not time.

---

## Core Invariant

A completed assistant **turn** is a first-class object:
- detectable
- complete
- addressable
- relayable independently of UI

All versions must preserve this invariant.

---

## v0 — Terminal Turn Relay (Keystone Release)

**Goal**
Eliminate friction when relaying the most recent completed assistant response
between interactive terminal sessions.

**Scope**
- Linux + X11
- Konsole-specific session resolution
- Keyboard-only workflow
- No persistence beyond latest completed turn

**Capabilities**
- PTY wrapper per agent session
- Deterministic detection of completed assistant turns
- Per-session “latest completed turn” buffer
- Global relay buffer
- Global hotkey to capture from focused session
- Global hotkey to paste into focused session

**Non-goals**
- History
- Search
- tmux support
- macOS support
- GUI controls
- Editor plugins

**Why v0 matters**
This version establishes the operator boundary where agent output becomes
addressable state.

---

## v1 — Local Turn Registry

**Goal**
Move from copy/paste to structured, addressable turn state.

**New Capabilities**
- Ring buffer of recent completed turns per session
- Stable turn identifiers
- Turn metadata (timestamps, truncation, interruption)
- Multiple sinks (clipboard, file, injection)

Clipboard becomes one consumer, not the model.

---

## v2 — Resolver Abstraction

**Goal**
Support additional environments without destabilizing the core.

**Changes**
- Abstract session resolution behind pluggable resolvers
- Konsole resolver remains reference implementation
- Add tmux resolver (pane → PTY)
- Add macOS Terminal/iTerm resolver

Core turn detection and registry remain unchanged.

---

## v3 — Agent Routing

**Goal**
Make agent interaction composable.

**New Capabilities**
- Explicit agent-to-agent relay paths
- Turn templating and wrapping
- Structured injection (review, implementation, synthesis)

Clipboard use becomes optional.

---

## v4 — Optional Persistence & Replay

**Goal**
Enable selective memory without implicit logging.

**Capabilities**
- Explicit session snapshots
- Replay last N turns into fresh sessions
- User-controlled persistence only

---

## Design Principles

- Determinism over cleverness
- Explicit contracts over heuristics
- Adapters over conditionals
- Bootstrap leverage over polish
