/**
 * The availability-gate arms of the browser schema twin (v4 `6864bf0e`, "custom
 * tools can be gated on the invoking character's metadata"), pinned against
 * v4's REAL Zod at `231be14c`.
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
    reason: "withheldWhen.metadata.b: Unrecognized key: \"nope\"",
    data: null,
  },
  {
    id: "gate-param-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"parameters\":{\"level\":{\"type\":\"number\",\"default\":1}},\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":{\"$param\":\"level\"}}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.gte: Invalid input: expected number, received object",
    data: null,
  },
  {
    id: "gate-state-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":{\"$state\":\"player.rank\",\"fallback\":2}}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.gte: Invalid input: expected number, received object",
    data: null,
  },
  {
    id: "gate-empty-metadata",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{}}}",
    success: false,
    reason: "availableWhen.metadata: must test at least one metadata key",
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
    reason: "availableWhen.metadata: Invalid input: expected record, received undefined; availableWhen: Unrecognized key: \"params\"",
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
    reason: "availableWhen.metadata.notes: Unrecognized key: \"startsWith\"",
    data: null,
  },
  {
    id: "gate-empty-metadata-key",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"\":{\"eq\":1}}}}",
    success: false,
    reason: "availableWhen.metadata.: Invalid key in record",
    data: null,
  },
  {
    id: "gate-ordering-string-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gte\":\"3\"}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.gte: Invalid input: expected number, received string",
    data: null,
  },
  {
    id: "gate-ordering-boolean-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"gt\":true}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.gt: Invalid input: expected number, received boolean",
    data: null,
  },
  {
    id: "gate-eq-array-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"eq\":[1,2]}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.eq: Invalid input: expected number, received array — or — Invalid input: expected string, received array — or — Invalid input: expected boolean, received array",
    data: null,
  },
  {
    id: "gate-eq-null-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":{\"eq\":null}}}}",
    success: false,
    reason: "availableWhen.metadata.rank.eq: Invalid input: expected number, received null — or — Invalid input: expected string, received null — or — Invalid input: expected boolean, received null",
    data: null,
  },
  {
    id: "gate-contains-number-operand",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"notes\":{\"contains\":7}}}}",
    success: false,
    reason: "availableWhen.metadata.notes.contains: Invalid input: expected string, received number",
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
    reason: "availableWhen.metadata: Invalid input: expected record, received string",
    data: null,
  },
  {
    id: "gate-comparator-not-an-object",
    inputJson: "{\"name\":\"reprogram\",\"description\":\"Rewrite the thing’s instructions.\",\"outcomes\":[{\"when\":true,\"message\":\"done\",\"state\":\"info\"}],\"availableWhen\":{\"metadata\":{\"rank\":3}}}",
    success: false,
    reason: "availableWhen.metadata.rank: Invalid input: expected object, received number",
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
