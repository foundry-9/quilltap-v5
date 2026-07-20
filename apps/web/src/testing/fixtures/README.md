# SPA test fixtures

Committed oracle output the SPA's own differentials replay. These are **copies**
of harness oracle output, never hand-edited: if one looks wrong, regenerate it
and investigate the diff — do not patch the fixture.

## `pascal-custom-tool-definition.oracle.ndjson`

**175 rows** (10 `title` + 165 `definition`; 58 accept / 107 reject). Drives
`app/pascal/custom-tool-types.corpus.spec.ts`, which replays every row through
the SPA's hand-ported schema module and byte-compares the verdict, the parsed
data (`JSON.stringify`), the unknown-key report, and the full
`formatDefinitionIssues` rejection sentence against v4's REAL Zod output.

The sentence is compared rather than regex-matched because it is payload:
`loadToolsFromMount` stores it as a load error's `reason`, and both the chat
roster route and the Workbench library route return it verbatim — so a browser
that phrased it differently would be disagreeing with the server about the same
file.

- **Provenance:** v4 `7e6d13e5` (4.8.0-dev.92), regenerated 2026-07-20 (P4.d10
  unit 5 / §C, from the `/private/tmp/qt-v4-pin-7e6d13e5` pinned worktree).
  Byte-identical to the copy the Rust
  `pascal_custom_tool_definition_equivalence` differential consumed at the same
  commit — that differential was re-run green over this exact output. The
  `616930db` corpus was 159 rows (10 title + 149 definition; 53 accept / 96
  reject); the `f48f34dc` state-cascade drift added **16 definition rows** (the
  `$state` reference accept/reject arms across the roll-field / operand /
  parameter-default unions and the fallback-typing validation).
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
