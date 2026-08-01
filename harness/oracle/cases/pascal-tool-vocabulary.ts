/**
 * Oracle case: the Pascal tool VOCABULARY (P4.d19, v4 `faab6881` + `6864bf0e`).
 *
 * Drives the REAL code:
 *   collectToolVocabulary, isEmptyVocabulary  (lib/pascal/tool-vocabulary.ts)
 *   QtapCustomToolSchema                      (lib/pascal/custom-tool.types.ts)
 *
 * `references` is payload — the `/api/v1/chats/{id}/custom-tools` GET route puts
 * this object on every listing — so the whole shape is compared, key order
 * included, rather than field by field. Each row ships the definition's BYTES
 * (parsed by both sides' own JSON parser, exactly as `readToolFile` does) and
 * the SERIALIZED vocabulary, so "all seven keys always present" is part of what
 * is compared rather than a claim in a comment.
 *
 * Every definition here LOADS: a vocabulary is only ever computed for a
 * definition the roster accepted, so a rejected file has no vocabulary to
 * compare and would only be testing the schema twice.
 *
 * Run from inside the server checkout (v4 @ 231be14c, Node 24; the pinned
 * detached worktree):
 *   cd /tmp/qt-v4-pin-231be14c
 *   TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/pascal-tool-vocabulary.ts \
 *     > /tmp/oracle-pascal-vocabulary.ndjson
 */

import { QtapCustomToolSchema } from '@/lib/pascal/custom-tool.types';
import { collectToolVocabulary, isEmptyVocabulary } from '@/lib/pascal/tool-vocabulary';

const rows: unknown[] = [];

const CATCH_ALL = { when: true, message: 'nothing quoted', state: 'info' };

function def(extra: Record<string, unknown>): Record<string, unknown> {
  return { name: 'probe', description: 'A probe.', outcomes: [CATCH_ALL], ...extra };
}

/** One outcome carrying `message`, plus the mandatory trailing catch-all. */
function withMessage(message: string, extra: Record<string, unknown> = {}) {
  return def({ ...extra, outcomes: [{ when: { gt: 0.5 }, message, state: 'info' }, CATCH_ALL] });
}

const STR_PARAM = { parameters: { material: { type: 'string', default: 'brass' } } };
const TWO_PARAMS = {
  parameters: {
    material: { type: 'string', default: 'brass' },
    scale: { type: 'number', default: 1 },
  },
};
const LLM = { llm: { prompt: 'Answer about {{value}}.', errorMessage: 'The wire went dead.' } };

/** A definition carrying `effects` (P4.D35, c4d4b0de). */
function withEffects(effects: unknown[], extra: Record<string, unknown> = {}) {
  return def({ ...extra, effects });
}

const corpus: Array<[string, Record<string, unknown>]> = [
  // ---- the empty vocabulary: the case that renders no panel at all ---------
  ['quotes-nothing', def({})],
  ['rolls-dice-but-never-writes-it', def({ roll: '3d6' })],
  ['declares-a-param-never-quoted', def(STR_PARAM)],

  // ---- the four booleans, one at a time and together ----------------------
  ['value-placeholder', withMessage('The draw was {{value}}.')],
  ['roll-placeholder', withMessage('The raw draw was {{roll}}.')],
  ['dice-placeholder', withMessage('Rolled {{dice}}.', { roll: '2d6' })],
  ['llm-placeholder', withMessage('The oracle says {{llm}}.', LLM)],
  ['all-four-booleans', withMessage('{{value}} {{roll}} {{dice}} {{llm}}', { roll: '2d6', ...LLM })],

  // ---- the declared-params filter -----------------------------------------
  ['params-declared-and-quoted', withMessage('Working the {{params.material}}.', STR_PARAM)],
  ['params-quoted-but-undeclared', withMessage('Working the {{params.ghost}}.', STR_PARAM)],
  ['params-some-declared-some-not', withMessage('{{params.material}} {{params.scale}} {{params.ghost}}', TWO_PARAMS)],
  ['params-empty-suffix', withMessage('Bare {{params.}} suffix.', STR_PARAM)],
  ['params-quoted-twice', withMessage('{{params.material}} then {{params.material}}', STR_PARAM)],

  // ---- metadata: from placeholders, from `when`, and from BOTH gates -------
  ['metadata-placeholder', withMessage('Clearance {{metadata.clearanceLevel}}.')],
  ['metadata-empty-suffix', withMessage('Bare {{metadata.}} suffix.')],
  ['metadata-from-when', def({
    outcomes: [
      { when: { metadata: { hasAnsibleAccess: { eq: true } } }, message: 'granted', state: 'success' },
      CATCH_ALL,
    ],
  })],
  ['metadata-from-catch-all-only', def({ outcomes: [{ when: true, message: 'It reads {{metadata.house}}.', state: 'info' }] })],
  ['metadata-from-available-when', def({ availableWhen: { metadata: { toolAbilities: { contains: 'programmable' } } } })],
  ['metadata-from-withheld-when', def({ withheldWhen: { metadata: { novice: { eq: true } } } })],
  ['metadata-from-gate-and-table-deduped', def({
    availableWhen: { metadata: { clearance: { gte: 3 } } },
    outcomes: [
      { when: { metadata: { clearance: { gte: 5 }, house: { eq: 'Aurum' } } }, message: 'ok {{metadata.clearance}}', state: 'success' },
      CATCH_ALL,
    ],
  })],

  // ---- state: `{{state.path}}` AND `$state` refs anywhere in the tree ------
  ['state-placeholder', withMessage('The stakes are {{state.difficulty}}.')],
  ['state-empty-suffix', withMessage('Bare {{state.}} suffix.')],
  ['state-ref-in-a-roll-field', def({
    roll: { min: { $state: 'game.low', fallback: 0 }, max: { $state: 'game.high', fallback: 6 } },
  })],
  ['state-ref-in-a-parameter-default', def({
    parameters: { threshold: { type: 'number', default: { $state: 'game.threshold', fallback: 5 } } },
  })],
  ['state-ref-in-a-comparator-operand', def({
    outcomes: [
      { when: { gte: { $state: 'game.difficulty', fallback: 3 } }, message: 'hard', state: 'success' },
      CATCH_ALL,
    ],
  })],
  ['state-ref-and-placeholder-deduped', def({
    outcomes: [
      { when: { gte: { $state: 'game.difficulty', fallback: 3 } }, message: 'hard at {{state.game.difficulty}}', state: 'success' },
      CATCH_ALL,
    ],
  })],
  ['state-ref-in-a-string-operand', def({
    ...STR_PARAM,
    outcomes: [
      { when: { params: { material: { contains: { $state: 'game.needle', fallback: 'brass' } } } }, message: 'found', state: 'success' },
      CATCH_ALL,
    ],
  })],

  // ---- the llm prompt is a rendered string too ----------------------------
  ['llm-prompt-quotes-a-param', def({
    ...STR_PARAM,
    llm: { prompt: 'Is {{params.material}} auspicious for {{metadata.house}}?', errorMessage: 'Silence.' },
  })],
  ['llm-prompt-quotes-state', def({
    llm: { prompt: 'At difficulty {{state.difficulty}}, is it wise?', errorMessage: 'Silence.' },
  })],

  // ---- placeholder MATCHING corners --------------------------------------
  ['placeholder-with-inner-whitespace', withMessage('The draw was {{  value  }}.')],
  ['placeholder-empty-braces', withMessage('Nothing here: {{}} at all.')],
  ['placeholder-unknown-family', withMessage('Unknown {{whatever}} family.')],
  ['placeholder-single-braces', withMessage('Not a placeholder: {value}.')],
  ['placeholder-adjacent', withMessage('{{value}}{{roll}}')],

  // ---- ORDERING: keys where byte order and en-US collation disagree -------
  // `_a`, `Z`, `a`, `Ä` sort one way by code unit and another by ICU, so a
  // byte-order sort cannot pass this row.
  ['sort-collation-vs-byte-order', withMessage(
    '{{metadata.Z}} {{metadata.a}} {{metadata._a}} {{metadata.Ä}} {{metadata.b}}'
  )],
  ['sort-state-paths', withMessage('{{state.z.one}} {{state.A}} {{state._x}} {{state.ä}}')],
  // Parameter names take the identifier grammar (lowercase, `[a-z0-9_-]`), so
  // the collation difference reachable here is punctuation: `-` (0x2D) sorts
  // before `_` (0x5F) by code unit, and ICU weighs both below the letters.
  ['sort-params', withMessage('{{params.zeta}} {{params.a-b}} {{params.a_b}} {{params.alpha}}', {
    parameters: {
      zeta: { type: 'string', default: 'z' },
      'a-b': { type: 'string', default: 'h' },
      a_b: { type: 'string', default: 'u' },
      alpha: { type: 'string', default: 'a' },
    },
  })],
  // ---- the c4d4b0de write lists + the chipLabel scan ----------------------
  // A target is a WRITE, on its own list; the state prefix is stripped, the
  // metadata key is taken whole. An expression's {{ref}}s and a chipLabel's
  // placeholders are READS, on the existing lists, via the one scanner.
  ['effect-state-write', withEffects([{ target: 'state.encounter.count', value: 1 }])],
  ['effect-metadata-write', withEffects([{ target: 'metadata.lockBroken', value: true }])],
  ['effect-metadata-write-dotted-key', withEffects([{ target: 'metadata.ansible.tool', value: true }])],
  ['effect-nested-state-write', withEffects([{ target: 'state.party[0].hp', value: 1 }])],
  ['effect-expression-refs-are-reads', withEffects([{ target: 'state.tally', value: '{{state.tally}} + {{value}} + {{roll}} + {{dice}}' }])],
  ['effect-condition-metadata-is-a-read', withEffects([{ when: { metadata: { hasKey: { eq: true } } }, target: 'state.opened', value: true }])],
  ['effect-both-lists-at-once', withEffects([{ target: 'state.encounter.count', value: 1 }, { target: 'metadata.ok', value: 1 }])],
  ['effect-writes-are-sorted-and-deduped', withEffects([
    { target: 'state.zeta', value: 1 },
    { target: 'state.alpha', value: 2 },
    { target: 'state.zeta', value: 3 },
    { target: 'metadata.zulu', value: 4 },
    { target: 'metadata.alfa', value: 5 },
  ])],
  ['chip-label-placeholders-are-reads', def({ chipLabel: '{{value}} {{params.scale}} {{metadata.house}} {{state.floor}}', parameters: { scale: { type: 'number', default: 1 } } })],
  ['effect-llm-ref-is-a-read', withEffects([{ target: 'metadata.verdict', value: '{{llm}}' }], { llm: { prompt: 'Answer.', errorMessage: 'dead' } })],
  // A tool whose ONLY vocabulary is a write is still non-empty — the two new
  // lists join `isEmptyVocabulary`.
  ['write-only-is-not-empty', withEffects([{ target: 'state.k', value: 1 }])],
];

for (const [id, doc] of corpus) {
  const text = JSON.stringify(doc);
  const parsed = QtapCustomToolSchema.safeParse(JSON.parse(text));
  if (!parsed.success) {
    throw new Error(`vocabulary case '${id}': definition does not load — ${parsed.error.message}`);
  }
  const vocabulary = collectToolVocabulary(parsed.data);
  rows.push({
    kind: 'vocabulary',
    id,
    inputJson: text,
    // The whole object as it is serialized: all seven keys, in v4's key order.
    vocabulary: JSON.stringify(vocabulary),
    empty: isEmptyVocabulary(vocabulary),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
