/**
 * The `$state` reference arms of the browser schema twin (v4 `f48f34dc`,
 * "cascading state"), pinned against v4's REAL Zod at `c53510c7`.
 *
 * The committed corpus NDJSON is lane D10's to regenerate (§C of the P4.6be
 * work order), so this file is the same-teeth stand-in: every row below was
 * CAPTURED by driving v4's own `QtapCustomToolSchema.safeParse` +
 * `formatDefinitionIssues` over the input bytes, at `c53510c7`, and is
 * byte-compared here exactly as `custom-tool-types.corpus.spec.ts` compares the
 * corpus. It proves the SPA schema port accepts/rejects `$state` references the
 * way the server's Zod does, down to the rejection sentence.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in ~/source/quilltap-server @ c53510c7, `npx tsx <script>` with Node 24:
 * import { QtapCustomToolSchema, formatDefinitionIssues } from '@/lib/pascal/custom-tool.types'
 * const r = QtapCustomToolSchema.safeParse(JSON.parse(inputJson))
 * ({ success: r.success,
 *    reason: r.success ? null : formatDefinitionIssues(r.error),
 *    data: r.success ? JSON.stringify(r.data) : null })
 * ```
 *
 * v4 anchors: `lib/pascal/custom-tool.types.ts` — `StateRefSchema`,
 * `isStateRef`, the parameter-default fallback typing, `validateRollRefs`'s
 * `$state` arm, and `resolveOperandType`'s `$state` arm.
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

/** Captured from v4's real Zod at `c53510c7`. Do not hand-edit a message. */
const ROWS: Row[] = [

  {
    id: "state-roll-field-numeric",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":0},\"max\":{\"$state\":\"game.high\",\"fallback\":6}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":0},\"max\":{\"$state\":\"game.high\",\"fallback\":6}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-operand-and-param-default",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"threshold\":{\"type\":\"number\",\"default\":{\"$state\":\"game.threshold\",\"fallback\":5}}},\"outcomes\":[{\"when\":{\"gte\":{\"$state\":\"game.difficulty\",\"fallback\":3}},\"message\":\"hard\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"threshold\":{\"type\":\"number\",\"default\":{\"$state\":\"game.threshold\",\"fallback\":5}}},\"outcomes\":[{\"when\":{\"gte\":{\"$state\":\"game.difficulty\",\"fallback\":3}},\"message\":\"hard\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-missing-fallback",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\"}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll: Invalid input: expected string, received object — or — min: Invalid input: expected number, received object — or — $param: Invalid input: expected string, received undefined; Unrecognized key: \"$state\" — or — fallback: Invalid input: expected number, received undefined — or — Invalid input: expected string, received undefined — or — Invalid input: expected boolean, received undefined",
    data: null,
  },
  {
    id: "state-roll-fallback-not-number",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":\"nope\"}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll.min: roll.min uses a $state reference whose fallback is string rather than a number",
    data: null,
  },
  {
    id: "state-param-default-type-mismatch",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"count\":{\"type\":\"integer\",\"default\":{\"$state\":\"game.count\",\"fallback\":\"x\"}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "parameters.count.default: default must be a number for type integer",
    data: null,
  },
  {
    id: "state-extra-key",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":0,\"extra\":1}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll.min: Unrecognized key: \"extra\"",
    data: null,
  },
  {
    id: "state-empty-path",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"\",\"fallback\":0}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll.min.$state: Too small: expected string to have >=1 characters",
    data: null,
  },
  {
    id: "state-string-operand-contains",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$state\":\"clue.needle\",\"fallback\":\"ras\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$state\":\"clue.needle\",\"fallback\":\"ras\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-eq-string-operand",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"eq\":{\"$state\":\"clue.material\",\"fallback\":\"brass\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"eq\":{\"$state\":\"clue.material\",\"fallback\":\"brass\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-string-default",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"label\":{\"type\":\"string\",\"default\":{\"$state\":\"ui.label\",\"fallback\":\"ready\"}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"label\":{\"type\":\"string\",\"default\":{\"$state\":\"ui.label\",\"fallback\":\"ready\"}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-boolean-default",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"flag\":{\"type\":\"boolean\",\"default\":{\"$state\":\"game.flag\",\"fallback\":true}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"flag\":{\"type\":\"boolean\",\"default\":{\"$state\":\"game.flag\",\"fallback\":true}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "state-fallback-object",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":{}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll: Invalid input: expected string, received object — or — min: Invalid input: expected number, received object — or — $param: Invalid input: expected string, received undefined; Unrecognized keys: \"$state\", \"fallback\" — or — fallback: Invalid input: expected number, received object — or — Invalid input: expected string, received object — or — Invalid input: expected boolean, received object",
    data: null,
  },
  {
    id: "state-fallback-null",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":null}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll: Invalid input: expected string, received object — or — min: Invalid input: expected number, received object — or — $param: Invalid input: expected string, received undefined; Unrecognized keys: \"$state\", \"fallback\" — or — fallback: Invalid input: expected number, received null — or — Invalid input: expected string, received null — or — Invalid input: expected boolean, received null",
    data: null,
  },
  {
    id: "state-ordering-string-fallback",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"outcomes\":[{\"when\":{\"gte\":{\"$state\":\"game.diff\",\"fallback\":\"no\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.gte: gte orders the rolled value against a string, and only numbers can be ordered",
    data: null,
  },
  {
    id: "state-contains-numeric-fallback",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$state\":\"clue.n\",\"fallback\":5}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.params.material.contains: contains looks for a number inside parameter \"material\", and a substring must be a string",
    data: null,
  },
  {
    id: "state-roll-fallback-boolean",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":{\"$state\":\"game.low\",\"fallback\":true}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "roll.min: roll.min uses a $state reference whose fallback is boolean rather than a number",
    data: null,
  },
  {
    id: "state-integer-default-fractional-fallback",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"parameters\":{\"count\":{\"type\":\"integer\",\"default\":{\"$state\":\"game.count\",\"fallback\":2.5}}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "parameters.count.default: default must be a whole number for type integer",
    data: null,
  },
  {
    id: "state-eq-bare-value-type-mismatch",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"outcomes\":[{\"when\":{\"eq\":{\"$state\":\"game.tag\",\"fallback\":\"txt\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.eq: eq compares the rolled value, which is a number, with a string — this can never hold",
    data: null,
  },
  {
    id: "state-roll-and-transform",
    inputJson: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":1,\"max\":6,\"multiplier\":{\"$state\":\"game.mult\",\"fallback\":2},\"offset\":{\"$state\":\"game.off\",\"fallback\":3}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"draw\",\"description\":\"x\",\"roll\":{\"min\":1,\"max\":6,\"multiplier\":{\"$state\":\"game.mult\",\"fallback\":2},\"offset\":{\"$state\":\"game.off\",\"fallback\":3}},\"outcomes\":[{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
];

describe('custom-tool schema — the c53510c7 $state arms', () => {
  it('the row set is the expected size (a truncated table must not pass silently)', () => {
    expect(ROWS.length).toBe(19);
  });

  for (const row of ROWS) {
    it(`matches v4 — ${row.id}`, () => {
      const raw: unknown = JSON.parse(row.inputJson);
      const result = safeParse(raw);

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

