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
- Memory name-resolution leaves: reinforced-importance formula, name+pronoun
  formatting, the about/holder name-set builders, and the word-boundary name
  matchers (presence / occurrence-count / about-character resolution) — the
  Unicode-boundary + lookahead regex reproduced without a backtracking engine.
- Embedding: L2 vector normalisation, the profile storage policy (Matryoshka
  truncate + optional normalise), cosine similarity with the dimension-mismatch
  guard and message, the fallback keyword/phrase scorer, the literal-phrase
  boost helpers, Float32 ↔ little-endian-byte BLOB conversion, and the legacy
  JSON-text recovery (`parseLegacyEmbeddingText` — reproducing JS `Object.values`
  ascending integer-key ordering for the index-keyed-object shape).
- Canon: the memory-extraction canon blocks (self / other ALREADY ESTABLISHED
  rendering) and the New-Chat scenario-text combiner.
- Mentioned-character scan: detecting non-participant characters named in a chat
  corpus (ASCII word-boundary alternation, longest-token-first, lowercased
  token→ids map).
- Novel-detail extraction: the deterministic proper-noun / date / currency /
  number-with-unit / CamelCase / acronym scanner (ASCII `\d`/`\b`, the JS `\s`
  whitespace set reproduced exactly, case-insensitive dedup).
- Chat-task text shaping: tool-artifact stripping, visible-conversation
  extraction, and the chat-card preview, over shared JS string primitives (the
  JS `\s`/`trim` set and UTF-16 length/slice).
- Model context limit: `getModelContextLimit` (+ `hasExtendedContext`,
  `getSafeInputLimit`) — the override / provider-default tables ported as
  constants, with the plugin model-info, `FALLBACK_PRICING` rows, and registry
  default injected; reproduces v4's lookup order and substring matching, and the
  JS-truthy fall-through on a zero/null context value.
- Cheap-model classifiers: `isCheapModel` / `estimateModelCost` /
  `getCheapestModel` and their deprecated fallback tables — the registry-sourced
  recommended-list and default-model are injected (empty / none takes the
  fallback path), the string heuristics (expensive/mid/cheap indicators, the
  dashed-vs-undashed `o1`/`o3` split) are pure.
- Version compare: documented `compareVersions`' `localeCompare` fallback (the
  malformed-input path) as a deferred ICU-collation seam — the parseable
  numeric path stays exact; faithful collation waits on the ICU-crate decision.
- Tool canonicalization: byte-stable `UniversalTool` serialization for
  cache-prefix stability — deep code-unit key-sort of `function.parameters` plus
  the tool-name array sort. The name sort is a documented `localeCompare`
  residual seam (the lowercase snake_case tool-name corpus collates identically
  under code-unit order; the ICU-collation decision is deferred).
- Number formatting: the JS `Number.prototype.toFixed` kernel (V8
  half-away-from-zero rounding on the f64's exact value, via IEEE-754
  mantissa/exponent + u128 — distinct from Rust's half-to-even formatter), and
  the display formatters built on it (`formatBytes`, `formatCostForDisplay`, and
  both the `K` and lowercase-`k` `formatTokenCount` variants).
- Small leaf utilities: chat-type/participant predicates, semver parse/compare,
  pronoun→gender hint, tag-style merge, char-count colour class.

