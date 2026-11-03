/**
 * The operator-intervention arms of the browser schema twin (v4 `839575d89`,
 * "Feature: operator intervention in Pascal custom tools"): the tool's
 * `intervention` block, the per-parameter and per-effect `intervene` flags, and
 * the 18th known top-level key — authored BLIND (P4.D346 R-1) against v4's
 * REAL Zod at the round's pin `236c04e35` (`zod` 4.6.7).
 *
 * The committed definition corpus is P4.D345's to regenerate (its rows with
 * `intervention` arrive with its copy at the unification), so this file is the
 * stand-in with the same rigour as `custom-tool-types.triggers.spec.ts` /
 * `.effects.spec.ts`: every MEASURED row below was CAPTURED by driving v4's own
 * `QtapCustomToolSchema.safeParse` + `formatDefinitionIssues` +
 * `collectUnknownKeys` over the input bytes and is byte-compared here — the
 * verdict, the parsed data through `JSON.stringify` (so key ORDER is pinned:
 * `announceAsCharacter` → `intervention` → `parameters`, and `fields` in
 * `INTERVENTION_FIELD_KEYS` order whatever order the file wrote them in), the
 * unknown-key report, and the FULL rejection sentence.
 *
 * Regen recipe (the capture script is scratch, the ROWS are the artifact):
 *
 * ```ts
 * // in a detached v4 worktree at 236c04e35 with its own `npm ci --offline`,
 * // `npx tsx <script>` under Node 24:
 * import { QtapCustomToolSchema, formatDefinitionIssues, collectUnknownKeys }
 *   from '@/lib/pascal/custom-tool.types';
 * for (const [id, input] of CASES) {
 *   const r = QtapCustomToolSchema.safeParse(input);
 *   console.log(JSON.stringify({ id, input, success: r.success,
 *     data: r.success ? r.data : undefined,
 *     sentence: r.success ? undefined : formatDefinitionIssues(r.error),
 *     unknown: collectUnknownKeys(input) }));
 * }
 * ```
 */
import { describe, expect, it } from 'vitest';

import {
  INTERVENTION_FIELD_KEYS,
  KNOWN_TOP_LEVEL_KEYS,
  collectUnknownKeys,
  formatDefinitionIssues,
  safeParse,
} from './custom-tool-types';

const BASE = {
  name: 'probe',
  description: 'A probe.',
  outcomes: [{ when: true, message: '-', state: 'info' }],
};

interface Measured {
  id: string;
  input: unknown;
  success: boolean;
  /** The parsed data's `JSON.stringify` on success; the full sentence otherwise. */
  out: string;
  unknown: string[];
}

/** Measured at `236c04e35` through v4's real schema (2026-11-02). */
const MEASURED: Measured[] = [
  {
    id: "enabled-true",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":true},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "enabled-with-fields",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true,"fields":{"roll":false,"value":false}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":true,\"fields\":{\"roll\":false,\"value\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "off-with-title",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":false,"fields":{"title":false}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":false,\"fields\":{\"title\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "empty-block",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{}},
    success: false,
    out: "intervention.enabled: Invalid input: expected boolean, received undefined",
    unknown: [],
  },
  {
    id: "enabled-null",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":null}},
    success: false,
    out: "intervention.enabled: Invalid input: expected boolean, received null",
    unknown: [],
  },
  {
    id: "enabled-yes",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":"yes"}},
    success: false,
    out: "intervention.enabled: Invalid input: expected boolean, received string",
    unknown: [],
  },
  {
    id: "unknown-field-dice",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true,"fields":{"dice":false}}},
    success: false,
    out: "intervention.fields: Unrecognized key: \"dice\"",
    unknown: [],
  },
  {
    id: "fields-null",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true,"fields":null}},
    success: false,
    out: "intervention.fields: Invalid input: expected object, received null",
    unknown: [],
  },
  {
    id: "llmAnswer-without-llm",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true,"fields":{"llmAnswer":false}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":true,\"fields\":{\"llmAnswer\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "intervention-yes",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":"yes"},
    success: false,
    out: "intervention: Invalid input: expected object, received string",
    unknown: [],
  },
  {
    id: "param-false",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0,"intervene":false}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0,\"intervene\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "param-true",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0,"intervene":true}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0,\"intervene\":true}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "param-no",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0,"intervene":"no"}}},
    success: false,
    out: "parameters.bonus.intervene: Invalid input: expected boolean, received string",
    unknown: [],
  },
  {
    id: "param-null",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0,"intervene":null}}},
    success: false,
    out: "parameters.bonus.intervene: Invalid input: expected boolean, received null",
    unknown: [],
  },
  {
    id: "param-unknown-key",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0,"intervene":false,"bogus":1}}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0,\"intervene\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "effect-write-false",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"target":"state.n","value":1,"intervene":false}]},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"target\":\"state.n\",\"value\":1,\"intervene\":false}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "effect-wardrobe-false",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"wardrobe":{"action":"wear","items":["Hat"]},"intervene":false}]},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"wardrobe\":{\"action\":\"wear\",\"items\":[\"Hat\"]},\"intervene\":false}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "effect-inventory-false",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"inventory":{"action":"pick_up","items":["Coin"]},"intervene":false}]},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"inventory\":{\"action\":\"pick_up\",\"items\":[\"Coin\"]},\"intervene\":false}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "effect-log-false",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"log":"A line.","intervene":false}]},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"effects\":[{\"log\":\"A line.\",\"intervene\":false}],\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "effect-no",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"target":"state.n","value":1,"intervene":"no"}]},
    success: false,
    out: "effects.0.intervene: Invalid input: expected boolean, received string",
    unknown: [],
  },
  {
    id: "effect-unknown-key",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"target":"state.n","value":1,"intervene":false,"bogus":1}]},
    success: false,
    out: "effects.0: Unrecognized key: \"bogus\"",
    unknown: [],
  },
  {
    id: "bogus-top",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true},"bogus":1},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":true},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: ["bogus"],
  },
  {
    id: "key-order",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"bonus":{"type":"number","default":0}},"intervention":{"enabled":true},"announceAsCharacter":true},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"announceAsCharacter\":true,\"intervention\":{\"enabled\":true},\"parameters\":{\"bonus\":{\"type\":\"number\",\"default\":0}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  // A second scratch probe at the same pin: key order, issue order, plurals.
  {
    id: "reversed",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"fields":{"announcer":false,"roll":true},"enabled":true}},
    success: true,
    out: "{\"name\":\"probe\",\"description\":\"A probe.\",\"intervention\":{\"enabled\":true,\"fields\":{\"roll\":true,\"announcer\":false}},\"outcomes\":[{\"when\":true,\"message\":\"-\",\"state\":\"info\"}]}",
    unknown: [],
  },
  {
    id: "two-bad",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":1,"fields":{"roll":"x","dice":false}}},
    success: false,
    out: "intervention.enabled: Invalid input: expected boolean, received number; intervention.fields.roll: Invalid input: expected boolean, received string; intervention.fields: Unrecognized key: \"dice\"",
    unknown: [],
  },
  {
    id: "two-unknown",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"intervention":{"enabled":true,"bogus":1,"fields":{"dice":false,"foo":true}}},
    success: false,
    out: "intervention.fields: Unrecognized keys: \"dice\", \"foo\"; intervention: Unrecognized key: \"bogus\"",
    unknown: [],
  },
  {
    id: "param-bad-and-refine",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"parameters":{"s":{"type":"string","default":"x","min":1,"intervene":0}}},
    success: false,
    out: "parameters.s.intervene: Invalid input: expected boolean, received number",
    unknown: [],
  },
  {
    id: "effect-flag-on-two-kinds",
    input: {"name":"probe","description":"A probe.","outcomes":[{"when":true,"message":"-","state":"info"}],"effects":[{"log":"x","target":"state.n","value":1,"intervene":"x"}]},
    success: false,
    out: "effects.0.intervene: Invalid input: expected boolean, received string",
    unknown: [],
  },
];

/**
 * A parameter carrying an unknown key: v4's `CustomToolParameterSchema` is a
 * plain `z.object` and STRIPS it (measured: `param-unknown-key` above parses).
 * The v5 twin has refused unknown parameter keys since before this round —
 * a PRE-EXISTING divergence outside this lane's hunks, recorded in the P4.D346
 * lane record for a follow-up, and skipped from the byte compare here.
 */
const PREEXISTING_PARAMETER_STRIP_DIVERGENCE = new Set(['param-unknown-key']);

const verdict = (input: unknown) => {
  const r = safeParse(input);
  return {
    success: r.success,
    out: r.success ? JSON.stringify(r.data) : formatDefinitionIssues(r.issues),
    unknown: collectUnknownKeys(input),
  };
};

describe('the intervention block, measured through v4 at 236c04e35', () => {
  for (const row of MEASURED) {
    if (PREEXISTING_PARAMETER_STRIP_DIVERGENCE.has(row.id)) continue;
    it(row.id, () => {
      expect(verdict(row.input)).toEqual({ success: row.success, out: row.out, unknown: row.unknown });
    });
  }

  it('records the one pre-existing divergence it skips', () => {
    const r = safeParse({ ...BASE, parameters: { bonus: { type: 'number', default: 0, intervene: false, bogus: 1 } } });
    expect(r.success).toBe(false);
    expect(r.success ? '' : formatDefinitionIssues(r.issues)).toBe('parameters.bonus: Unrecognized key: "bogus"');
  });
});

// v4 `__tests__/unit/lib/pascal/intervention-format.test.ts:17-47`, by name.
describe('the intervention block', () => {
  it('accepts an enabled block, with or without field flags', () => {
    expect(safeParse({ ...BASE, intervention: { enabled: true } }).success).toBe(true);
    expect(
      safeParse({ ...BASE, intervention: { enabled: true, fields: { roll: false, value: false } } }).success,
    ).toBe(true);
  });

  it('rejects an empty block as ambiguous', () => {
    expect(safeParse({ ...BASE, intervention: {} }).success).toBe(false);
  });

  it('rejects an unknown field key', () => {
    expect(safeParse({ ...BASE, intervention: { enabled: true, fields: { dice: false } } }).success).toBe(false);
  });

  it('accepts and ignores flags on a tool that is not intervenable', () => {
    const parsed = safeParse({
      ...BASE,
      intervention: { enabled: false, fields: { llmAnswer: false } },
      parameters: { bonus: { type: 'number', default: 0, intervene: false } },
    });
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;
    expect(parsed.data.intervention?.enabled).toBe(false);
    expect(parsed.data.parameters?.['bonus']?.intervene).toBe(false);
  });

  it('keeps an effect flag through the schema', () => {
    const parsed = safeParse({ ...BASE, effects: [{ target: 'state.n', value: 1, intervene: false }] });
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;
    expect(parsed.data.effects?.[0]?.intervene).toBe(false);
  });
});

describe('the key list and the known keys', () => {
  it('carries the eleven field keys in v4 order', () => {
    expect([...INTERVENTION_FIELD_KEYS]).toEqual([
      'roll', 'value', 'llmPrompt', 'llmAnswer', 'outcome', 'message', 'state',
      'chipLabel', 'title', 'visibility', 'announcer',
    ]);
  });

  it('treats intervention as a known top-level key, the eighteenth, after announceAsCharacter', () => {
    expect(KNOWN_TOP_LEVEL_KEYS).toHaveLength(18);
    expect(KNOWN_TOP_LEVEL_KEYS.indexOf('intervention')).toBe(KNOWN_TOP_LEVEL_KEYS.indexOf('announceAsCharacter') + 1);
    expect(collectUnknownKeys({ ...BASE, intervention: { enabled: true }, bogus: 1 })).toEqual(['bogus']);
  });

  it('serializes announceAsCharacter, then intervention, then parameters', () => {
    const parsed = safeParse({
      ...BASE,
      parameters: { bonus: { type: 'number', default: 0 } },
      intervention: { enabled: true },
      announceAsCharacter: true,
    });
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;
    const keys = Object.keys(parsed.data);
    expect(keys.slice(keys.indexOf('announceAsCharacter'), keys.indexOf('parameters') + 1)).toEqual([
      'announceAsCharacter', 'intervention', 'parameters',
    ]);
  });
});
