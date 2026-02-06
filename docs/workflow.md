# workflow.md — clippy

How clippy fits into agent-driven development workflows.

This document is **explanatory**. It illustrates the design intent
behind clippy by describing the workflow it enables. It does not
introduce new guarantees or supported behavior.

For normative behavior, see the contracts in `docs/contracts/`.

---

## Two Planes

Agent-driven development operates on two distinct planes:

### Operator plane

- Fast, ephemeral, lossy by design.
- Optimized for **momentum**.
- Concerned with moving agent output between sessions right now.
- clippy lives here.

### Artifact plane

- Slow, durable, curated.
- Optimized for **meaning**.
- Concerned with preserving decisions, patterns, and rules.
- Systems like [gastown](https://github.com/steveyegge/gastown)
  live here.

**Nothing crosses planes automatically.**

Promotion from operator to artifact is always deliberate, manual,
and edited. This is a feature, not a limitation.

---

## The Operator Loop

The core development cycle is tight micro-handoffs between agents:

```
PLAN → IMPLEMENT → REVIEW → IMPLEMENT → …
```

Each arrow is a clippy relay: capture from one session, paste into
another. The loop runs many times per task. Most iterations produce
nothing worth keeping.

### PLAN → IMPLEMENT

1. Planner session finishes a turn.
2. Capture hotkey.
3. Switch to implementer session.
4. Paste hotkey.

The plan is tactical and provisional. It will likely change within
minutes. This does not go to the artifact plane.

### IMPLEMENT → REVIEW

1. Implementer finishes a turn (code, diffs, rationale).
2. Capture hotkey.
3. Switch to reviewer session.
4. Paste hotkey.

This is validation, not knowledge capture. Still operator plane.

### REVIEW → IMPLEMENT

Same relay, reversed. The reviewer's feedback goes back to the
implementer. This loop may run several times before the task
stabilizes.

All of this is clippy's job: horizontal relay between sessions,
zero friction, zero ceremony.

---

## Promotion to Artifact Plane

Artifact systems enter the picture only at **phase boundaries** —
moments when the operator loop stabilizes or shifts.

There are three canonical promotion moments:

### A plan stabilizes

The planning loop has converged. The plan isn't changing anymore.
It has become a reference point.

**Action**: Capture the plan from the planner session. Paste it
into the artifact system. Edit it: add a title, remove
conversational cruft, surface invariants. Save.

The artifact holds *what the plan became*, not how it was debated.

### Implementation produces reusable insight

The implementer explains something non-obvious, architectural,
or likely to recur.

**Action**: Capture the turn. Paste it into a fragment in the
artifact system. Add a line or two of framing.

The result is a reusable reference, not tied to a single PR
or session.

### Review yields a rule

The reviewer surfaces a general constraint, a repeated failure
mode, or a standard worth enforcing.

**Action**: Capture the review turn. Paste it into the artifact
system's rule collection. Normalize the language from critique
to rule.

The artifact system accumulates *standards*, not arguments.

---

## Backflow: Artifact → Operator

Long-term thinking re-enters the operator loop when a new planning
session starts and prior decisions should anchor it.

**Action**: Open the relevant artifact. Skim or select. Paste
selectively into the planner session. Continue the live loop.

No automation. Human judgment decides relevance.

---

## The Separation Rule

> If you haven't edited it, it doesn't belong on the artifact plane.

Raw agent output is ephemeral by default. Only curated, edited
output earns persistence. This single rule preserves signal.

---

## Why This Matters for clippy

clippy's design decisions follow from this workflow:

- **Turns are ephemeral** (v0 single-slot, v1 bounded ring) because
  most turns are consumed immediately and never referenced again.
- **Relay is byte-exact** because the user — not clippy — decides
  what to keep, edit, or discard.
- **No persistence** (until explicit opt-in in v4) because the
  artifact plane is a separate system with separate curation.
- **No integration with artifact systems** because the boundary
  is a human decision, not an API call.

clippy moves **horizontally** — fast, between sessions.
Artifact systems move **vertically** — deep, into durable knowledge.

The two are complementary. clippy does not need to know what
artifact system you use, or whether you use one at all.
