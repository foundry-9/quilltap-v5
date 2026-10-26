/**
 * The chipLabel + side-effects arms of the browser schema twin (v4
 * `c4d4b0de`, "custom-tool chip labels, two-block bubbles, and side effects"),
 * pinned against v4's REAL Zod at that commit (P4.D36 unit 2).
 *
 * ⚠ **Re-measured at the `322a5488e` pin (P4.D333).** Five rows moved, and the both-or-neither test reads the four-kind sentences — every replacement
 * re-captured by driving v4's real `QtapCustomToolSchema` at that pin over the
 * row's own bytes (a scratch probe; the rows are the artifact). The causes are
 * v4 `a53024d3c`'s any-of unions (a group that fails HARD now reports under a
 * union whose not-an-array branch is dropped, so its path splits at the
 * clause: `availableWhen: metadata.…`), the two "must test something" refines
 * naming the Inventory subjects, a record key rendered by its key schema's own
 * reason, and `e1872c94f`'s `$inventory` arm joining every operand union.
 *
 * ⚠ **Re-measured at the `25b8fab6a` pin (P4.D307).** Three rows moved BY
 * DESIGN, each re-captured through v4's real schema there: `effect-target-missing`
 * and `effect-value-missing` now read v4 `f0db8cbb8`'s refine sentence `a write
 * needs both target and value` (the write halves became optional when the
 * wardrobe gesture joined the effect), and `effects-past-cap-and-broken` reads
 * Zod 4.6.7's lone cap sentence (see the third bullet below).
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
 * The committed corpus NDJSON is P4.D35's to regenerate (§C of the round's
 * shared contract), so this file is the stand-in with identical rigour — the
 * same shape `custom-tool-types.state.spec.ts`, `.llm.spec.ts` and
 * `.gate.spec.ts` use. Every row below was CAPTURED by driving v4's own
 * `QtapCustomToolSchema.safeParse` + `formatDefinitionIssues` +
 * `collectUnknownKeys` over the input bytes, and is byte-compared here exactly
 * as the corpus spec compares the corpus: the verdict, the parsed data (via
 * `JSON.stringify`, so key ORDER is pinned — `chipLabel` lands between `title`
 * and `description`, `effects` between `llm` and `outcomes`, whatever order
 * the file wrote them in), the unknown-key report, and the FULL rejection
 * sentence.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in ~/source/quilltap-server @ c4d4b0de, `npx tsx <script>` with Node 24:
 * import { QtapCustomToolSchema, formatDefinitionIssues, collectUnknownKeys,
 *          parseEffectTarget } from '@/lib/pascal/custom-tool.types'
 * const r = QtapCustomToolSchema.safeParse(JSON.parse(inputJson))
 * ({ success: r.success,
 *    reason: r.success ? null : formatDefinitionIssues(r.error),
 *    data: r.success ? JSON.stringify(r.data) : null,
 *    unknownKeys: collectUnknownKeys(JSON.parse(inputJson)) })
 * ```
 *
 * v4 anchors: `lib/pascal/custom-tool.types.ts` — `EffectWhenSchema`,
 * `CustomToolEffectSchema`, `parseEffectTarget`, the `chipLabel` key,
 * `validateWhenSubjects` (the shared walk outcome rows now share with effect
 * conditions), and `validateEffects`.
 *
 * Three behaviours worth naming, all empirical rather than inferred:
 *
 *  - an empty `target` collects BOTH sentences — Zod's `Too small` (a check, so
 *    continuable) and then `target must start with "state.", "metadata." or "progress."`
 *    from the superRefine, which still runs because nothing aborted. The same
 *    doubling happens for an empty or over-long `value`.
 *  - `effects: at most 16 effects` USED to fire alongside per-effect
 *    superRefine complaints (a bad target is not a schema-shape failure). Under
 *    Zod 4.6.7 (`capSize`, the `25b8fab6a` pin) an oversized list reports ONLY
 *    its cap, aborting, before any element is parsed — so
 *    `effects-past-cap-and-broken` now carries that one sentence.
 *  - the `value` union reports all three branch failures joined by ` — or — `
 *    for a null/object/array value, but an EMPTY string hoists the string
 *    branch's own `Too small` alone (exactly one non-aborted branch).
 */

import { describe, expect, it } from 'vitest';

import * as types from './custom-tool-types';
import {
  collectUnknownKeys,
  formatDefinitionIssues,
  parseEffectTarget,
  safeParse,
} from './custom-tool-types';

/** One captured `QtapCustomToolSchema.safeParse` verdict. */
interface DefinitionRow {
  kind: 'definition';
  id: string;
  /** The definition's BYTES — never a re-stringified structure. */
  inputJson: string;
  success: boolean;
  reason: string | null;
  data: string | null;
  unknownKeys: string[];
}

/** One captured `parseEffectTarget` verdict. */
interface TargetRow {
  kind: 'target';
  id: string;
  target: string;
  ok: boolean;
  /** The parsed target, SERIALIZED, so `path` order and element types are pinned. */
  parsed: string | null;
  reason: string | null;
}

type Row = DefinitionRow | TargetRow;

const CAPTURED: readonly Row[] = [
  {"kind": "definition", "id": "chip-label-templated", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":\"Agent lambda — {{params.label}}\"}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"chipLabel\":\"Agent lambda — {{params.label}}\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-at-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"chipLabel\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-empty", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":\"\"}", "success": false, "reason": "chipLabel: Too small: expected string to have >=1 characters", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-over-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\"}", "success": false, "reason": "chipLabel: Too big: expected string to have <=160 characters", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-not-a-string", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":42}", "success": false, "reason": "chipLabel: Invalid input: expected string, received number", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-unknown-placeholder", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"chipLabel\":\"{{params.never_declared}}\"}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"chipLabel\":\"{{params.never_declared}}\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "chip-label-with-title", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"title\":\"Probe the Thing\",\"chipLabel\":\"Probe — {{value}}\"}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"title\":\"Probe the Thing\",\"chipLabel\":\"Probe — {{value}}\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-literal-number", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.encounter.count\",\"value\":3}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.encounter.count\",\"value\":3}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-literal-boolean", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.lockBroken\",\"value\":true}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"metadata.lockBroken\",\"value\":true}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-quoted-prose", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.lockpick\",\"value\":\"'broken pick'\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"metadata.lockpick\",\"value\":\"'broken pick'\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-expression-over-subjects", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"effects\":[{\"target\":\"state.tally\",\"value\":\"{{state.tally}} + {{params.scale}}\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"effects\":[{\"target\":\"state.tally\",\"value\":\"{{state.tally}} + {{params.scale}}\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-eq", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{\"eq\":\"success\"}},\"target\":\"state.wins\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"outcome\":{\"eq\":\"success\"}},\"target\":\"state.wins\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-neq", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{\"neq\":\"failure\"}},\"target\":\"state.wins\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"outcome\":{\"neq\":\"failure\"}},\"target\":\"state.wins\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-both", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{\"eq\":\"success\",\"neq\":\"failure\"}},\"target\":\"state.wins\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"outcome\":{\"eq\":\"success\",\"neq\":\"failure\"}},\"target\":\"state.wins\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-metadata", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"metadata\":{\"hasKey\":{\"eq\":true}}},\"target\":\"state.opened\",\"value\":true}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"metadata\":{\"hasKey\":{\"eq\":true}}},\"target\":\"state.opened\",\"value\":true}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-value-and-outcome", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"gt\":0.5,\"outcome\":{\"neq\":\"failure\"}},\"target\":\"state.x\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"gt\":0.5,\"outcome\":{\"neq\":\"failure\"}},\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-roll", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"roll\":{\"gte\":15}},\"target\":\"state.x\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"when\":{\"roll\":{\"gte\":15}},\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-params", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"effects\":[{\"when\":{\"params\":{\"scale\":{\"gt\":1}}},\"target\":\"state.x\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"effects\":[{\"when\":{\"params\":{\"scale\":{\"gt\":1}}},\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-llm", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"llm\":{\"ok\":true}},\"target\":\"state.x\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"llm\":{\"ok\":true}},\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-every-subject", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"gte\":0.5,\"roll\":{\"gt\":1},\"params\":{\"scale\":{\"eq\":2}},\"metadata\":{\"house\":{\"eq\":\"Aurum\"}},\"llm\":{\"ok\":true},\"outcome\":{\"eq\":\"success\"}},\"target\":\"state.x\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"scale\":{\"type\":\"number\",\"default\":1}},\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"gte\":0.5,\"roll\":{\"gt\":1},\"params\":{\"scale\":{\"eq\":2}},\"metadata\":{\"house\":{\"eq\":\"Aurum\"}},\"llm\":{\"ok\":true},\"outcome\":{\"eq\":\"success\"}},\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-subjects-written-backwards", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"outcome\":{\"neq\":\"failure\"},\"llm\":{\"ok\":false},\"metadata\":{\"a\":{\"eq\":1}},\"roll\":{\"lt\":9},\"lte\":3},\"target\":\"metadata.y\",\"value\":true}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"when\":{\"lte\":3,\"roll\":{\"lt\":9},\"metadata\":{\"a\":{\"eq\":1}},\"llm\":{\"ok\":false},\"outcome\":{\"neq\":\"failure\"}},\"target\":\"metadata.y\",\"value\":true}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-llm-ref-with-block", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"target\":\"metadata.verdict\",\"value\":\"{{llm}}\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"llm\":{\"prompt\":\"Answer YES or NO: is {{value}} auspicious?\",\"errorMessage\":\"The wire went dead.\"},\"effects\":[{\"target\":\"metadata.verdict\",\"value\":\"{{llm}}\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-negative-literal", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.debt\",\"value\":-5}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.debt\",\"value\":-5}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-float-literal", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.ratio\",\"value\":0.25}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.ratio\",\"value\":0.25}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-metadata-dotted", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.ansible.tool\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"metadata.ansible.tool\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-bracket-index", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.crew[0].mood\",\"value\":\"'grim'\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.crew[0].mood\",\"value\":\"'grim'\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-underscore-deeper", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.encounter._notes\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.encounter._notes\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-at-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-at-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":\"'yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy'\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.x\",\"value\":\"'yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy'\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effects-at-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.k0\",\"value\":1},{\"target\":\"state.k1\",\"value\":1},{\"target\":\"state.k2\",\"value\":1},{\"target\":\"state.k3\",\"value\":1},{\"target\":\"state.k4\",\"value\":1},{\"target\":\"state.k5\",\"value\":1},{\"target\":\"state.k6\",\"value\":1},{\"target\":\"state.k7\",\"value\":1},{\"target\":\"state.k8\",\"value\":1},{\"target\":\"state.k9\",\"value\":1},{\"target\":\"state.k10\",\"value\":1},{\"target\":\"state.k11\",\"value\":1},{\"target\":\"state.k12\",\"value\":1},{\"target\":\"state.k13\",\"value\":1},{\"target\":\"state.k14\",\"value\":1},{\"target\":\"state.k15\",\"value\":1}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.k0\",\"value\":1},{\"target\":\"state.k1\",\"value\":1},{\"target\":\"state.k2\",\"value\":1},{\"target\":\"state.k3\",\"value\":1},{\"target\":\"state.k4\",\"value\":1},{\"target\":\"state.k5\",\"value\":1},{\"target\":\"state.k6\",\"value\":1},{\"target\":\"state.k7\",\"value\":1},{\"target\":\"state.k8\",\"value\":1},{\"target\":\"state.k9\",\"value\":1},{\"target\":\"state.k10\",\"value\":1},{\"target\":\"state.k11\",\"value\":1},{\"target\":\"state.k12\",\"value\":1},{\"target\":\"state.k13\",\"value\":1},{\"target\":\"state.k14\",\"value\":1},{\"target\":\"state.k15\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effects-empty-array", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-many-mixed", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.a\",\"value\":1},{\"when\":{\"outcome\":{\"eq\":\"failure\"}},\"target\":\"metadata.b\",\"value\":false},{\"when\":{\"gte\":0.5},\"target\":\"state.c\",\"value\":\"{{value}} + 'x'\"}]}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.a\",\"value\":1},{\"when\":{\"outcome\":{\"eq\":\"failure\"}},\"target\":\"metadata.b\",\"value\":false},{\"when\":{\"gte\":0.5},\"target\":\"state.c\",\"value\":\"{{value}} + 'x'\"}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": []},
  {"kind": "definition", "id": "effect-bare-prose", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.lockpick\",\"value\":\"broken pick\"}]}", "success": false, "reason": "effects.0.value: value is not a valid expression: \"broken\" is a bare word — there are no identifiers here. Quote literal text ('broken') or use a {{ref}}", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-bad-prefix", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"somewhere.else\",\"value\":1}]}", "success": false, "reason": "effects.0.target: target must start with \"state.\", \"metadata.\" or \"progress.\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-underscore-guard", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state._secrets.combo\",\"value\":1}]}", "success": false, "reason": "effects.0.target: target writes \"_secrets.combo\" — state keys starting with an underscore are user-only, and no tool may write them", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-state-empty", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.\",\"value\":1}]}", "success": false, "reason": "effects.0.target: target names no state path after \"state.\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-metadata-empty", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.\",\"value\":1}]}", "success": false, "reason": "effects.0.target: target names no metadata key after \"metadata.\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-empty-string", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"\",\"value\":1}]}", "success": false, "reason": "effects.0.target: Too small: expected string to have >=1 characters; effects.0.target: target must start with \"state.\", \"metadata.\" or \"progress.\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-over-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk\",\"value\":1}]}", "success": false, "reason": "effects.0.target: Too big: expected string to have <=200 characters", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-missing", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"value\":1}]}", "success": false, "reason": "effects.0.target: a write needs both target and value", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-target-not-a-string", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":7,\"value\":1}]}", "success": false, "reason": "effects.0.target: Invalid input: expected string, received number", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-missing", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\"}]}", "success": false, "reason": "effects.0.value: a write needs both target and value", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-empty-string", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":\"\"}]}", "success": false, "reason": "effects.0.value: Too small: expected string to have >=1 characters; effects.0.value: value is not a valid expression: the expression is empty", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-over-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":\"'yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy'\"}]}", "success": false, "reason": "effects.0.value: Too big: expected string to have <=500 characters; effects.0.value: value is not a valid expression: the expression exceeds 500 characters", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-null", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":null}]}", "success": false, "reason": "effects.0.value: Invalid input: expected number, received null — or — Invalid input: expected boolean, received null — or — Invalid input: expected string, received null", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-object", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":{\"a\":1}}]}", "success": false, "reason": "effects.0.value: Invalid input: expected number, received object — or — Invalid input: expected boolean, received object — or — Invalid input: expected string, received object", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-value-array", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":[1]}]}", "success": false, "reason": "effects.0.value: Invalid input: expected number, received array — or — Invalid input: expected boolean, received array — or — Invalid input: expected string, received array", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-ghost-param", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":\"{{params.ghost}} + 1\"}]}", "success": false, "reason": "effects.0.value: value references undeclared parameter \"ghost\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-llm-ref-no-block", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"metadata.verdict\",\"value\":\"{{llm}}\"}]}", "success": false, "reason": "effects.0.value: value references {{llm}}, but the tool declares no `llm` block", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-llm-no-block", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"llm\":{\"ok\":true}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when.llm: tests the LLM consult, but the tool declares no `llm` block", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-ghost-param", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"params\":{\"ghost\":{\"gt\":1}}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when.params.ghost: tests undeclared parameter \"ghost\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-empty", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: must test something: a comparator on the value, `roll`, `llm`, `outcome`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-empty", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when.outcome: must test something: `eq` or `neq` against an outcome state", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-unknown-state", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{\"eq\":\"triumph\"}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: outcome.eq: Invalid option: expected one of \"success\"|\"partial\"|\"failure\"|\"info\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-outcome-unknown-key", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"outcome\":{\"is\":\"success\"}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when.outcome: Unrecognized key: \"is\"; effects.0.when.outcome: must test something: `eq` or `neq` against an outcome state", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-unknown-key", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"nonsense\":1},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: Unrecognized key: \"nonsense\"; effects.0.when: must test something: a comparator on the value, `roll`, `llm`, `outcome`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-not-an-object", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":true,\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: Invalid input: expected object, received boolean", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-string-eq-on-value", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"eq\":\"brass\"},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: eq: Invalid input: expected number, received string — or — Invalid input: expected object, received string — or — Invalid input: expected object, received string — or — Invalid input: expected object, received string", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-contains-on-value", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"when\":{\"contains\":\"x\"},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when: Unrecognized key: \"contains\"; effects.0.when: must test something: a comparator on the value, `roll`, `llm`, `outcome`, a non-empty `params`, `metadata`, `progress`, `inventory`, `inventoryCount`, or `inventoryAggregate`", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-when-params-string-ordering", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"parameters\":{\"material\":{\"type\":\"string\",\"default\":\"brass\"}},\"effects\":[{\"when\":{\"params\":{\"material\":{\"gt\":1}}},\"target\":\"state.x\",\"value\":1}]}", "success": false, "reason": "effects.0.when.params.material.gt: gt orders parameter \"material\" against a number, and only numbers can be ordered", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-unknown-key", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":1,\"bogus\":true}]}", "success": false, "reason": "effects.0: Unrecognized key: \"bogus\"", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-not-an-object", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[1]}", "success": false, "reason": "effects.0: Invalid input: expected object, received number", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effects-not-an-array", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":{\"target\":\"state.x\",\"value\":1}}", "success": false, "reason": "effects: Invalid input: expected array, received object", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effects-past-cap", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.k0\",\"value\":1},{\"target\":\"state.k1\",\"value\":1},{\"target\":\"state.k2\",\"value\":1},{\"target\":\"state.k3\",\"value\":1},{\"target\":\"state.k4\",\"value\":1},{\"target\":\"state.k5\",\"value\":1},{\"target\":\"state.k6\",\"value\":1},{\"target\":\"state.k7\",\"value\":1},{\"target\":\"state.k8\",\"value\":1},{\"target\":\"state.k9\",\"value\":1},{\"target\":\"state.k10\",\"value\":1},{\"target\":\"state.k11\",\"value\":1},{\"target\":\"state.k12\",\"value\":1},{\"target\":\"state.k13\",\"value\":1},{\"target\":\"state.k14\",\"value\":1},{\"target\":\"state.k15\",\"value\":1},{\"target\":\"state.k16\",\"value\":1}]}", "success": false, "reason": "effects: at most 16 effects", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effects-past-cap-and-broken", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"nowhere0\",\"value\":1},{\"target\":\"nowhere1\",\"value\":1},{\"target\":\"nowhere2\",\"value\":1},{\"target\":\"nowhere3\",\"value\":1},{\"target\":\"nowhere4\",\"value\":1},{\"target\":\"nowhere5\",\"value\":1},{\"target\":\"nowhere6\",\"value\":1},{\"target\":\"nowhere7\",\"value\":1},{\"target\":\"nowhere8\",\"value\":1},{\"target\":\"nowhere9\",\"value\":1},{\"target\":\"nowhere10\",\"value\":1},{\"target\":\"nowhere11\",\"value\":1},{\"target\":\"nowhere12\",\"value\":1},{\"target\":\"nowhere13\",\"value\":1},{\"target\":\"nowhere14\",\"value\":1},{\"target\":\"nowhere15\",\"value\":1},{\"target\":\"nowhere16\",\"value\":1}]}", "success": false, "reason": "effects: at most 16 effects", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-two-broken", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"nowhere\",\"value\":1},{\"target\":\"state.x\",\"value\":\"bare prose\"}]}", "success": false, "reason": "effects.0.target: target must start with \"state.\", \"metadata.\" or \"progress.\"; effects.1.value: value is not a valid expression: \"bare\" is a bare word — there are no identifiers here. Quote literal text ('bare') or use a {{ref}}", "data": null, "unknownKeys": []},
  {"kind": "definition", "id": "effect-plus-unknown-top-level", "inputJson": "{\"name\":\"probe\",\"description\":\"A probe.\",\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}],\"effects\":[{\"target\":\"state.x\",\"value\":1}],\"persist\":true}", "success": true, "reason": null, "data": "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.x\",\"value\":1}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}", "unknownKeys": ["persist"]},
  {"kind": "target", "id": "state-path", "target": "state.encounter.count", "ok": true, "parsed": "{\"kind\":\"state\",\"path\":[\"encounter\",\"count\"],\"raw\":\"state.encounter.count\"}", "reason": null},
  {"kind": "target", "id": "state-single", "target": "state.tally", "ok": true, "parsed": "{\"kind\":\"state\",\"path\":[\"tally\"],\"raw\":\"state.tally\"}", "reason": null},
  {"kind": "target", "id": "state-bracket", "target": "state.crew[0].mood", "ok": true, "parsed": "{\"kind\":\"state\",\"path\":[\"crew\",0,\"mood\"],\"raw\":\"state.crew[0].mood\"}", "reason": null},
  {"kind": "target", "id": "state-deep-underscore", "target": "state.encounter._notes", "ok": true, "parsed": "{\"kind\":\"state\",\"path\":[\"encounter\",\"_notes\"],\"raw\":\"state.encounter._notes\"}", "reason": null},
  {"kind": "target", "id": "state-underscore-guard", "target": "state._secrets.combo", "ok": false, "parsed": null, "reason": "writes \"_secrets.combo\" — state keys starting with an underscore are user-only, and no tool may write them"},
  {"kind": "target", "id": "state-empty", "target": "state.", "ok": false, "parsed": null, "reason": "names no state path after \"state.\""},
  {"kind": "target", "id": "state-spaces", "target": "state.the key", "ok": true, "parsed": "{\"kind\":\"state\",\"path\":[\"the\",\"key\"],\"raw\":\"state.the key\"}", "reason": null},
  {"kind": "target", "id": "metadata-whole-key", "target": "metadata.ansible.tool", "ok": true, "parsed": "{\"kind\":\"metadata\",\"key\":\"ansible.tool\",\"raw\":\"metadata.ansible.tool\"}", "reason": null},
  {"kind": "target", "id": "metadata-spaces", "target": "metadata.Clearance Level", "ok": true, "parsed": "{\"kind\":\"metadata\",\"key\":\"Clearance Level\",\"raw\":\"metadata.Clearance Level\"}", "reason": null},
  {"kind": "target", "id": "metadata-empty", "target": "metadata.", "ok": false, "parsed": null, "reason": "names no metadata key after \"metadata.\""},
  {"kind": "target", "id": "no-prefix", "target": "encounter.count", "ok": false, "parsed": null, "reason": "must start with \"state.\", \"metadata.\" or \"progress.\""},
  {"kind": "target", "id": "empty", "target": "", "ok": false, "parsed": null, "reason": "must start with \"state.\", \"metadata.\" or \"progress.\""},
  {"kind": "target", "id": "state-no-dot", "target": "state", "ok": false, "parsed": null, "reason": "must start with \"state.\", \"metadata.\" or \"progress.\""},
  {"kind": "target", "id": "metadata-no-dot", "target": "metadata", "ok": false, "parsed": null, "reason": "must start with \"state.\", \"metadata.\" or \"progress.\""},];

const definitionRows = CAPTURED.filter((r): r is DefinitionRow => r.kind === 'definition');
const targetRows = CAPTURED.filter((r): r is TargetRow => r.kind === 'target');

describe('chipLabel + effects — the captured v4 rows', () => {
  it('the table is the expected shape (a truncated capture must not pass silently)', () => {
    // Composition, never a hand-written count (`harness-corpus-shape-constants-
    // rot`): the partition is total, both kinds are non-empty, both verdicts
    // appear on each, and every row carries the half the replay reads.
    expect(definitionRows.length + targetRows.length).toBe(CAPTURED.length);
    expect(definitionRows.some((r) => r.success)).toBe(true);
    expect(definitionRows.some((r) => !r.success)).toBe(true);
    expect(targetRows.some((r) => r.ok)).toBe(true);
    expect(targetRows.some((r) => !r.ok)).toBe(true);
    expect(new Set(CAPTURED.map((r) => r.id)).size).toBe(CAPTURED.length);

    for (const row of definitionRows) {
      expect(typeof row.inputJson, row.id).toBe('string');
      expect(Array.isArray(row.unknownKeys), row.id).toBe(true);
      // An accepted row must carry the parsed bytes; a rejected one the
      // sentence. Either missing would compare against `null` and pass.
      expect(typeof (row.success ? row.data : row.reason), row.id).toBe('string');
    }
    for (const row of targetRows) {
      expect(typeof (row.ok ? row.parsed : row.reason), row.id).toBe('string');
    }
  });

  describe('safeParse', () => {
    for (const row of definitionRows) {
      it(`matches v4 — ${row.id}`, () => {
        const raw: unknown = JSON.parse(row.inputJson);

        expect(collectUnknownKeys(raw)).toEqual(row.unknownKeys);

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

  describe('parseEffectTarget', () => {
    for (const row of targetRows) {
      it(`matches v4 — ${row.id}`, () => {
        const result = parseEffectTarget(row.target);
        if (result.ok) {
          expect(row.ok, `v5 parsed; v4 refused with: ${row.reason}`).toBe(true);
          expect(JSON.stringify(result.target)).toBe(row.parsed);
        } else {
          expect(row.ok, `v5 refused with: ${result.reason}; v4 parsed`).toBe(false);
          expect(result.reason).toBe(row.reason);
        }
      });
    }
  });
});

// ---------------------------------------------------------------------------
// v4 `f0db8cbb8` — "effects — wardrobe gestures" (`custom-tool-definition.
// test.ts:276-354`, pinned at `25b8fab6a`), transcribed CASE FOR CASE, plus
// the Zod half of "is deliberately weaker than Zod on wardrobe per-action
// rules" (`:791-801`). The ten JSON-Schema-mirror rows (`:725-734`) and the
// mirror half of that case read the published schema file, which the SPA
// serves rather than imports (P4.D313's byte guards cover it) — not
// transcribed, recorded by name in the P4.D307 lane record. The regexes run
// over the twin's JOINED sentence (`path: message; …`), exactly as v4's run
// over `formatDefinitionIssues`.
// ---------------------------------------------------------------------------

const WB_BASE = {
  name: 'probe',
  description: 'A probe.',
  outcomes: [{ when: true, message: '-', state: 'info' }],
};
const STR_PARAM = { parameters: { material: { type: 'string', default: 'brass' } } };
const LLM_BLOCK = {
  llm: { prompt: 'Answer YES or NO: is {{value}} auspicious?', errorMessage: 'The wire went dead.' },
};
const MAX_EFFECTS = 16;

function withEffects(effects: unknown, extra: Record<string, unknown> = {}) {
  return { ...WB_BASE, ...extra, effects };
}

function accepts(doc: unknown): boolean {
  return safeParse(doc).success;
}

function rejection(doc: unknown): string {
  const result = safeParse(doc);
  if (result.success) throw new Error('expected the definition to be rejected, but it parsed');
  return formatDefinitionIssues(result.issues);
}

describe('effects — wardrobe gestures', () => {
  const issuesOf = (doc: unknown): string => {
    const result = safeParse(doc);
    return result.success ? '' : formatDefinitionIssues(result.issues);
  };

  it('accepts each action in its own shape', () => {
    expect(
      accepts(
        withEffects([
          { wardrobe: { action: 'wear', items: ['Cloak of Night'] } },
          { wardrobe: { action: 'wear', items: ['Ribbon'], mode: 'add_to_slot', slot: 'hair' } },
          { wardrobe: { action: 'wear', items: ['Regalia'], mode: 'replace' } },
          { when: { outcome: { eq: 'failure' } }, wardrobe: { action: 'take_off', items: ['Sundress'] } },
          { wardrobe: { action: 'take_off', items: ['Sundress'], slot: 'top' } },
          { wardrobe: { action: 'clear_slot', slot: 'hair' } },
        ]),
      ),
    ).toBe(true);
  });

  it('accepts a parameter choosing the garment', () => {
    expect(
      accepts(withEffects([{ wardrobe: { action: 'wear', items: ['{{params.material}} gloves'] } }], STR_PARAM)),
    ).toBe(true);
  });

  it('tolerates an unknown placeholder family — it simply fails to resolve at the table', () => {
    expect(accepts(withEffects([{ wardrobe: { action: 'wear', items: ['{{mystery}}'] } }]))).toBe(true);
  });

  it('rejects an effect that is both a write and a gesture, or neither', () => {
    expect(
      rejection(withEffects([{ target: 'state.x', value: 1, wardrobe: { action: 'clear_slot', slot: 'hair' } }])),
    ).toMatch(/exactly one of a write \(target \+ value\), a wardrobe gesture, an inventory gesture, or a log line/);
    expect(rejection(withEffects([{}]))).toMatch(/needs target \+ value, wardrobe, inventory, or log/);
    expect(rejection(withEffects([{ target: 'state.x' }]))).toMatch(/needs both target and value/);
  });

  it('states the per-action rules at precise paths', () => {
    expect(issuesOf(withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], mode: 'add_to_slot' } }]))).toMatch(
      /effects\.0\.wardrobe\.slot.*mode add_to_slot needs a slot/,
    );
    expect(issuesOf(withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], slot: 'hair' } }]))).toMatch(
      /effects\.0\.wardrobe\.slot.*only used with mode add_to_slot/,
    );
    expect(issuesOf(withEffects([{ wardrobe: { action: 'wear' } }]))).toMatch(
      /effects\.0\.wardrobe\.items.*at least one item/,
    );
    expect(issuesOf(withEffects([{ wardrobe: { action: 'take_off', items: ['Hat'], mode: 'replace' } }]))).toMatch(
      /effects\.0\.wardrobe\.mode/,
    );
    expect(issuesOf(withEffects([{ wardrobe: { action: 'clear_slot' } }]))).toMatch(/clear_slot needs a slot/);
    expect(issuesOf(withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', items: ['Wig'] } }]))).toMatch(
      /effects\.0\.wardrobe\.items.*takes no items/,
    );
  });

  it('rejects an item naming an undeclared parameter, or {{llm}} without a consult', () => {
    expect(issuesOf(withEffects([{ wardrobe: { action: 'wear', items: ['Hat', '{{params.ghost}}'] } }]))).toMatch(
      /effects\.0\.wardrobe\.items\.1.*undeclared parameter "ghost"/,
    );
    expect(rejection(withEffects([{ wardrobe: { action: 'wear', items: ['{{llm}}'] } }]))).toMatch(
      /declares no `llm` block/,
    );
    expect(accepts(withEffects([{ wardrobe: { action: 'wear', items: ['{{llm}}'] } }], LLM_BLOCK))).toBe(true);
  });

  it('rejects an over-long item list or reference', () => {
    const nine = Array.from({ length: 9 }, (_, i) => `Item ${i}`);
    expect(accepts(withEffects([{ wardrobe: { action: 'wear', items: nine } }]))).toBe(false);
    expect(accepts(withEffects([{ wardrobe: { action: 'wear', items: ['x'.repeat(201)] } }]))).toBe(false);
  });

  it('counts gestures and writes together against the effects cap', () => {
    const many = Array.from({ length: MAX_EFFECTS + 1 }, (_, i) =>
      i % 2 ? { target: `state.k${i}`, value: 1 } : { wardrobe: { action: 'clear_slot', slot: 'hair' } },
    );
    expect(accepts(withEffects(many))).toBe(false);
  });

  it('is deliberately weaker than Zod on wardrobe per-action rules — the Zod half', () => {
    const addToSlotNoSlot = withEffects([{ wardrobe: { action: 'wear', items: ['Hat'], mode: 'add_to_slot' } }]);
    const clearSlotWithItems = withEffects([{ wardrobe: { action: 'clear_slot', slot: 'hair', items: ['Wig'] } }]);
    const ghostGarment = withEffects([{ wardrobe: { action: 'wear', items: ['{{params.ghost}}'] } }]);
    for (const doc of [addToSlotNoSlot, clearSlotWithItems, ghostGarment]) {
      expect(accepts(doc)).toBe(false);
    }
  });
});

describe('the gesture guards and the slot enum (L-B)', () => {
  const t = types as unknown as Record<string, unknown>;

  it('isWriteEffect / isWardrobeEffect tell the two shapes apart, and neither claims a half-write', () => {
    const isWrite = t['isWriteEffect'] as ((e: unknown) => boolean) | undefined;
    const isWardrobe = t['isWardrobeEffect'] as ((e: unknown) => boolean) | undefined;
    expect(typeof isWrite).toBe('function');
    expect(typeof isWardrobe).toBe('function');
    const write = { target: 'state.x', value: 1 };
    const gesture = { wardrobe: { action: 'clear_slot', slot: 'hair' } };
    expect([isWrite?.(write), isWardrobe?.(write)]).toEqual([true, false]);
    expect([isWrite?.(gesture), isWardrobe?.(gesture)]).toEqual([false, true]);
    expect([isWrite?.({ target: 'state.x' }), isWardrobe?.({ target: 'state.x' })]).toEqual([false, false]);
  });

  it('wardrobeGestureProblems speaks the loader’s words, in its switch order', () => {
    const problems = t['wardrobeGestureProblems'] as ((g: unknown) => unknown) | undefined;
    expect(typeof problems).toBe('function');
    expect(problems?.({ action: 'clear_slot', items: ['Wig'], mode: 'wear' })).toEqual([
      { field: 'slot', message: 'clear_slot needs a slot' },
      { field: 'items', message: 'clear_slot takes no items' },
      { field: 'mode', message: 'mode is only used with wear' },
    ]);
    expect(problems?.({ action: 'wear', items: [], mode: 'add_to_slot' })).toEqual([
      { field: 'items', message: 'wear needs at least one item' },
      { field: 'slot', message: 'mode add_to_slot needs a slot' },
    ]);
    expect(problems?.({ action: 'take_off', items: ['Hat'], slot: 'top' })).toEqual([]);
  });

  it('the slot enum is v4’s five, in v4’s order (a wardrobe-registry reorder trips HERE)', async () => {
    const { WARDROBE_SLOT_TYPES } = await import('../wardrobe/slot-meta');
    expect(WARDROBE_SLOT_TYPES).toEqual(['top', 'bottom', 'footwear', 'accessories', 'hair']);
  });
});
