# Quilltap Changelog

## Recent Changes

### 5.0-dev

Phase 1 — pure-function ports to `quilltap-core`, each with a tier-1 differential
test against the v4 oracle:

- Memory: weighting/decay, ranking blend, recall-tag multipliers, recall-history
  ring buffer.
- Write path: write-batch partitioning, main-primary policy, folder-conflict id
  remap, unique-constraint detection.
- Context: sliding-window compression sizing; per-purpose context-budget
  arithmetic (summarize trigger, recent-message count, max-available, allocation
  split); the summarisation cadence (fold/hard gate, interchange count,
  title-check crossing, turn partition); per-character context shaping
  (history-access gate, presence windows, whisper visibility, role/name
  attribution).
- Enclave: autonomous-run budget verdict and progress-toward-binding-cap.
- LLM: completion cost estimate, cost-aware model selection, model classes,
  character-based token estimation.
- Turn manager: the turn-state machine — queue ops, history-derived state, and
  the spoken-this-cycle wrap; the all-LLM auto-pause thresholds; the
  participant-list filters (user/LLM/active resolvers); the display-only
  predicted turn order; and the weighted-random next-speaker selection (with the
  RNG injected for determinism).
- Small leaf utilities: chat-type/participant predicates, semver parse/compare,
  pronoun→gender hint, tag-style merge, char-count colour class.

