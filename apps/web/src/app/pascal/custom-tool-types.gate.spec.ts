/**
 * The availability-gate arms of the browser schema twin (v4 `6864bf0e`, "custom
 * tools can be gated on the invoking character's metadata"), pinned against
 * v4's REAL Zod at `231be14c`.
 *
 * ⚠ **Refreshed at v4 `25f534c0b` (P4.D170).** `0587d1e96` gave Pascal's format
 * a `progress` family, and the sentences these rows carry MOVED with it: the
 * effect target's terminal reason gained `"progress."`, both "must test
 * something" refines now read ``a non-empty `params`, `metadata`, or
 * `progress` ``, and the gate's "must test at least one …" refine moved from the
 * metadata RECORD to the gate OBJECT (so its path is the gate, and a gate with
 * no `metadata` key at all reads as "tests nothing" rather than
 * `expected record, received undefined`). Every replacement below was MEASURED
 * against v4's real schema at that pin, not inferred — the same rows moved in
 * the shared corpus's own re-recording at that sha.
 *
 * Same shape and same teeth as `custom-tool-types.state.spec.ts`: the committed
 * corpus NDJSON is P4.d19's to regenerate (§3 of the round's shared contract),
 * so this file is the stand-in with identical rigour — every row below was
 * CAPTURED by driving v4's own `QtapCustomToolSchema.safeParse` +
 * `formatDefinitionIssues` over the input bytes, and is byte-compared here
 * exactly as the corpus spec compares the corpus: verdict, parsed data (via
 * `JSON.stringify`, so key ORDER is pinned — `availableWhen` lands between
 * `disabled` and `revealOdds` whatever order the file wrote them in), and the
 * FULL rejection sentence.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in /tmp/qt-v4-pin-231be14c, `npx tsx <script>` with Node 24:
 * import { QtapCustomToolSchema, formatDefinitionIssues } from '@/lib/pascal/custom-tool.types'
 * const r = QtapCustomToolSchema.safeParse(JSON.parse(inputJson))
 * ({ success: r.success,
 *    reason: r.success ? null : formatDefinitionIssues(r.error),
 *    data: r.success ? JSON.stringify(r.data) : null })
 * ```
 *
 * v4 anchors: `lib/pascal/custom-tool.types.ts` — `GateComparatorSchema`,
 * `ToolGateSchema`, the two optional top-level keys, and `validateGates`.
 *
 * ⚠ **Re-measured at the `322a5488e` pin (P4.D333).** Twelve rows moved — every replacement
 * re-captured by driving v4's real `QtapCustomToolSchema` at that pin over the
 * row's own bytes (a scratch probe; the rows are the artifact). The causes are
 * v4 `a53024d3c`'s any-of unions (a group that fails HARD now reports under a
 * union whose not-an-array branch is dropped, so its path splits at the
 * clause: `availableWhen: metadata.…`), the two "must test something" refines
 * naming the Inventory subjects, a record key rendered by its key schema's own
 * reason, and `e1872c94f`'s `$inventory` arm joining every operand union.
 *
 * One finding the capture surfaced, recorded rather than fixed: v4 phrases a
 * non-object `z.record` as `expected record`, and the THREE pre-existing
 * `z.record` sites in this module (`when.params`, `when.metadata`, top-level
 * `parameters`) say `expected object` — as does the Rust port at the same three
 * sites, so the two v5 halves agree with each other and both differ from v4. No
 * corpus row covers them. The gate's own `metadata` record follows v4.
 */

import { describe, expect, it } from 'vitest';

import { formatDefinitionIssues, safeParse } from './custom-tool-types';

interface Row {
  id: string;
  /** The definition's BYTES, exactly as they were fed to v4. */
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
}

/** Captured from v4's real Zod at `231be14c`. Do not hand-edit a message. */
const ROWS: Row[] = [
  {
    id: "gate-available-contains",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"toolAbilities\":{\"contains\":\"programmable\"}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"availableWhen\":{\"metadata\":{\"toolAbilities\":{\"contains\":\"programmable\"}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-withheld-eq-boolean",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"withheldWhen\":{\"metadata\":{\"novice\":{\"eq\":true}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"withheldWhen\":{\"metadata\":{\"novice\":{\"eq\":true}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-multi-key-anded",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"toolAbilities\":{\"contains\":\"programmable\"},\"rank\":{\"gte\":3},\"cleared\":{\"eq\":true}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"availableWhen\":{\"metadata\":{\"toolAbilities\":{\"contains\":\"programmable\"},\"rank\":{\"gte\":3},\"cleared\":{\"eq\":true}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-every-comparator",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gt\":1,\"gte\":2,\"lt\":9,\"lte\":8,\"eq\":5,\"neq\":6},\"notes\":{\"contains\":\"a\",\"ncontains\":\"b\"}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"availableWhen\":{\"metadata\":{\"rank\":{\"gt\":1,\"gte\":2,\"lt\":9,\"lte\":8,\"eq\":5,\"neq\":6},\"notes\":{\"contains\":\"a\",\"ncontains\":\"b\"}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-key-with-spaces-and-caps",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"Clearance Level\":{\"gte\":3}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"availableWhen\":{\"metadata\":{\"Clearance Level\":{\"gte\":3}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-eq-string-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"withheldWhen\":{\"metadata\":{\"house\":{\"eq\":\"Corvid\"}}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"withheldWhen\":{\"metadata\":{\"house\":{\"eq\":\"Corvid\"}}},\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-key-order-between-disabled-and-revealodds",
    inputJson: "{\"name\":\"reprogram\",\"revealOdds\":false,\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":3}}},\"description\":\"Rewrite the thing’s instructions.\",\"disabled\":true,\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"disabled\":true,\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":3}}},\"revealOdds\":false,\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-with-metadata-outcome-test",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":{\"metadata\":{\"rank\":{\"gte\":{\"$param\":\"bonus\"}}}},\"message\":\"high\",\"state\":\"success\"},{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":3}}},\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":3}}},\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0}},\"outcomes\":[{\"when\":{\"metadata\":{\"rank\":{\"gte\":{\"$param\":\"bonus\"}}}},\"message\":\"high\",\"state\":\"success\"},{\"when\":true,\"message\":\"done\",\"state\":\"info\"}]}",
  },
  {
    id: "gate-both-clauses",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"a\":{\"eq\":true}}},\"withheldWhen\":{\"metadata\":{\"b\":{\"eq\":true}}}}",
    success: false,
    reason: "withheldWhen: declares both availableWhen and withheldWhen — a definition gates one way or the other. Fold the second test into the first, remembering that a key the character lacks never matches.",
    data: null,
  },
  {
    id: "gate-both-clauses-one-malformed",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"a\":{\"eq\":true}}},\"withheldWhen\":{\"metadata\":{\"b\":{\"nope\":1}}}}",
    success: false,
    reason: "withheldWhen.metadata.b: Unrecognized key: \"nope\"; withheldWhen.metadata.b: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains); withheldWhen: declares both availableWhen and withheldWhen \u2014 a definition gates one way or the other. Fold the second test into the first, remembering that a key the character lacks never matches.",
    data: null,
  },
  {
    id: "gate-param-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"parameters\":{\"level\":{\"type\":\"number\",\"default\":1}},\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":{\"$param\":\"level\"}}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.gte: Invalid input: expected number, received object",
    data: null,
  },
  {
    id: "gate-state-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":{\"$state\":\"player.rank\",\"fallback\":2}}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.gte: Invalid input: expected number, received object",
    data: null,
  },
  {
    id: "gate-empty-metadata",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{}}}",
    success: false,
    reason: "availableWhen: must test at least one metadata key, progress field, inventory object, or inventory aggregate",
    data: null,
  },
  {
    id: "gate-empty-comparator",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"a\":{}}}}",
    success: false,
    reason: "availableWhen.metadata.a: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)",
    data: null,
  },
  {
    id: "gate-unknown-subject-params",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"params\":{\"a\":{\"eq\":1}}}}",
    success: false,
    reason: "availableWhen: Unrecognized key: \"params\"; availableWhen: must test at least one metadata key, progress field, inventory object, or inventory aggregate",
    data: null,
  },
  {
    id: "gate-extra-subject-key",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"a\":{\"eq\":1}},\"roll\":{\"gte\":1}}}",
    success: false,
    reason: "availableWhen: Unrecognized key: \"roll\"",
    data: null,
  },
  {
    id: "gate-empty-contains",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"notes\":{\"contains\":\"\"}}}}",
    success: false,
    reason: "availableWhen.metadata.notes.contains: the substring to look for must not be empty",
    data: null,
  },
  {
    id: "gate-empty-ncontains",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"withheldWhen\":{\"metadata\":{\"notes\":{\"ncontains\":\"\"}}}}",
    success: false,
    reason: "withheldWhen.metadata.notes.ncontains: the substring to look for must not be empty",
    data: null,
  },
  {
    id: "gate-unknown-comparator",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"notes\":{\"startsWith\":\"a\"}}}}",
    success: false,
    reason: "availableWhen.metadata.notes: Unrecognized key: \"startsWith\"; availableWhen.metadata.notes: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)",
    data: null,
  },
  {
    id: "gate-empty-metadata-key",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"\":{\"eq\":1}}}}",
    success: false,
    reason: "availableWhen: metadata.: Too small: expected string to have >=1 characters",
    data: null,
  },
  {
    id: "gate-ordering-string-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":\"3\"}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.gte: Invalid input: expected number, received string",
    data: null,
  },
  {
    id: "gate-ordering-boolean-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gt\":true}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.gt: Invalid input: expected number, received boolean",
    data: null,
  },
  {
    id: "gate-eq-array-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"eq\":[1,2]}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.eq: Invalid input: expected number, received array — or — Invalid input: expected string, received array — or — Invalid input: expected boolean, received array",
    data: null,
  },
  {
    id: "gate-eq-null-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"eq\":null}}}}",
    success: false,
    reason: "availableWhen: metadata.rank.eq: Invalid input: expected number, received null — or — Invalid input: expected string, received null — or — Invalid input: expected boolean, received null",
    data: null,
  },
  {
    id: "gate-contains-number-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"notes\":{\"contains\":7}}}}",
    success: false,
    reason: "availableWhen: metadata.notes.contains: Invalid input: expected string, received number",
    data: null,
  },
  {
    id: "gate-not-an-object",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":true}",
    success: false,
    reason: "availableWhen: Invalid input: expected object, received boolean",
    data: null,
  },
  {
    id: "gate-metadata-not-an-object",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":\"rank\"}}",
    success: false,
    reason: "availableWhen: metadata: Invalid input: expected record, received string",
    data: null,
  },
  {
    id: "gate-comparator-not-an-object",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":3}}}",
    success: false,
    reason: "availableWhen: metadata.rank: Invalid input: expected object, received number",
    data: null,
  },
];

describe('custom-tool schema — the availability gate, against v4 231be14c', () => {
  it('the row set covers both verdicts (a truncated capture must not pass silently)', () => {
    expect(ROWS.filter((r) => r.success).length).toBeGreaterThan(0);
    expect(ROWS.filter((r) => !r.success).length).toBeGreaterThan(0);
    expect(new Set(ROWS.map((r) => r.id)).size).toBe(ROWS.length);
  });

  for (const row of ROWS) {
    it(`matches v4 — ${row.id}`, () => {
      const result = safeParse(JSON.parse(row.inputJson));

      if (result.success) {
        expect(row.success, `v5 accepted; v4 rejected with: ${row.reason}`).toBe(true);
        expect(JSON.stringify(result.data)).toBe(row.data);
      } else {
        const sentence = formatDefinitionIssues(result.issues);
        expect(row.success, `v5 rejected with: ${sentence}; v4 accepted`).toBe(false);
        expect(sentence).toBe(row.reason);
      }
    });
  }
});

/**
 * The any-of grammar (v4 `a53024d3c`, at the `322a5488e` pin) — one `it` per
 * new sentence or rendering rule. Every expected string is v4's REAL output for
 * the same bytes: each is the `reason` of the committed corpus row named in the
 * test title (`pascal-custom-tool-definition.oracle.ndjson`), which the corpus
 * spec also replays whole.
 */
describe('custom-tool schema — any-of gates and conditions, against v4 322a5488e', () => {
  const probe = (extra: Record<string, unknown>, outcomes?: unknown[]): string | null => {
    const result = safeParse({
      name: 'probe',
      description: 'A probe.',
      outcomes: outcomes ?? [{ when: true, message: '-', state: 'info' }],
      ...extra,
    });
    return result.success ? null : formatDefinitionIssues(result.issues);
  };
  const row = (when: unknown) => [
    { when, message: '-', state: 'info' },
    { when: true, message: '-', state: 'info' },
  ];

  it('accepts an array gate, an array outcome `when`, and an array effect `when` (anyof-*-two-groups)', () => {
    expect(probe({ availableWhen: [{ metadata: { a: { eq: true } } }, { metadata: { b: { eq: true } } }] })).toBeNull();
    expect(probe({}, row([{ gte: 0.9 }, { metadata: { lucky: { eq: true } } }]))).toBeNull();
    expect(
      probe({
        effects: [
          { when: [{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }], target: 'state.hits', value: 1 },
        ],
      }),
    ).toBeNull();
  });

  it('names every Inventory subject in the gate group refine (anyof-gate-bare-empty-object-unburied)', () => {
    expect(probe({ availableWhen: {} })).toBe(
      'availableWhen: must test at least one metadata key, progress field, inventory object, or inventory aggregate',
    );
  });

  it('names every Inventory subject in the `when` refine, at the group index (anyof-outcome-empty-group)', () => {
    expect(probe({}, row([{ gte: 0.9 }, {}]))).toBe(
      'outcomes.0.when.1: must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`',
    );
  });

  it('names `outcome` and every Inventory subject in the effect `when` refine', () => {
    expect(probe({ effects: [{ when: {}, target: 'state.hits', value: 1 }] })).toBe(
      'effects.0.when: must test something: a comparator on the value, `roll`, `llm`, `outcome`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`',
    );
  });

  it('refuses an empty any-of array with Zod\'s own continuable min (anyof-gate-empty, anyof-outcome-empty)', () => {
    expect(probe({ withheldWhen: [] })).toBe('withheldWhen: Too small: expected array to have >=1 items');
    expect(probe({}, row([]))).toBe('outcomes.0.when: Too small: expected array to have >=1 items');
  });

  it('refuses a ninth group with ONLY the author\'s cap, hard, before any element parses (anyof-*-past-the-cap*)', () => {
    const nine = Array.from({ length: 9 }, (_, k) => ({ metadata: { [`k${k}`]: { eq: 1 } } }));
    expect(probe({ availableWhen: nine })).toBe(
      'availableWhen: Invalid input: expected object, received array — or — at most 8 groups',
    );
    // The ninth group is malformed too, and is never read.
    const eightAndBad = [...Array.from({ length: 8 }, (_, k) => ({ gte: k })), {}];
    expect(probe({}, row(eightAndBad))).toBe(
      'outcomes.0.when: Invalid input: expected true — or — Invalid input: expected object, received array — or — at most 8 groups',
    );
  });

  it('drops the not-an-array branch from a union rendering (anyof-gate-scalar, anyof-outcome-scalar-when)', () => {
    expect(probe({ availableWhen: 5 })).toBe('availableWhen: Invalid input: expected object, received number');
    expect(probe({}, row(5))).toBe(
      'outcomes.0.when: Invalid input: expected true — or — Invalid input: expected object, received number',
    );
  });

  it('keeps the array branch when a string also fails its length checks (anyof-*-long-string*, anyof-outcome-empty-string-when)', () => {
    expect(probe({ availableWhen: 'abcdefghij' })).toBe(
      'availableWhen: Invalid input: expected object, received string — or — Invalid input: expected array, received string; at most 8 groups',
    );
    expect(probe({}, row(''))).toBe(
      'outcomes.0.when: Invalid input: expected true — or — Invalid input: expected object, received string — or — Invalid input: expected array, received string; Too small: expected string to have >=1 characters',
    );
  });

  it('indexes a cross-field path by group ONLY when the clause is an array (anyof-*-ghost-param-*)', () => {
    expect(probe({}, row([{ gte: 0.9 }, { params: { ghost: { eq: 1 } } }]))).toBe(
      'outcomes.0.when.1.params.ghost: tests undeclared parameter "ghost"',
    );
    expect(probe({}, row({ params: { ghost: { eq: 1 } } }))).toBe(
      'outcomes.0.when.params.ghost: tests undeclared parameter "ghost"',
    );
    expect(
      probe({ effects: [{ when: [{ outcome: { eq: 'success' } }, { params: { ghost: { eq: 1 } } }], target: 'state.hits', value: 1 }] }),
    ).toBe('effects.0.when.1.params.ghost: tests undeclared parameter "ghost"');
  });

  it('renders a refused record key by the key schema\'s own reason (anyof-gate-progress-bad-key-in-group-1)', () => {
    expect(probe({ withheldWhen: [{ metadata: { a: { eq: 1 } } }, { progress: { cannon: { eq: true } } }] })).toBe(
      'withheldWhen: Invalid input: expected object, received array — or — 1.progress.cannon: "cannon" names no field — write "<progression id>.<field>", e.g. "cannon.complete"',
    );
    expect(probe({ availableWhen: { metadata: { '': { eq: 1 } } } })).toBe(
      'availableWhen: metadata.: Too small: expected string to have >=1 characters',
    );
  });

  it('never reads an array `when` as a catch-all (anyof-outcome-is-never-a-catch-all)', () => {
    expect(probe({}, [{ when: [{ gte: 0.5 }], message: '-', state: 'info' }])).toBe(
      'outcomes.0.when: the final outcome must be a catch-all (when: true) so every roll lands somewhere',
    );
  });
});
