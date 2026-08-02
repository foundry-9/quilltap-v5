# SPA test fixtures

Committed oracle output the SPA's own differentials replay. These are **copies**
of harness oracle output, never hand-edited: if one looks wrong, regenerate it
and investigate the diff — do not patch the fixture.

## `pascal-custom-tool-definition.oracle.ndjson`

**299 rows** — 10 `title` + 258 `definition` + 31 `gate`. Drives
`app/pascal/custom-tool-types.corpus.spec.ts`, which replays
every row through the SPA's hand-ported schema module and byte-compares the
verdict, the parsed data (`JSON.stringify`), the unknown-key report, and the
full `formatDefinitionIssues` rejection sentence against v4's REAL Zod output.

The `gate` kind arrived with the `231be14c` round: each row is one
`evaluateToolGate` verdict against one fact sheet, serialized, so `withheldBy`'s
ABSENCE on an available verdict is part of the comparison. The SPA replays them
through its own `tool-gate.ts` — the same rows the Rust half diffs — which is
what proves the two client-safe ports agree with v4 and with each other.

The sentence is compared rather than regex-matched because it is payload:
`loadToolsFromMount` stores it as a load error's `reason`, and both the chat
roster route and the Workbench library route return it verbatim — so a browser
that phrased it differently would be disagreeing with the server about the same
file.

- **Provenance:** v4 `c4d4b0de`, regenerated 2026-08-01 at the `c4d4b0de`
  drift round's unification (P4.D35's §C extension added 63 definition rows —
  7 chipLabel, 21 accepted effects, 32 rejected effects, 3 shape-order — from
  the clean checkout; no pin needed). Byte-identical to the copy the Rust
  `pascal_custom_tool_definition_equivalence` differential consumed at the
  same commit, with that differential re-run green over this exact output.
  The `231be14c` corpus was 236 rows (10 title + 195 definition + 31 gate). The `7e6d13e5` corpus
  was 175 rows (10 title + 165 definition; 58 accept / 107 reject); the
  `6864bf0e` availability-gate drift added **30 definition rows** (the
  `availableWhen`/`withheldWhen` accept/reject arms, the both-clauses rejection,
  the literal-operand rejections, and three rows covering the pre-existing
  `z.record` sites nothing had ever reached) plus the **31 new `gate` rows**.
- **Owner of the generator:** `harness/oracle/cases/pascal-custom-tool-definition.ts`
  (lane AY's tree — this is a consumer copy).
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  cd ~/source/quilltap-server            # must be at the pinned v4 baseline
  npx tsx <V5>/harness/oracle/cases/pascal-custom-tool-definition.ts \
    > <V5>/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson
  ```

  If the v4 tree is dirty, generate from a pinned detached worktree at the
  baseline instead. Expect 175 lines; a shorter file means the generator errored
  and left the old one in place.
