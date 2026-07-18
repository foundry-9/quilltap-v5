# SPA test fixtures

Committed oracle output the SPA's own differentials replay. These are **copies**
of harness oracle output, never hand-edited: if one looks wrong, regenerate it
and investigate the diff — do not patch the fixture.

## `pascal-custom-tool-definition.oracle.ndjson`

**159 rows** (10 `title` + 149 `definition`; 53 accept / 96 reject). Drives
`app/pascal/custom-tool-types.corpus.spec.ts`, which replays every row through
the SPA's hand-ported schema module and byte-compares the verdict, the parsed
data (`JSON.stringify`), the unknown-key report, and the full
`formatDefinitionIssues` rejection sentence against v4's REAL Zod output.

The sentence is compared rather than regex-matched because it is payload:
`loadToolsFromMount` stores it as a load error's `reason`, and both the chat
roster route and the Workbench library route return it verbatim — so a browser
that phrased it differently would be disagreeing with the server about the same
file.

- **Provenance:** v4 `616930db` (4.8.0-dev.75), regenerated 2026-07-18 (P4.d8
  unit 1 / §C). Byte-identical to the copy the Rust
  `pascal_custom_tool_definition_equivalence` differential consumed at the same
  commit — that differential was re-run green over this exact output, confirming
  no v4 drift since the corpus was authored. The `d68638b4` corpus was 115 rows
  (10 title + 105 definition); the `616930db` llm-consult + contains/ncontains
  drift added **44 definition rows** (the `llm` block accept/reject arms, the
  `llm` subject, containment across all four subjects, the `maxOutput` bounds)
  and moved two PRE-EXISTING rows, whose at-least-one message widened to the
  eight-key form.
- **Owner of the generator:** `harness/oracle/cases/pascal-custom-tool-definition.ts`
  (lane AY's tree — this is a consumer copy).
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  cd ~/source/quilltap-server            # must be at the pinned v4 baseline
  npx tsx <V5>/harness/oracle/cases/pascal-custom-tool-definition.ts \
    > <V5>/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson
  ```

  If the v4 tree is dirty, generate from a pinned detached worktree at the
  baseline instead. Expect 159 lines; a shorter file means the generator errored
  and left the old one in place.
