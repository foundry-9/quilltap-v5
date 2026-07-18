/**
 * The consulted-oracle + containment arms of the browser schema twin, pinned
 * against v4's REAL Zod at `616930db` (P4.6bc unit 1).
 *
 * The committed 115-row corpus NDJSON is lane D8's to regenerate (§C of the
 * work order), so it cannot carry the arms this drift added. This file is the
 * stand-in with the same teeth: every row below was CAPTURED by driving v4's
 * own `QtapCustomToolSchema.safeParse` + `formatDefinitionIssues` over the
 * input bytes, at `616930db`, and is byte-compared here exactly as
 * `custom-tool-types.corpus.spec.ts` compares the corpus.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in ~/source/quilltap-server, `npx tsx <script>` with Node 24:
 * import { QtapCustomToolSchema, formatDefinitionIssues } from '@/lib/pascal/custom-tool.types'
 * const r = QtapCustomToolSchema.safeParse(JSON.parse(inputJson))
 * ({ success: r.success,
 *    reason: r.success ? null : formatDefinitionIssues(r.error),
 *    data: r.success ? JSON.stringify(r.data) : null })
 * ```
 *
 * v4 anchors: `lib/pascal/custom-tool.types.ts` — `CustomToolLlmSchema` :420,
 * `LlmComparatorSchema` :297, `StringOperandSchema` :214, the containment arm
 * of `validateComparator` :731, the llm-without-block rejection :623.
 *
 * Two behaviors worth naming, both empirical rather than inferred:
 *
 *  - a non-integer `maxOutput` SUPPRESSES the bound checks — `100001.5` reports
 *    only `expected int, received number`, never the ceiling as well. (A
 *    mutation check confirmed the abort/continue flag on that issue is NOT
 *    observable through this surface; only the suppression is.)
 *  - the empty-substring sentence survives the operand union only because of
 *    Zod's hoist rule: `z.string().min(1, …)` raises a CONTINUABLE issue while
 *    the `$param` branch aborts on shape, so exactly one branch is non-aborted
 *    and its authored sentence replaces the bare "Invalid input".
 */

import { describe, expect, it } from 'vitest';

import { collectUnknownKeys, formatDefinitionIssues, safeParse } from './custom-tool-types';

interface Row {
  id: string;
  /** The definition's BYTES, exactly as they were fed to v4. */
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
}

/** Captured from v4's real Zod at `616930db`. Do not hand-edit a message. */
const ROWS: Row[] = [
  {
    id: "llm-block-ok",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}]}",
  },
  {
    id: "llm-block-no-errormessage",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\"}}",
    success: false,
    reason: "llm.errorMessage: Invalid input: expected string, received undefined",
    data: null,
  },
  {
    id: "llm-block-empty-prompt",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"\",\"errorMessage\":\"Silence.\"}}",
    success: false,
    reason: "llm.prompt: Too small: expected string to have >=1 characters",
    data: null,
  },
  {
    id: "llm-block-unknown-key",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"model\":\"gpt\"}}",
    success: false,
    reason: "llm: Unrecognized key: \"model\"",
    data: null,
  },
  {
    id: "llm-block-maxoutput-ok",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":50000}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":50000},\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}]}",
  },
  {
    id: "llm-block-maxoutput-zero",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":0}}",
    success: false,
    reason: "llm.maxOutput: Too small: expected number to be >=1",
    data: null,
  },
  {
    id: "llm-block-maxoutput-negative",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":-5}}",
    success: false,
    reason: "llm.maxOutput: Too small: expected number to be >=1",
    data: null,
  },
  {
    id: "llm-block-maxoutput-fractional",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":12.5}}",
    success: false,
    reason: "llm.maxOutput: Invalid input: expected int, received number",
    data: null,
  },
  {
    id: "llm-block-maxoutput-ceiling",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"Silence.\",\"maxOutput\":100001}}",
    success: false,
    reason: "llm.maxOutput: Too big: expected number to be <=100000",
    data: null,
  },
  {
    id: "llm-block-prompt-too-long",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\",\"errorMessage\":\"Silence.\"}}",
    success: false,
    reason: "llm.prompt: Too big: expected string to have <=4000 characters",
    data: null,
  },
  {
    id: "llm-block-error-too-long",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"Speak.\",\"errorMessage\":\"yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\"}}",
    success: false,
    reason: "llm.errorMessage: Too big: expected string to have <=1000 characters",
    data: null,
  },
  {
    id: "llm-block-not-object",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":\"speak\"}",
    success: false,
    reason: "llm: Invalid input: expected object, received string",
    data: null,
  },
  {
    id: "when-llm-eq",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"YES\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"YES\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-llm-ok-only",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ok\":false}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"ok\":false}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-llm-ok-and-eq",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ok\":true,\"eq\":\"YES\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"YES\",\"ok\":true}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-llm-gte",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"gte\":5}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"gte\":5}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-llm-param-operand",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"eq\":{\"$param\":\"scale\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"eq\":{\"$param\":\"scale\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-llm-no-block",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"YES\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.llm: tests the LLM consult, but the tool declares no `llm` block",
    data: null,
  },
  {
    id: "when-llm-empty",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: false,
    reason: "outcomes.0.when.llm: must test something: a comparator on the answer, or `ok`",
    data: null,
  },
  {
    id: "when-llm-undeclared-param",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"eq\":{\"$param\":\"ghost\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: false,
    reason: "outcomes.0.when.llm.eq: references undeclared parameter \"ghost\"",
    data: null,
  },
  {
    id: "when-llm-misspelled",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"a\",\"gt3\":1}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 llm: Unrecognized key: \"gt3\"",
    data: null,
  },
  {
    id: "when-llm-ok-not-boolean",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ok\":\"yes\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 llm.ok: Invalid input: expected boolean, received string",
    data: null,
  },
  {
    id: "when-only-llm-refine",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ok\":true}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"ok\":true}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "when-empty-object",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when: must test something: a comparator on the value, `roll`, `llm`, a non-empty `params`, or a non-empty `metadata`",
    data: null,
  },
  {
    id: "contains-string-param",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":\"ras\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":\"ras\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "ncontains-string-param",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"ncontains\":\"iron\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"ncontains\":\"iron\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-param-ref",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"cargo\":{\"contains\":{\"$param\":\"sought\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"cargo\":{\"type\":\"string\",\"default\":\"silk\"},\"sought\":{\"type\":\"string\",\"default\":\"opium\"}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"cargo\":{\"type\":\"string\",\"default\":\"silk\"},\"sought\":{\"type\":\"string\",\"default\":\"opium\"}},\"outcomes\":[{\"when\":{\"params\":{\"cargo\":{\"contains\":{\"$param\":\"sought\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-metadata",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"metadata\":{\"faction\":{\"contains\":\"Aurum\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"metadata\":{\"faction\":{\"contains\":\"Aurum\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-llm",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"contains\":\"west door\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"contains\":\"west door\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-numeric-param",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"scale\":{\"contains\":\"1\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}}}",
    success: false,
    reason: "outcomes.0.when.params.scale.contains: contains searches parameter \"scale\", which is a number \u2014 only a string can contain a substring",
    data: null,
  },
  {
    id: "contains-bare-value",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"contains\":\"x\"},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 Unrecognized key: \"contains\"",
    data: null,
  },
  {
    id: "contains-raw-roll",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"roll\":{\"contains\":\"x\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 roll: Unrecognized key: \"contains\"",
    data: null,
  },
  {
    id: "contains-empty-substring",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":\"\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: false,
    reason: "outcomes.0.when.params.material.contains: the substring to look for must not be empty",
    data: null,
  },
  {
    id: "contains-number-literal",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":42}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 params.material.contains: Invalid input: expected string, received number \u2014 or \u2014 Invalid input: expected object, received number",
    data: null,
  },
  {
    id: "contains-param-ref-numeric",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$param\":\"scale\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"},\"scale\":{\"type\":\"number\",\"default\":1}}}",
    success: false,
    reason: "outcomes.0.when.params.material.contains: contains looks for a number inside parameter \"material\", and a substring must be a string",
    data: null,
  },
  {
    id: "contains-param-ref-ghost",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$param\":\"ghost\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: false,
    reason: "outcomes.0.when.params.material.contains: references undeclared parameter \"ghost\"",
    data: null,
  },
  {
    id: "contains-boolean-literal",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":true}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: false,
    reason: "outcomes.0.when: Invalid input: expected true \u2014 or \u2014 params.material.contains: Invalid input: expected string, received boolean \u2014 or \u2014 Invalid input: expected object, received boolean",
    data: null,
  },
  {
    id: "contains-empty-metadata",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"metadata\":{\"faction\":{\"contains\":\"\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.metadata.faction.contains: the substring to look for must not be empty",
    data: null,
  },
  {
    id: "ncontains-llm-empty",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ncontains\":\"\"}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: false,
    reason: "outcomes.0.when.llm.ncontains: the substring to look for must not be empty",
    data: null,
  },
  {
    id: "param-comparator-empty",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: false,
    reason: "outcomes.0.when.params.material: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)",
    data: null,
  },
  {
    id: "metadata-comparator-empty",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"metadata\":{\"faction\":{}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
    success: false,
    reason: "outcomes.0.when.metadata.faction: must specify at least one comparator (gt, gte, lt, lte, eq, neq, contains, ncontains)",
    data: null,
  },
  {
    id: "contains-llm-param-ref",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"contains\":{\"$param\":\"material\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"contains\":{\"$param\":\"material\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "llm-block-maxoutput-fractional-and-over-ceiling",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"S.\",\"errorMessage\":\"x\",\"maxOutput\":100001.5}}",
    success: false,
    reason: "llm.maxOutput: Invalid input: expected int, received number",
    data: null,
  },
  {
    id: "llm-block-maxoutput-string",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"S.\",\"errorMessage\":\"x\",\"maxOutput\":\"lots\"}}",
    success: false,
    reason: "llm.maxOutput: Invalid input: expected number, received string",
    data: null,
  },
  {
    id: "llm-block-maxoutput-null",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"S.\",\"errorMessage\":\"x\",\"maxOutput\":null}}",
    success: false,
    reason: "llm.maxOutput: Invalid input: expected number, received null",
    data: null,
  },
  {
    id: "llm-block-empty-prompt-and-unknown-key",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"prompt\":\"\",\"errorMessage\":\"x\",\"zz\":1}}",
    success: false,
    reason: "llm.prompt: Too small: expected string to have >=1 characters; llm: Unrecognized key: \"zz\"",
    data: null,
  },
  {
    id: "llm-block-missing-both",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{}}",
    success: false,
    reason: "llm.prompt: Invalid input: expected string, received undefined; llm.errorMessage: Invalid input: expected string, received undefined",
    data: null,
  },
  {
    id: "llm-block-null",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":null}",
    success: false,
    reason: "llm: Invalid input: expected object, received null",
    data: null,
  },
  {
    id: "llm-block-key-order",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}],\"llm\":{\"maxOutput\":20,\"errorMessage\":\"x\",\"prompt\":\"S.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"S.\",\"errorMessage\":\"x\",\"maxOutput\":20},\"outcomes\":[{\"when\":true,\"message\":\"ok\",\"state\":\"success\"}]}",
  },
  {
    id: "when-llm-key-order",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"llm\":{\"ncontains\":\"b\",\"ok\":true,\"contains\":\"a\",\"eq\":\"c\"}},\"message\":\"h\",\"state\":\"info\"},{\"when\":true,\"message\":\"f\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"outcomes\":[{\"when\":{\"llm\":{\"eq\":\"c\",\"contains\":\"a\",\"ncontains\":\"b\",\"ok\":true}},\"message\":\"h\",\"state\":\"info\"},{\"when\":true,\"message\":\"f\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-key-order",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"ncontains\":\"b\",\"contains\":\"a\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}}}",
    success: true,
    reason: null,
    data: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":\"a\",\"ncontains\":\"b\"}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}]}",
  },
  {
    id: "contains-ghost-and-numeric",
    inputJson: "{\"name\":\"test_tool\",\"description\":\"A tool.\",\"outcomes\":[{\"when\":{\"params\":{\"material\":{\"contains\":{\"$param\":\"ghost\"},\"ncontains\":{\"$param\":\"scale\"}}}},\"message\":\"hit\",\"state\":\"success\"},{\"when\":true,\"message\":\"fall\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"},\"scale\":{\"type\":\"number\",\"default\":1}}}",
    success: false,
    reason: "outcomes.0.when.params.material.contains: references undeclared parameter \"ghost\"; outcomes.0.when.params.material.ncontains: ncontains looks for a number inside parameter \"material\", and a substring must be a string",
    data: null,
  },];

describe('custom-tool schema — the 616930db consult + containment arms', () => {
  it('the row set is the expected size (a truncated table must not pass silently)', () => {
    expect(ROWS.length).toBe(52);
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

  it('`llm` is a known top-level key, so it is never reported as unknown', () => {
    // v4 `KNOWN_TOP_LEVEL_KEYS` :832 — the discovery log must not call the
    // consult block a forward-compatibility surprise.
    expect(collectUnknownKeys({ name: 'probe', llm: {}, outcomes: [] })).toEqual([]);
  });
});
