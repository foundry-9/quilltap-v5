import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { P4D333_SPA_LANDED } from '../../../testing/p4d334-gates';
import {
  newDraft,
  validateDraft,
  type DraftCondition,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { OutcomesSection, describeSlot, subjectSelectValue } from './outcomes-section';

/**
 * OutcomesSection at the pin `322a5488e` — the parts the round moved: chips
 * drawn as lines, the three Inventory subjects, the aggregate key box, the
 * `$inventory` operand pill and the narrator's brief (v4 `a53024d3c`,
 * `e1872c94f`, `322a5488e`; the cases are `workbench-inventory.test.tsx`'s
 * OutcomesSection rows plus this lane's own).
 *
 * v4's cases load a document through `draftFromDefinition`, which reads
 * P4.D333's twin of the schema — each of those is gated on `P4D333_SPA_LANDED`
 * and has an UNGATED twin that builds the same draft by hand.
 */

let id = 0;
const chip = (over: Partial<DraftCondition> & Pick<DraftCondition, 'subject'>): DraftCondition => ({
  id: `c-${++id}`,
  comparator: 'eq',
  operand: { kind: 'string', text: 'equipped' },
  ...over,
});

function draftWith(conditions: DraftCondition[], mutate: (d: ToolDraft) => void = () => undefined): ToolDraft {
  const draft = newDraft();
  draft.name = 'strike';
  draft.description = 'Strike.';
  draft.outcomes[0].message = 'hit';
  draft.outcomes[0].conditions = conditions;
  mutate(draft);
  return draft;
}

async function render(draft: ToolDraft): Promise<{
  fixture: ComponentFixture<OutcomesSection>;
  emitted: ToolDraft[];
}> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutcomesSection] });
  const fixture = TestBed.createComponent(OutcomesSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  const emitted: ToolDraft[] = [];
  fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));
  fixture.detectChanges();
  return { fixture, emitted };
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => (el(f).textContent ?? '').replace(/\s+/g, ' ');
const change = (select: HTMLSelectElement, value: string) => {
  select.value = value;
  select.dispatchEvent(new Event('change'));
};
const typeInto = (input: HTMLInputElement | HTMLTextAreaElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input'));
};
const subjectSelect = (f: ComponentFixture<unknown>) =>
  el(f).querySelector<HTMLSelectElement>('select[aria-label="Condition subject"]')!;

describe('describeSlot — all ten sentences, and "a line" not "a row"', () => {
  const sentence = (subject: DraftCondition['subject']) => describeSlot({ subject, comparator: 'gte' });
  it.each([
    [{ kind: 'value' }, 'A line can test ≥ on the value only once.'],
    [{ kind: 'roll' }, 'A line can test ≥ on the raw roll only once.'],
    [{ kind: 'param', name: 'p' }, 'A line can test ≥ on "p" only once.'],
    [{ kind: 'metadata', key: 'k' }, 'A line can test ≥ on metadata "k" only once.'],
    [{ kind: 'progress', key: 'cannon.percent' }, 'A line can test ≥ on progress "cannon.percent" only once.'],
    [{ kind: 'llm' }, "A line can test ≥ on the consult's answer only once."],
    [{ kind: 'llm-ok' }, 'A line can test whether the consult succeeded only once.'],
    [{ kind: 'inventory', key: 'sword' }, 'A line can test ≥ on the state of "sword" only once.'],
    [{ kind: 'inventoryCount', key: 'sword' }, 'A line can test ≥ on the equipped count of "sword" only once.'],
    [{ kind: 'inventoryAggregate', key: 'equipped.count' }, 'A line can test ≥ on the aggregate "equipped.count" only once.'],
  ] as Array<[DraftCondition['subject'], string]>)('%j', (subject, expected) => {
    expect(sentence(subject)).toBe(expected);
  });

  it('subjectSelectValue maps each subject', () => {
    expect(subjectSelectValue({ kind: 'inventory', key: 'x' })).toBe('inventory');
    expect(subjectSelectValue({ kind: 'inventoryCount', key: 'x' })).toBe('inventoryCount');
    expect(subjectSelectValue({ kind: 'inventoryAggregate', key: 'x' })).toBe('inventoryAggregate');
    expect(subjectSelectValue({ kind: 'param', name: 'p' })).toBe('param:p');
  });
});

describe('OutcomesSection — the section copy', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('says rows hold by line', async () => {
    const { fixture } = await render(draftWith([]));
    expect(text(fixture)).toContain(
      'Checked top to bottom — the first row that holds wins. Tests on one line must all hold; any line of a row will do.',
    );
  });
});

describe('OutcomesSection — chips drawn as lines', () => {
  afterEach(() => TestBed.resetTestingModule());

  const TWO_LINES = () => [
    chip({ subject: { kind: 'value' }, comparator: 'gte', operand: { kind: 'number', text: '1' } }),
    chip({ subject: { kind: 'roll' }, comparator: 'lt', operand: { kind: 'number', text: '4' } }),
    chip({ subject: { kind: 'value' }, comparator: 'gte', operand: { kind: 'number', text: '2' }, joiner: 'or' }),
  ];

  it('draws an any-of row as lines and flips a joiner', async () => {
    const { fixture, emitted } = await render(draftWith(TWO_LINES()));
    const toggles = [...el(fixture).querySelectorAll<HTMLButtonElement>('qt-joiner-toggle button')];
    expect(toggles.map((b) => b.textContent?.trim())).toEqual(['AND', 'OR']);
    toggles[1].click();
    expect(emitted[0].outcomes[0].conditions.map((c) => c.joiner ?? 'and')).toEqual(['and', 'and', 'and']);
  });

  it('lets another line repeat a slot, refuses a repeat on the same line with the "line" sentence', async () => {
    const conditions = TWO_LINES();
    const { fixture, emitted } = await render(draftWith(conditions));
    // chip 3 is value ≥ on line 2; chip 1 is value ≥ on line 1 — fine.
    const comparators = el(fixture).querySelectorAll<HTMLSelectElement>('select[aria-label="Comparator"]');
    change(comparators[2], 'gt');
    expect(emitted).toHaveLength(1);
    // Fold line 2 into line 1 and the same edit collides.
    const merged = TWO_LINES();
    merged[2] = { ...merged[2], joiner: 'and', comparator: 'gt' };
    const second = await render(draftWith(merged));
    const sel = el(second.fixture).querySelectorAll<HTMLSelectElement>('select[aria-label="Comparator"]');
    change(sel[2], 'gte');
    second.fixture.detectChanges();
    expect(second.emitted).toHaveLength(0);
    expect(text(second.fixture)).toContain('A line can test ≥ on the value only once.');
  });

  it('puts a fresh chip on the last line’s free slot', async () => {
    const { fixture, emitted } = await render(draftWith(TWO_LINES()));
    const add = [...el(fixture).querySelectorAll<HTMLButtonElement>('button')].find(
      (b) => b.textContent?.trim() === 'add condition',
    )!;
    add.click();
    const chips = emitted[0].outcomes[0].conditions;
    // Line 2 holds only value ≥, so the first free slot is the value ≥ …'s successor on that line.
    expect(chips).toHaveLength(4);
    expect(chips[3]).toMatchObject({ subject: { kind: 'value' }, comparator: 'gt' });
  });

  it('draws an any-of row as lines (gated twin loading through the parse)', async () => {
    if (!P4D333_SPA_LANDED) return;
    const { draftFromDefinition } = await import('../../pascal/tool-draft');
    const draft = draftFromDefinition({
      name: 'x',
      description: 'x',
      outcomes: [{ when: [{ gte: 1 }, { lt: 3 }], message: 'm', state: 'info' }, { when: true, message: '-', state: 'info' }],
    })!;
    const { fixture } = await render(draft);
    expect(el(fixture).querySelectorAll('qt-joiner-toggle')).toHaveLength(1);
  });
});

describe('OutcomesSection — Inventory subjects', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers Inventory state…, count… and aggregate… after Progress… and before the consult pair', async () => {
    const { fixture } = await render(draftWith([chip({ subject: { kind: 'value' } })], (d) => (d.llmEnabled = true)));
    const options = [...subjectSelect(fixture).options];
    const values = options.map((o) => o.value);
    expect(values.slice(values.indexOf('progress'), values.indexOf('progress') + 5)).toEqual([
      'progress',
      'inventory',
      'inventoryCount',
      'inventoryAggregate',
      'llm',
    ]);
    const byValue = (v: string) => options.find((o) => o.value === v)!;
    expect(byValue('inventory').textContent?.trim()).toBe('Inventory state…');
    expect(byValue('inventory').title).toBe(
      "An object's state for the rolling character: equipped, carried, reachable or absent",
    );
    expect(byValue('inventoryCount').title).toBe("How many of an object's kind the rolling character has equipped");
    expect(byValue('inventoryAggregate').title).toBe('One attribute across everything equipped, owned or reachable');
  });

  it('renders a state chip with the four-way select and = / ≠ only', async () => {
    const { fixture } = await render(
      draftWith([chip({ subject: { kind: 'inventory', key: 'Excalibur' } })]),
    );
    const states = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
    expect([...states.options].map((o) => o.value)).toEqual(['equipped', 'carried', 'reachable', 'absent']);
    expect(
      [...el(fixture).querySelectorAll<HTMLOptionElement>('select[aria-label="Comparator"] option')].map((o) => o.value),
    ).toEqual(['eq', 'neq']);
    const key = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Inventory object reference"]')!;
    expect(key.getAttribute('placeholder')).toBe('Excalibur, or {{params.weapon}}');
    expect(key.getAttribute('title')).toBe(
      'The object, by id, title or name — a template, so {{params.weapon}} lets a parameter choose. A run nobody made declines the test.',
    );
    expect(key.value).toBe('Excalibur');
  });

  it('an inventory count chip keeps the ordering ladder and a number box', async () => {
    const { fixture } = await render(
      draftWith([
        chip({ subject: { kind: 'inventoryCount', key: 'ring' }, comparator: 'gte', operand: { kind: 'number', text: '2' } }),
      ]),
    );
    expect(el(fixture).querySelector('input[aria-label="Inventory object reference"]')).not.toBeNull();
    expect(el(fixture).querySelector('input[aria-label="Operand number"]')).not.toBeNull();
    expect(el(fixture).querySelector('select[aria-label="Inventory state"]')).toBeNull();
  });

  it('offers the aggregate subject on an outcome chip, with every comparator', async () => {
    const { fixture } = await render(
      draftWith([
        chip({
          subject: { kind: 'inventoryAggregate', key: 'equipped.names' },
          comparator: 'contains',
          operand: { kind: 'string', text: 'x' },
        }),
      ]),
    );
    expect(
      [...el(fixture).querySelectorAll<HTMLOptionElement>('select[aria-label="Comparator"] option')].map((o) => o.value),
    ).toEqual(['gt', 'gte', 'lt', 'lte', 'eq', 'neq', 'contains', 'ncontains']);
    expect(el(fixture).querySelector('qt-aggregate-key-field')).not.toBeNull();
  });

  it('fills the key box from the Insert-key helper, keeping a typed metadata key', async () => {
    const { fixture, emitted } = await render(
      draftWith([
        chip({
          subject: { kind: 'inventoryAggregate', key: 'equipped.metadata.toolVersion' },
          comparator: 'gte',
          operand: { kind: 'number', text: '1' },
        }),
      ]),
    );
    change(
      el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Insert an aggregate key"]')!,
      'owned.metadata.<key>.max',
    );
    expect(emitted[0].outcomes[0].conditions[0].subject).toEqual({
      kind: 'inventoryAggregate',
      key: 'owned.metadata.toolVersion.max',
    });
  });

  it('re-legalises a subject change: the inventory arm first (= / ≠ and a state), then the type', async () => {
    const { fixture, emitted } = await render(
      draftWith([chip({ subject: { kind: 'value' }, comparator: 'gte', operand: { kind: 'number', text: '3' } })]),
    );
    change(subjectSelect(fixture), 'inventory');
    expect(emitted[0].outcomes[0].conditions[0]).toMatchObject({
      subject: { kind: 'inventory', key: '' },
      comparator: 'eq',
      operand: { kind: 'string', text: 'equipped' },
    });
  });

  it('keeps the key moving between inventory and inventoryCount, and an aggregate keeps an aggregate key', async () => {
    const { fixture, emitted } = await render(draftWith([chip({ subject: { kind: 'inventory', key: 'ring' } })]));
    change(subjectSelect(fixture), 'inventoryCount');
    expect(emitted[0].outcomes[0].conditions[0].subject).toEqual({ kind: 'inventoryCount', key: 'ring' });
  });

  it('shows a $inventory operand as a read-only pill, with its own title, and no type picker', async () => {
    const { fixture } = await render(
      draftWith([
        chip({
          subject: { kind: 'metadata', key: 'reach' },
          comparator: 'gte',
          operand: { kind: 'inventory', path: 'sword.metadata.reach', fallback: 2 },
        }),
      ]),
    );
    const pill = [...el(fixture).querySelectorAll<HTMLElement>('span.font-mono')].find((s) =>
      s.textContent?.includes('$inventory'),
    )!;
    expect(pill.textContent?.trim()).toBe('$inventory: sword.metadata.reach → 2');
    expect(pill.getAttribute('title')).toBe(
      'This operand draws from an object the rolling character carries. Edit $inventory references in the raw JSON.',
    );
    expect(pill.getAttribute('class')).toBe('qt-text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1');
    expect(el(fixture).querySelector('select[aria-label="Literal type"]')).toBeNull();
  });

  it('gives an aggregate chip with an unknowable type the literal-type picker', async () => {
    const { fixture } = await render(
      draftWith([
        chip({
          subject: { kind: 'inventoryAggregate', key: 'equipped.metadata.tag' },
          comparator: 'eq',
          operand: { kind: 'string', text: 'x' },
        }),
      ]),
    );
    expect(el(fixture).querySelector('select[aria-label="Literal type"]')).not.toBeNull();
  });

  it('a number-valued aggregate has a known type, so no picker', async () => {
    const { fixture } = await render(
      draftWith([
        chip({
          subject: { kind: 'inventoryAggregate', key: 'equipped.count' },
          comparator: 'gte',
          operand: { kind: 'number', text: '1' },
        }),
      ]),
    );
    expect(el(fixture).querySelector('select[aria-label="Literal type"]')).toBeNull();
  });
});

describe('OutcomesSection — the narrator’s brief', () => {
  afterEach(() => TestBed.resetTestingModule());

  const brief = (f: ComponentFixture<unknown>) =>
    el(f).querySelector<HTMLTextAreaElement>('textarea[aria-label="Outcome brief for the LLM consult"]');

  it('is absent with the consult off and a blank brief; present with text or with the consult on', async () => {
    expect(brief((await render(draftWith([]))).fixture)).toBeNull();
    expect(brief((await render(draftWith([], (d) => (d.outcomes[0].brief = 'x')))).fixture)).not.toBeNull();
    expect(brief((await render(draftWith([], (d) => (d.llmEnabled = true)))).fixture)).not.toBeNull();
  });

  it('tests the brief UNTRIMMED: a whitespace brief shows the box', async () => {
    expect(brief((await render(draftWith([], (d) => (d.outcomes[0].brief = ' ')))).fixture)).not.toBeNull();
  });

  it('carries v4’s strings, rows and class', async () => {
    const { fixture } = await render(draftWith([], (d) => (d.llmEnabled = true)));
    const box = brief(fixture)!;
    expect(box.getAttribute('class')).toBe('qt-textarea w-full text-sm');
    expect(box.rows).toBe(2);
    expect(box.maxLength).toBe(1000);
    expect(box.getAttribute('placeholder')).toBe(
      "Brief for the consult's narrator — the verdict this row means, e.g. “Report that the signal is definitively confirmed.” Quote it in the prompt as {{outcome.brief}}.",
    );
    expect(text(fixture)).toContain(
      'Optional. When no row tests the consult’s answer, the table is dealt first and the model is simply told this, rather than asked to work the verdict out for itself.',
    );
  });

  it('writes the brief to the draft', async () => {
    const { fixture, emitted } = await render(draftWith([], (d) => (d.llmEnabled = true)));
    typeInto(brief(fixture)!, 'Confirmed.');
    expect(emitted[0].outcomes[0].brief).toBe('Confirmed.');
  });

  it('shows a brief error under its row', async () => {
    const withErr = draftWith([], (d) => {
      d.llmEnabled = false;
      d.outcomes[0].brief = 'b';
    });
    expect(text((await render(withErr)).fixture)).toContain('has a brief, but the LLM consult is not enabled to read it');
  });

  // The warning reads `{{outcome.brief}}` through the `outcome` classify arm — P4.D333's.
  it.skipIf(!P4D333_SPA_LANDED)('shows the missing-brief warning under its row', async () => {
    const withWarn = draftWith([], (d) => {
      d.llmEnabled = true;
      d.llmPrompt = 'Say {{outcome.brief}}.';
      d.outcomes[0].brief = '';
    });
    expect(text((await render(withWarn)).fixture)).toContain(
      '⚠ the consult prompt reads {{outcome.brief}}, but this outcome has no brief — if it wins, the placeholder reaches the model as written',
    );
  });

  it('seeds a new row with a blank brief', async () => {
    const { fixture, emitted } = await render(draftWith([]));
    const add = [...el(fixture).querySelectorAll<HTMLButtonElement>('button')].find((b) =>
      b.textContent?.includes('Add outcome'),
    )!;
    add.click();
    expect(emitted[0].outcomes.at(-2)!.brief).toBe('');
  });
});
