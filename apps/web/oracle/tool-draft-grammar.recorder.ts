/**
 * The v4-side recorder behind `src/app/pascal/tool-draft-grammar.oracle.spec.ts`
 * (P4.D334 R-13 / Tier 1 item 8).
 *
 * `tool-draft.ts` is the Workbench's draft model — the bijection between the
 * form-shaped draft and the canonical definition. The SPA had no oracle for it
 * (spec-pinned since P4.D307); this one closes the gap for the round's arms:
 * the one-level any-of groups (`a53024d3c`), the outcome brief (`322a5488e`),
 * the Inventory (`e1872c94f`) and the Log (`e4a60ad9e`). It drives v4's REAL
 * `lib/pascal/tool-draft.ts` at the pin.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...`.
 *
 * Draft inputs are DRAFT JSON built from `newDraft()` and edited; every
 * `nextDraftId` value is mapped to `<prefix>#<n>` by first appearance BEFORE
 * recording (draft first, then the outputs, so a `where.id` in an issue names
 * the same token), which lets v5 feed the same object to `definitionFromDraft`
 * / `validateDraft` WITHOUT going through the parse — most rows run LIVE on a
 * branch that lacks P4.D333's twin.
 *
 * Row kinds `{ kind, id, needs, … , out }`:
 *   - `groupsOf`     joiner list → the groups as index lists;
 *   - `draft`        draft → `{ definition, serialized, issues }` (the
 *                    `definitionFromDraft` object's `JSON.stringify`, the
 *                    `serializeDraft` text, and `[severity, where, message]`
 *                    in order);
 *   - `rename`       draft + from/to → the renamed draft;
 *   - `find`         draft + name → `findParameterReferences`;
 *   - `definition`   a v4-valid definition → `draftFromDefinition` (ids
 *                    normalised) + its round trip + `validateDraft` — ALL
 *                    `needs: ['P4D333']` (the parse);
 *   - `direct`       a direct call (`subjectValueType`, `conditionSlotKey`,
 *                    `gateConditionSlotKey`, `draftTableTestsLlm`,
 *                    `promptReadsBrief`, `reachableOutcomesMissingBrief`).
 *
 * `needs` is set by this file's author from what the row's truth reads: a
 * `draftFromDefinition` call reads P4.D333's parse; a placeholder of kind
 * `outcome` / `inventory` reads its classify arm; `inventoryGestureProblems`
 * and `parseInventoryOperand` (the object roll's path) read its grammar twins.
 *
 * Run it from a pinned v4 worktree at `322a5488e` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 322a5488e}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/tool-draft-grammar.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/tool-draft-grammar.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx tool-draft-grammar.recorder.ts > "$OUT")
 * wc -l "$OUT"                                  # expect the header count
 * grep -c '"kind":"draft"' "$OUT"               # expect > 0
 * ```
 */

import {
  conditionSlotKey,
  definitionFromDraft,
  draftFromDefinition,
  draftTableTestsLlm,
  findParameterReferences,
  gateConditionSlotKey,
  groupsOf,
  newDraft,
  newDraftEffect,
  nextDraftId,
  promptReadsBrief,
  reachableOutcomesMissingBrief,
  renameParameterEverywhere,
  serializeDraft,
  subjectValueType,
  validateDraft,
  type DraftCondition,
  type DraftEffect,
  type DraftGateCondition,
  type ToolDraft,
} from '@/lib/pascal/tool-draft';

type Needs = Array<'P4D333' | 'P4D330'>;
type AnyDraft = ToolDraft & Record<string, unknown>;

// ---------------------------------------------------------------------------
// Id normalisation
// ---------------------------------------------------------------------------

const ID_PATTERN = /^(cond|outcome|effect|param|gate|gcond)-\d+$/;

/** Map every draft id to `<prefix>#<n>` by first appearance across the given values, in order. */
function normaliser() {
  const seen = new Map<string, string>();
  const counts = new Map<string, number>();
  const map = (value: unknown): unknown => {
    if (typeof value === 'string' && ID_PATTERN.test(value)) {
      let token = seen.get(value);
      if (!token) {
        const prefix = value.slice(0, value.indexOf('-'));
        const n = (counts.get(prefix) ?? 0) + 1;
        counts.set(prefix, n);
        token = `${prefix}#${n}`;
        seen.set(value, token);
      }
      return token;
    }
    if (Array.isArray(value)) return value.map(map);
    if (value && typeof value === 'object') {
      return Object.fromEntries(Object.entries(value as Record<string, unknown>).map(([k, v]) => [k, map(v)]));
    }
    return value;
  };
  return map;
}

function emit(row: Record<string, unknown>): void {
  console.log(JSON.stringify(row));
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

/** A valid base draft: name + description, the two default outcomes. */
function base(): AnyDraft {
  const d = newDraft() as AnyDraft;
  d.name = 'probe';
  d.description = 'A probe.';
  d.outcomes[0].message = 'hit';
  return d;
}

type Operand = DraftCondition['operand'];
const num = (text: string): Operand => ({ kind: 'number', text });
const str = (text: string): Operand => ({ kind: 'string', text });

function cond(
  subject: DraftCondition['subject'],
  comparator: DraftCondition['comparator'],
  operand: Operand,
  joiner?: 'and' | 'or'
): DraftCondition {
  return { id: nextDraftId('cond'), subject, comparator, operand, ...(joiner ? { joiner } : {}) };
}
function gcond(
  subject: DraftGateCondition['subject'],
  key: string,
  comparator: DraftGateCondition['comparator'],
  operand: DraftGateCondition['operand'],
  joiner?: 'and' | 'or'
): DraftGateCondition {
  return { id: nextDraftId('gate'), subject, key, comparator, operand, ...(joiner ? { joiner } : {}) };
}
function effect(kind: DraftEffect['kind'], over: Partial<DraftEffect> = {}): DraftEffect {
  return { ...newDraftEffect(kind), ...over };
}

// ---------------------------------------------------------------------------
// needs
// ---------------------------------------------------------------------------

function needsOf(draft: AnyDraft): Needs {
  const text = JSON.stringify(draft);
  if (
    text.includes('{{outcome.') ||
    text.includes('{{inventory.') ||
    draft.effects.some((e) => e.kind === 'inventory') ||
    draft.rollForm === 'inventory' ||
    text.includes('"$inventory"')
  ) {
    return ['P4D333'];
  }
  return [];
}

// ---------------------------------------------------------------------------
// groupsOf
// ---------------------------------------------------------------------------

const JOINERS: Array<[string, Array<'and' | 'or' | undefined>]> = [
  ['empty', []],
  ['one chip', [undefined]],
  ['one chip, joiner or', ['or']],
  ['and only', [undefined, 'and', 'and']],
  ['leading or', ['or', 'and', 'or']],
  ['or on every chip', ['or', 'or', 'or', 'or']],
  ['alternating', [undefined, 'or', 'and', 'or', 'and']],
  ['no joiner after an or', [undefined, 'or', undefined, undefined]],
  ['two lines', [undefined, 'and', 'or', 'and']],
  ['nine lines', [undefined, 'or', 'or', 'or', 'or', 'or', 'or', 'or', 'or']],
];
for (const [id, joiners] of JOINERS) {
  const chips = joiners.map((joiner, index) => ({ index, ...(joiner ? { joiner } : {}) }));
  const out = groupsOf(chips).map((group) => group.map((c) => c.index));
  emit({ kind: 'groupsOf', id, needs: [], joiners, out: JSON.stringify(out) });
}

// ---------------------------------------------------------------------------
// draft rows
// ---------------------------------------------------------------------------

interface Scenario {
  id: string;
  draft: AnyDraft;
  needs?: Needs;
}
const SCENARIOS: Scenario[] = [];
const add = (id: string, draft: AnyDraft, needs?: Needs) => SCENARIOS.push({ id, draft, needs });

const meta = (key: string) => ({ kind: 'metadata' as const, key });
const agg = (key: string) => ({ kind: 'inventoryAggregate' as const, key });

// -- any-of: outcomes ------------------------------------------------------
{
  const d = base();
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', num('0.9')), cond(meta('lucky'), 'eq', { kind: 'boolean', value: true }, 'or')];
  add('any-of outcome, two lines', d);
}
{
  const d = base();
  d.outcomes[0].conditions = [
    cond({ kind: 'value' }, 'gte', num('1')),
    cond({ kind: 'roll' }, 'lt', num('4')),
    cond({ kind: 'value' }, 'lt', num('9'), 'or'),
    cond(meta('rank'), 'gt', num('3'), 'and'),
  ];
  add('any-of outcome, a slot repeated across lines', d);
}
{
  const d = base();
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', num('1')), cond({ kind: 'value' }, 'gte', num('2'), 'and')];
  add('one line repeating a slot', d);
}
{
  const d = base();
  d.outcomes[0].conditions = Array.from({ length: 9 }, (_, i) => cond({ kind: 'value' }, 'gte', num(String(i)), i === 0 ? undefined : 'or'));
  add('nine lines joined by OR', d);
}
{
  const d = base();
  d.outcomes[0].conditions = Array.from({ length: 8 }, (_, i) => cond({ kind: 'value' }, 'gte', num(String(i)), i === 0 ? undefined : 'or'));
  add('eight lines joined by OR', d);
}
{
  const d = base();
  d.outcomes[0].conditions = [
    cond({ kind: 'value' }, 'gte', num('')),
    cond({ kind: 'value' }, 'lt', num('3'), 'or'),
  ];
  add('a line of incomplete chips drops', d);
}
{
  const d = base();
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', num('')), cond({ kind: 'value' }, 'lt', num(''), 'or')];
  add('every line incomplete', d);
}
{
  const d = base();
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', num('1'), 'or'), cond({ kind: 'value' }, 'lt', num('3'), 'or')];
  add('a leading or on the first chip is ignored', d);
}
// -- any-of: gates ---------------------------------------------------------
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [
    gcond('metadata', 'a', 'eq', { kind: 'boolean', value: true }),
    gcond('inventoryAggregate', 'equipped.metadata.toolVersion.max', 'gte', { kind: 'number', text: '4' }),
    gcond('metadata', 'a', 'eq', { kind: 'boolean', value: true }, 'or'),
    gcond('inventoryAggregate', 'equipped.names', 'eq', { kind: 'string', text: 'ansible resonator' }),
  ];
  add('any-of gate with an aggregate on each line', d);
}
{
  const d = base();
  d.gateMode = 'withheld';
  d.gateConditions = [gcond('metadata', 'rank', 'gt', { kind: 'number', text: '3' }), gcond('metadata', 'rank', 'gt', { kind: 'number', text: '5' }, 'or')];
  add('two lines repeat a gate slot', d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [gcond('metadata', 'rank', 'gt', { kind: 'number', text: '3' }), gcond('metadata', 'rank', 'gt', { kind: 'number', text: '5' })];
  add('one line repeats a gate slot', d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = Array.from({ length: 9 }, (_, i) => gcond('metadata', `k${i}`, 'eq', { kind: 'number', text: '1' }, i === 0 ? undefined : 'or'));
  add('nine gate lines', d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [gcond('metadata', '', 'eq', { kind: 'number', text: '1' }), gcond('metadata', 'k', 'eq', { kind: 'number', text: '1' }, 'or')];
  add('a blank gate key on line one', d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [gcond('metadata', 'k', 'eq', { kind: 'number', text: '' }), gcond('metadata', 'k', 'eq', { kind: 'number', text: '' }, 'or')];
  add('a gate of incomplete lines', d);
}
// -- any-of: effects (carried verbatim) -----------------------------------
{
  const d = base();
  d.effects = [
    effect('write', {
      target: 'state.tuned',
      valueKind: 'literal-boolean',
      value: 'true',
      when: { kind: 'verbatim', when: [{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }] } as never,
    }),
  ];
  add('an array effect when is carried verbatim', d);
}
{
  const d = base();
  d.effects = [
    effect('write', {
      target: 'state.tuned',
      valueKind: 'literal-boolean',
      value: 'true',
      when: { kind: 'verbatim', when: [{ outcome: { eq: 'success' } }, { llm: { ok: true } }] } as never,
    }),
  ];
  add('an array effect when testing the consult with it off', d);
}

// -- briefs ---------------------------------------------------------------
function withLlm(prompt: string): AnyDraft {
  const d = base();
  d.parameters = [
    { id: nextDraftId('param'), name: 'layers', type: 'number', defaultValue: '95', description: '', min: '', max: '' },
  ] as never;
  d.llmEnabled = true;
  d.llmPrompt = prompt;
  d.llmErrorMessage = 'The link falters.';
  d.outcomes[0].conditions = [cond({ kind: 'roll' }, 'gte', { kind: 'param', name: 'layers' })];
  d.outcomes[0].message = '{{llm}}';
  d.outcomes[1].message = '{{llm}}';
  return d;
}
const READS = 'Write a report. State: {{outcome.state}}, row {{outcome.index}}. Conclusion to reach: {{outcome.brief}}';
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'Confirmed at {{params.layers}} layers.';
  d.outcomes[1].brief = 'Not found.';
  add('a scan with briefs, as v4 writes it', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = '  ';
  d.outcomes[1].brief = 'Not found.';
  add('a blank brief is omitted', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = '  padded brief  ';
  d.outcomes[1].brief = 'x';
  add('a padded brief is emitted untrimmed', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'a'.repeat(1001);
  d.outcomes[1].brief = 'b';
  add('a 1001-character brief', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'a'.repeat(1000);
  d.outcomes[1].brief = ' '.repeat(1001);
  add('a 1000-character brief and a 1001-space brief', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'Quotes {{llm}} and {{outcome.index}} and {{params.ghost}}.';
  d.outcomes[1].brief = 'ok';
  add('a brief quoting the answer, the row and a ghost parameter', d, ['P4D333']);
}
{
  const d = withLlm(READS);
  d.llmEnabled = false;
  d.outcomes[0].brief = 'b';
  d.outcomes[1].brief = '';
  add('a brief with the consult off', d);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = '';
  d.outcomes[1].brief = '';
  add('the prompt reads a brief no row has', d, ['P4D333']);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'x';
  d.outcomes[1].brief = 'y';
  d.outcomes[0].conditions.push(cond({ kind: 'llm' }, 'eq', str('YES')));
  add('{{outcome.*}} in the prompt when a row tests the consult', d, ['P4D333']);
}
{
  const d = withLlm(READS);
  d.outcomes[0].brief = 'x';
  d.outcomes[1].brief = 'y';
  d.outcomes[0].conditions.push(cond({ kind: 'llm-ok' }, 'eq', { kind: 'boolean', value: true }));
  add('{{outcome.*}} in the prompt when a row tests the consult succeeded', d, ['P4D333']);
}
for (const [id, prompt] of [
  ['compare roll and param', 'If {{roll}} >= {{params.layers}}, report success.'],
  ['compare value and figure', 'Report entangled when {{value}} < 40.'],
  ['do the math', 'You must actually do the math here.'],
  ['compute the sum', 'Compute the sum of the layers.'],
  ['work out the arithmetic', 'Work out the arithmetic first.'],
  ['quiet figure', 'Write a report on a scan of depth {{params.layers}}. Tone: clinical.'],
  ['two comparisons, one warning', 'If {{roll}} > {{params.layers}} or {{value}} <= 3, say so.'],
  ['unicode ordering sign', 'Say yes if {{roll}} ≥ {{params.layers}}.'],
  ['equality between figures', 'It is true when {{value}} == {{roll}}.'],
  ['a plain quote', 'The odds {{roll}} and {{value}} are shown below.'],
  ['quotes the outcome fields', 'State {{outcome.state}} on row {{outcome.index}}.'],
] as Array<[string, string]>) {
  const d = withLlm(prompt);
  d.outcomes[0].conditions = [];
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', num('1'))];
  const needs: Needs = prompt.includes('{{outcome.') ? ['P4D333'] : [];
  add(`prompt: ${id}`, d, needs);
  const e = clone(d);
  e.outcomes[0].brief = 'b';
  e.outcomes[1].brief = 'b';
  add(`prompt with briefs: ${id}`, e, needs);
}
{
  const d = withLlm(READS);
  d.llmPrompt = 'Reads {{outcome.state}} only.';
  d.outcomes[0].brief = '';
  add('the prompt reads no brief', d, ['P4D333']);
}

// -- the Inventory: chips -------------------------------------------------
function withParam(d: AnyDraft, name: string, type: string, def = ''): AnyDraft {
  d.parameters = [
    ...d.parameters,
    { id: nextDraftId('param'), name, type, defaultValue: def, description: '', min: '', max: '' },
  ] as never;
  return d;
}
{
  const d = withParam(base(), 'weapon', 'string', 'Excalibur');
  d.outcomes[0].conditions = [
    cond({ kind: 'inventory', key: '{{params.weapon}}' }, 'eq', str('equipped')),
    cond({ kind: 'inventoryCount', key: 'ring' }, 'gte', num('2')),
  ];
  add('inventory and inventoryCount chips', d);
}
for (const [id, operand, comparator] of [
  ['a state outside the four', str('worn'), 'eq'],
  ['an ordering comparator', str('equipped'), 'gt'],
  ['a containment comparator', str('equipped'), 'contains'],
  ['a number operand', num('1'), 'eq'],
  ['carried and neq', str('carried'), 'neq'],
  ['a missing parameter', { kind: 'param', name: 'ghost' }, 'eq'],
  ['a number parameter', { kind: 'param', name: 'n' }, 'eq'],
  ['a string parameter', { kind: 'param', name: 's' }, 'eq'],
  ['a nameless parameter', { kind: 'param', name: '' }, 'eq'],
] as Array<[string, Operand, DraftCondition['comparator']]>) {
  const d = withParam(withParam(base(), 'n', 'number', '1'), 's', 'string', 'x');
  d.outcomes[0].conditions = [cond({ kind: 'inventory', key: 'sword' }, comparator, operand)];
  add(`inventory chip: ${id}`, d);
}
for (const [id, key] of [
  ['blank key', ''],
  ['padded key is untrimmed', ' sword '],
  ['an undeclared parameter in the key', '{{params.ghost}}'],
  ['a declared parameter in the key', '{{params.s}}'],
] as Array<[string, string]>) {
  const d = withParam(base(), 's', 'string', 'x');
  d.outcomes[0].conditions = [cond({ kind: 'inventory', key }, 'eq', str('equipped')), cond({ kind: 'inventoryCount', key }, 'gte', num('1'))];
  add(`inventory key: ${id}`, d);
}
for (const comparator of ['contains', 'ncontains', 'eq', 'lt'] as const) {
  const d = base();
  d.outcomes[0].conditions = [cond({ kind: 'inventoryCount', key: 'ring' }, comparator, comparator === 'contains' || comparator === 'ncontains' ? str('x') : num('2'))];
  add(`inventoryCount chip with ${comparator}`, d);
}

// -- the aggregate audit --------------------------------------------------
for (const [key, comparator, operand] of [
  ['equipped.bag', 'eq', num('1')],
  ['equipped.names', 'gt', num('1')],
  ['owned.metadata.tags', 'lte', num('1')],
  ['equipped.count', 'contains', str('x')],
  ['equipped.metadata.v.max', 'ncontains', str('x')],
  ['', 'eq', num('1')],
  ['  ', 'eq', num('1')],
  ['{{params.s}}.count', 'eq', num('1')],
  ['equipped.metadata.{{params.s}}', 'eq', str('x')],
  ['owned.metadata.stats.max', 'gte', num('1')],
  ['owned.metadata.stats.sum', 'gte', num('1')],
  [' equipped.names ', 'contains', str('x')],
  ['equipped.names', 'contains', str('x')],
  ['reachable.titles', 'neq', str('x')],
  ['equipped.metadata.toolAbilities', 'contains', str('suppress')],
  ['equipped.count', 'gte', num('1')],
] as Array<[string, DraftCondition['comparator'], Operand]>) {
  const d = withParam(base(), 's', 'string', 'x');
  d.outcomes[0].conditions = [cond(agg(key), comparator, operand)];
  add(`aggregate outcome chip ${JSON.stringify(key)} ${comparator}`, d);
  const g = base();
  g.gateMode = 'available';
  g.gateConditions = [
    gcond('inventoryAggregate', key, comparator, operand.kind === 'param' ? ({ kind: 'string', text: '' } as never) : (operand as never)),
  ];
  add(`aggregate gate chip ${JSON.stringify(key)} ${comparator}`, g);
}
{
  const d = withParam(base(), 'n', 'number', '1');
  d.outcomes[0].conditions = [cond(agg('equipped.names'), 'contains', { kind: 'param', name: 'n' })];
  add('aggregate with a number parameter operand', d);
}
{
  const d = withParam(base(), 's', 'string', 'x');
  d.outcomes[0].conditions = [cond(agg('equipped.count'), 'gte', { kind: 'param', name: 's' })];
  add('aggregate count against a string parameter', d);
}

// -- the gate's inventory rules -------------------------------------------
for (const [id, subject, key, comparator, operand] of [
  ['state, valid', 'inventory', 'lantern', 'eq', { kind: 'string', text: 'equipped' }],
  ['state, bad state', 'inventory', 'lantern', 'eq', { kind: 'string', text: 'worn' }],
  ['state, ordering', 'inventory', 'lantern', 'gt', { kind: 'string', text: 'equipped' }],
  ['state, number operand', 'inventory', 'lantern', 'eq', { kind: 'number', text: '1' }],
  ['state, templated key', 'inventory', '{{params.weapon}}', 'eq', { kind: 'string', text: 'equipped' }],
  ['state, blank key', 'inventory', '  ', 'eq', { kind: 'string', text: 'equipped' }],
  ['count, valid', 'inventoryCount', 'ring', 'gte', { kind: 'number', text: '1' }],
  ['count, containment', 'inventoryCount', 'ring', 'contains', { kind: 'string', text: 'x' }],
  ['count, string operand', 'inventoryCount', 'ring', 'eq', { kind: 'string', text: 'x' }],
  ['count, templated key', 'inventoryCount', 'a{{x}}', 'eq', { kind: 'number', text: '1' }],
  ['count, blank key', 'inventoryCount', '', 'eq', { kind: 'number', text: '1' }],
  ['metadata, blank key', 'metadata', '', 'eq', { kind: 'number', text: '1' }],
  ['progress, valid', 'progress', 'cannon.percent', 'gte', { kind: 'number', text: '50' }],
] as Array<[string, DraftGateCondition['subject'], string, DraftGateCondition['comparator'], DraftGateCondition['operand']]>) {
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [gcond(subject, key, comparator, operand)];
  add(`gate chip: ${id}`, d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [
    gcond('inventory', 'lantern', 'eq', { kind: 'string', text: 'equipped' }),
    gcond('inventory', 'lantern', 'eq', { kind: 'string', text: 'carried' }),
    gcond('metadata', 'lantern', 'eq', { kind: 'string', text: 'x' }),
    gcond('inventoryCount', 'lantern', 'eq', { kind: 'number', text: '1' }),
  ];
  add('gate slots across subjects', d);
}
{
  const d = base();
  d.gateMode = 'available';
  d.gateConditions = [
    gcond('inventoryAggregate', ' equipped.count ', 'gte', { kind: 'number', text: '1' }),
    gcond('inventory', ' sword ', 'eq', { kind: 'string', text: 'equipped' }),
    gcond('inventoryCount', ' sword ', 'gte', { kind: 'number', text: '1' }),
    gcond('metadata', 'a', 'eq', { kind: 'number', text: '1' }),
    gcond('progress', 'cannon.percent', 'gte', { kind: 'number', text: '5' }),
  ];
  add('gate sheets in schema order, keys as typed', d);
}

// -- the $inventory roll --------------------------------------------------
for (const [id, path, fallback] of [
  ['valid', 'sword.metadata.damage', '1d4'],
  ['blank path', '', '1d4'],
  ['not a metadata path', 'sword.title', 'four'],
  ['dice not given', 'sword.metadata.damage', ''],
  ['padded', '  sword.metadata.damage  ', '  2d6  '],
  ['bad dice', 'sword.metadata.damage', 'four'],
  ['a dotted key', 'sword.metadata.a.b', '1d4'],
] as Array<[string, string, string]>) {
  const d = base();
  d.rollForm = 'inventory';
  d.rollInventory = { path, fallback };
  add(`object roll: ${id}`, d, ['P4D333']);
}
{
  const d = base();
  d.rollForm = 'range';
  d.rollInventory = { path: 'sword.metadata.damage', fallback: '1d4' };
  add('the object roll kept while another form is chosen', d);
}
{
  const d = base();
  d.rollForm = 'range';
  d.rollRange.min = { kind: 'inventory', path: 'sword.metadata.reach', fallback: '1' } as never;
  d.rollRange.max = { kind: 'inventory', path: 'sword.metadata.reach', fallback: 'x' } as never;
  add('a $inventory range field', d);
}
{
  const d = base();
  d.rollForm = 'dice';
  d.rollDice = '2d6';
  d.outcomes[0].message = 'rolled {{dice}}';
  add('{{dice}} under the dice form', d);
}
{
  const d = base();
  d.rollForm = 'inventory';
  d.rollInventory = { path: 'sword.metadata.damage', fallback: '1d4' };
  d.outcomes[0].message = 'rolled {{dice}}';
  add('{{dice}} under the object form', d, ['P4D333']);
}
{
  const d = base();
  d.rollForm = 'range';
  d.outcomes[0].message = 'rolled {{dice}}';
  add('{{dice}} under the range form', d);
}
{
  const d = base();
  d.parameters = [
    { id: nextDraftId('param'), name: 'p', type: 'number', defaultValue: { $inventory: 'sword.metadata.reach', fallback: 3 }, description: '', min: '', max: '' },
  ] as never;
  add('a $inventory parameter default', d, ['P4D333']);
}
{
  const d = base();
  d.parameters = [
    { id: nextDraftId('param'), name: 'p', type: 'number', defaultValue: { $state: 'x.y', fallback: 3 }, description: '', min: '', max: '' },
  ] as never;
  d.outcomes[0].conditions = [cond({ kind: 'value' }, 'gte', { kind: 'inventory', path: 'sword.metadata.reach', fallback: 2 } as never)];
  add('a $state default and a $inventory operand', d);
}

// -- the inventory gesture ------------------------------------------------
const gest = (action: string, items: string[], equip = true) => ({ action, items, equip }) as never;
for (const [id, inv, extra] of [
  ['equip one', gest('equip', ['Excalibur']), undefined],
  ['unequip two', gest('unequip', ['a', 'b']), undefined],
  ['pick_up with equip false', gest('pick_up', ['magic bow'], false), undefined],
  ['pick_up with equip true', gest('pick_up', ['magic bow'], true), undefined],
  ['equip with equip false', gest('equip', ['x'], false), undefined],
  ['blank rows', gest('equip', ['', '  ', 'sword']), undefined],
  ['all blank', gest('equip', ['', ' ']), undefined],
  ['a metadata path in an item', gest('equip', ['sword.metadata.damage']), undefined],
  ['an undeclared parameter', gest('equip', ['{{params.ghost}}']), undefined],
  ['a declared parameter', gest('equip', ['{{params.loot}}']), 'loot'],
  ['{{llm}} with the consult off', gest('equip', ['{{llm}}']), undefined],
  ['an unknown placeholder', gest('equip', ['{{mystery}}']), undefined],
  ['padded items are trimmed on save', gest('unequip', ['  sword  ']), undefined],
] as Array<[string, never, string | undefined]>) {
  let d = base();
  if (extra) d = withParam(d, extra, 'string', 'x');
  d.effects = [effect('inventory', { inventory: inv })];
  add(`inventory effect: ${id}`, d, ['P4D333']);
}
{
  const d = base();
  d.effects = [
    effect('inventory', {
      inventory: gest('equip', ['sword']),
      when: { kind: 'outcome-state', state: 'failure' },
    }),
    effect('log', { log: 'Worn.', when: { kind: 'outcome-state', state: 'success' } }),
  ];
  add('inventory and log effects with outcome-state whens', d, ['P4D333']);
}
{
  const d = base();
  d.effects = [effect('inventory', { inventory: gest('equip', ['sword']), target: 'state.x', value: '1', valueKind: 'literal-number' })];
  add('only the active half is emitted', d, ['P4D333']);
}

// -- the log --------------------------------------------------------------
for (const [id, log, extra] of [
  ['a line', '{{character}} rolled {{roll}} against the lock.', undefined],
  ['padded is trimmed', '   spaced out   ', undefined],
  ['blank', '   ', undefined],
  ['1000 characters', 'x'.repeat(1000), undefined],
  ['1001 characters', 'x'.repeat(1001), undefined],
  ['1001 trimmed to 1000', ' ' + 'x'.repeat(1000) + ' ', undefined],
  ['an undeclared parameter', 'Opened by {{params.ghost}}.', undefined],
  ['a declared parameter', 'Opened by {{params.who}}.', 'who'],
  ['{{llm}} with the consult off', 'The model said {{llm}}.', undefined],
  ['a state placeholder', 'Tally {{state.tally}}.', undefined],
  ['an unknown placeholder', 'See {{mystery}}.', undefined],
  ['too long and with an undeclared parameter', 'x'.repeat(1001) + '{{params.ghost}}', undefined],
] as Array<[string, string, string | undefined]>) {
  let d = base();
  if (extra) d = withParam(d, extra, 'string', 'x');
  d.effects = [effect('log', { log })];
  add(`log effect: ${id}`, d);
}
{
  const d = withLlm('Say something.');
  d.effects = [effect('log', { log: 'The model said {{llm}}.' })];
  add('log effect: {{llm}} with the consult on', d);
}

// -- rename / find over every arm -----------------------------------------
const RICH = (): AnyDraft => {
  const d = withLlm('Prompt {{params.layers}}.');
  d.outcomes[0].brief = 'Brief {{params.layers}} and {{ params.layers }}.';
  d.outcomes[0].message = 'Msg {{params.layers}}';
  d.outcomes[1].brief = 'none';
  d.effects = [
    effect('log', { log: 'Log {{params.layers}}.' }),
    effect('wardrobe', { wardrobe: { action: 'wear', items: ['{{params.layers}} hat'], mode: 'wear', slot: '' } as never }),
    effect('inventory', { inventory: gest('equip', ['{{params.layers}} blade']) }),
  ];
  d.outcomes[0].conditions = [
    cond({ kind: 'inventory', key: '{{params.layers}}' }, 'eq', str('equipped')),
    cond(agg('equipped.metadata.x'), 'gte', { kind: 'param', name: 'layers' }),
    cond({ kind: 'param', name: 'layers' }, 'gte', num('1')),
  ];
  d.rollRange.min = { kind: 'param', name: 'layers' };
  return d;
};
add('RICH draft for rename and find', RICH(), ['P4D333']);

// ---------------------------------------------------------------------------
// Emit the draft rows
// ---------------------------------------------------------------------------

function run(draft: AnyDraft) {
  const definition = definitionFromDraft(draft as ToolDraft);
  const serialized = serializeDraft(draft as ToolDraft);
  const issues = validateDraft(draft as ToolDraft).map((i) => [i.severity, i.where, i.message]);
  return { definition: JSON.stringify(definition), serialized, issues };
}

for (const { id, draft, needs } of SCENARIOS) {
  const norm = normaliser();
  const normDraft = norm(clone(draft)) as AnyDraft;
  const out = norm(run(draft));
  emit({ kind: 'draft', id, needs: needs ?? needsOf(draft), draft: normDraft, out });
}

{
  const rich = RICH();
  const norm = normaliser();
  const normDraft = norm(clone(rich));
  const renamed = renameParameterEverywhere(clone(rich) as ToolDraft, 'layers', 'depth');
  emit({ kind: 'rename', id: 'RICH layers → depth', needs: ['P4D333'], draft: normDraft, from: 'layers', to: 'depth', out: norm(renamed) });
  emit({ kind: 'find', id: 'RICH layers', needs: ['P4D333'], draft: normDraft, name: 'layers', out: findParameterReferences(rich as ToolDraft, 'layers') });
  emit({ kind: 'find', id: 'RICH ghost', needs: ['P4D333'], draft: normDraft, name: 'ghost', out: findParameterReferences(rich as ToolDraft, 'ghost') });
  const plain = base();
  const normPlain = normaliser();
  emit({ kind: 'rename', id: 'plain draft', needs: [], draft: normPlain(clone(plain)), from: 'a', to: 'b', out: normPlain(renameParameterEverywhere(clone(plain) as ToolDraft, 'a', 'b')) });
  const logOnly = base();
  logOnly.effects = [effect('log', { log: 'Log {{params.a}} {{params.a}}.' })];
  withParam(logOnly, 'a', 'string', 'x');
  const normLog = normaliser();
  emit({ kind: 'rename', id: 'a log line', needs: [], draft: normLog(clone(logOnly)), from: 'a', to: 'b', out: normLog(renameParameterEverywhere(clone(logOnly) as ToolDraft, 'a', 'b')) });
  emit({ kind: 'find', id: 'a log line', needs: [], draft: normLog(clone(logOnly)), name: 'a', out: findParameterReferences(logOnly as ToolDraft, 'a') });
  const briefOnly = withLlm('x');
  briefOnly.outcomes[1].brief = 'Use {{params.layers}}';
  const normBrief = normaliser();
  emit({ kind: 'rename', id: 'a brief', needs: [], draft: normBrief(clone(briefOnly)), from: 'layers', to: 'depth', out: normBrief(renameParameterEverywhere(clone(briefOnly) as ToolDraft, 'layers', 'depth')) });
  emit({ kind: 'find', id: 'a brief', needs: [], draft: normBrief(clone(briefOnly)), name: 'layers', out: findParameterReferences(briefOnly as ToolDraft, 'layers') });
}

// ---------------------------------------------------------------------------
// definition rows (all need P4.D333's parse)
// ---------------------------------------------------------------------------

const TOOL = {
  name: 'strike',
  description: 'Strike.',
  parameters: { weapon: { type: 'string', default: 'Excalibur' } },
  availableWhen: { inventory: { lantern: { eq: 'equipped' } }, inventoryCount: { ring: { gte: 1 } } },
  roll: { $inventory: 'sword.metadata.damage', fallback: '1d4' },
  effects: [
    { inventory: { action: 'equip', items: ['{{params.weapon}}'] } },
    { when: { outcome: { eq: 'failure' } }, inventory: { action: 'pick_up', items: ['magic bow'], equip: false } },
  ],
  outcomes: [
    { when: { inventory: { '{{params.weapon}}': { eq: 'equipped' } }, inventoryCount: { ring: { gte: 2 } } }, message: 'hit', state: 'success' },
    { when: true, message: 'miss', state: 'failure' },
  ],
};
const ANY_OF = {
  $schema: '/schemas/qtap-custom-tool.schema.json',
  name: 'tune',
  description: 'Tune.',
  parameters: { band: { type: 'string', default: 'low' } },
  availableWhen: [
    { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.metadata.toolVersion.max': { gte: 4 } } },
    { metadata: { a: { eq: true } }, inventoryAggregate: { 'equipped.names': { eq: 'ansible resonator' } } },
  ],
  effects: [{ when: [{ outcome: { eq: 'success' } }, { metadata: { lucky: { eq: true } } }], target: 'state.tuned', value: true }],
  outcomes: [
    {
      when: [{ gte: 0.9 }, { inventory: { 'lucky charm': { eq: 'equipped' } }, inventoryAggregate: { 'owned.titles': { contains: { $param: 'band' } } } }],
      message: 'tuned',
      state: 'success',
    },
    { when: true, message: 'static', state: 'info' },
  ],
};
const SCANNER = {
  name: 'entanglement_scan',
  description: 'Scan a shard for entanglement.',
  parameters: { layers: { type: 'number', default: 95 } },
  roll: { min: 1, max: 100 },
  llm: { prompt: READS, errorMessage: 'The link falters.' },
  outcomes: [
    { when: { roll: { gte: { $param: 'layers' } } }, message: '{{llm}}', state: 'success', brief: 'Confirmed at {{params.layers}} layers.' },
    { when: true, message: '{{llm}}', state: 'failure', brief: 'Not found.' },
  ],
};
const DEFINITIONS: Array<[string, unknown]> = [
  ['the inventory tool', TOOL],
  ['the any-of tool', ANY_OF],
  ['the brief scanner', SCANNER],
  ['a log effect, with and without a when', { name: 'x', description: 'x', effects: [{ log: 'Opened.' }, { when: { outcome: { eq: 'success' } }, log: 'It worked.' }], outcomes: [{ when: true, message: '-', state: 'info' }] }],
  ['two gate lines repeating a slot', { ...ANY_OF, availableWhen: [{ metadata: { rank: { gt: 3 } } }, { metadata: { rank: { gt: 5 } } }] }],
  ['a withheld any-of', { name: 'x', description: 'x', withheldWhen: [{ metadata: { a: { eq: 1 } } }, { progress: { 'cannon.percent': { gte: 50 } } }, { inventoryCount: { ring: { lt: 1 } } }], outcomes: [{ when: true, message: '-', state: 'info' }] }],
  ['an aggregate outcome with a fold word', { name: 'x', description: 'x', outcomes: [{ when: { inventoryAggregate: { 'owned.metadata.stats.max': { gte: 1 } } }, message: 'y', state: 'info' }, { when: true, message: '-', state: 'info' }] }],
  ['an aggregate against a number parameter', { name: 'x', description: 'x', parameters: { n: { type: 'number', default: 1 } }, outcomes: [{ when: { inventoryAggregate: { 'equipped.names': { contains: { $param: 'n' } } } }, message: 'y', state: 'info' }, { when: true, message: '-', state: 'info' }] }],
  ['a $inventory parameter default and range field', { name: 'x', description: 'x', parameters: { p: { type: 'number', default: { $inventory: 'sword.metadata.reach', fallback: 3 } } }, roll: { min: { $inventory: 'sword.metadata.reach', fallback: 1 }, max: 10 }, outcomes: [{ when: true, message: '-', state: 'info' }] }],
  ['a $inventory operand', { name: 'x', description: 'x', outcomes: [{ when: { gte: { $inventory: 'sword.metadata.reach', fallback: 2 } }, message: 'y', state: 'info' }, { when: true, message: '-', state: 'info' }] }],
  ['nine groups is refused by the parse', { name: 'x', description: 'x', outcomes: [{ when: Array.from({ length: 9 }, (_, i) => ({ gte: i })), message: 'y', state: 'info' }, { when: true, message: '-', state: 'info' }] }],
  ['eight groups', { name: 'x', description: 'x', outcomes: [{ when: Array.from({ length: 8 }, (_, i) => ({ gte: i })), message: 'y', state: 'info' }, { when: true, message: '-', state: 'info' }] }],
];
for (const [id, doc] of DEFINITIONS) {
  const draft = draftFromDefinition(doc);
  if (!draft) {
    emit({ kind: 'definition', id, needs: ['P4D333'], doc, out: null });
    continue;
  }
  const norm = normaliser();
  const normDraft = norm(clone(draft));
  const out = norm({ ...run(draft as AnyDraft) });
  emit({ kind: 'definition', id, needs: ['P4D333'], doc, draft: normDraft, out });
}

// ---------------------------------------------------------------------------
// direct calls
// ---------------------------------------------------------------------------

const direct = (id: string, fn: string, needs: Needs, args: unknown, out: unknown) =>
  emit({ kind: 'direct', id, fn, needs, args, out: JSON.stringify(out === undefined ? null : out) });

{
  const params = new Map<string, { type: string }>([
    ['n', { type: 'number' }],
    ['s', { type: 'string' }],
    ['b', { type: 'boolean' }],
  ]);
  const paramsObj = Object.fromEntries(params);
  const subjects: Array<[string, DraftCondition['subject']]> = [
    ['value', { kind: 'value' }],
    ['roll', { kind: 'roll' }],
    ['param n', { kind: 'param', name: 'n' }],
    ['param s', { kind: 'param', name: 's' }],
    ['param ghost', { kind: 'param', name: 'ghost' }],
    ['metadata', { kind: 'metadata', key: 'k' }],
    ['progress', { kind: 'progress', key: 'cannon.percent' }],
    ['llm', { kind: 'llm' }],
    ['llm-ok', { kind: 'llm-ok' }],
    ['inventory', { kind: 'inventory', key: 'sword' }],
    ['inventoryCount', { kind: 'inventoryCount', key: 'sword' }],
    ['aggregate count', agg('equipped.count')],
    ['aggregate names', agg('equipped.names')],
    ['aggregate titles', agg('owned.titles')],
    ['aggregate metadata list', agg('equipped.metadata.tags')],
    ['aggregate fold', agg('equipped.metadata.v.max')],
    ['aggregate bad', agg('equipped.bag')],
    ['aggregate padded', agg(' equipped.names ')],
    ['aggregate blank', agg('')],
  ];
  for (const [id, subject] of subjects) {
    const type = subjectValueType(subject, new Map(Object.entries(paramsObj)) as never);
    direct(`subjectValueType ${id}`, 'subjectValueType', [], { subject }, type);
    direct(`conditionSlotKey ${id}`, 'conditionSlotKey', [], { subject, comparator: 'gte' }, conditionSlotKey({ subject, comparator: 'gte' }));
  }
  for (const [id, key, comparator] of [
    ['plain', 'rank', 'gt'],
    ['padded', ' rank ', 'eq'],
    ['dotted', 'a.b', 'contains'],
  ] as Array<[string, string, string]>) {
    direct(`gateConditionSlotKey ${id}`, 'gateConditionSlotKey', [], { key, comparator }, gateConditionSlotKey({ key, comparator } as never));
  }
}
{
  const outcomes = (kinds: Array<'llm' | 'llm-ok' | 'value'>) => ({
    outcomes: kinds.map((kind) => ({ conditions: [{ subject: { kind } }] })),
  });
  direct('draftTableTestsLlm: none', 'draftTableTestsLlm', [], outcomes(['value']), draftTableTestsLlm(outcomes(['value']) as never));
  direct('draftTableTestsLlm: llm', 'draftTableTestsLlm', [], outcomes(['value', 'llm']), draftTableTestsLlm(outcomes(['value', 'llm']) as never));
  direct('draftTableTestsLlm: llm-ok', 'draftTableTestsLlm', [], outcomes(['llm-ok']), draftTableTestsLlm(outcomes(['llm-ok']) as never));
  direct('draftTableTestsLlm: empty', 'draftTableTestsLlm', [], { outcomes: [] }, draftTableTestsLlm({ outcomes: [] } as never));

  const pb = (llmEnabled: boolean, llmPrompt: string) => ({ llmEnabled, llmPrompt });
  for (const [id, args] of [
    ['reads the brief', pb(true, 'say {{outcome.brief}}')],
    ['reads the brief, padded', pb(true, 'say {{ outcome.brief }}')],
    ['reads the state only', pb(true, 'say {{outcome.state}}')],
    ['consult off', pb(false, 'say {{outcome.brief}}')],
    ['reads nothing', pb(true, 'say hi')],
  ] as Array<[string, { llmEnabled: boolean; llmPrompt: string }]>) {
    direct(`promptReadsBrief: ${id}`, 'promptReadsBrief', ['P4D333'], args, promptReadsBrief(args));
  }
  const reads = { llmEnabled: true, llmPrompt: 'say {{outcome.brief}}', outcomes: [{ brief: '' }, { brief: 'x' }, { brief: '   ' }] };
  for (const [id, hits] of [
    ['mixed', [{ index: 0, hits: 12 }, { index: 1, hits: 5 }, { index: 2, hits: 7 }]],
    ['unreachable rows are not nagged', [{ index: 0, hits: 0 }, { index: 1, hits: 1000 }, { index: 2, hits: 0 }]],
    ['an index past the table', [{ index: 9, hits: 3 }]],
    ['no hits', []],
  ] as Array<[string, Array<{ index: number; hits: number }>]>) {
    direct(`reachableOutcomesMissingBrief: ${id}`, 'reachableOutcomesMissingBrief', ['P4D333'], { draft: reads, hits }, reachableOutcomesMissingBrief(reads as never, hits));
  }
  const quiet = { ...reads, llmPrompt: 'say hi' };
  direct('reachableOutcomesMissingBrief: a prompt that reads none', 'reachableOutcomesMissingBrief', [], { draft: quiet, hits: [{ index: 0, hits: 4 }] }, reachableOutcomesMissingBrief(quiet as never, [{ index: 0, hits: 4 }]));
}
