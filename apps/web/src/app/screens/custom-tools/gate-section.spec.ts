import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import {
  newDraft,
  validateDraft,
  type DraftGateCondition,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { GateSection, comparatorsFor, withSubject } from './gate-section';

/**
 * GateSection — asserted against v4 `components/custom-tools/GateSection.tsx`.
 * The load-bearing claims: three exclusive modes with v4's exact labels and
 * titles, chips that SURVIVE a mode change, every comparator always on offer
 * (a metadata key's stored type is unknowable here), the operand re-seated into
 * a widget the new comparator can use, and a duplicate key+comparator pair
 * blocked at the door rather than silently overwriting its twin.
 */

function gatedDraft(conditions: DraftGateCondition[] = []): ToolDraft {
  const draft = newDraft();
  draft.name = 'reprogram';
  draft.description = 'd';
  draft.gateMode = 'available';
  draft.gateConditions = conditions;
  return draft;
}

const chip = (over: Partial<DraftGateCondition> = {}): DraftGateCondition => ({
  id: 'g1',
  subject: 'metadata',
  key: 'rank',
  comparator: 'eq',
  operand: { kind: 'boolean', value: true },
  ...over,
});

async function render(draft: ToolDraft): Promise<ComponentFixture<GateSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [GateSection] });
  const fixture = TestBed.createComponent(GateSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function el(fixture: ComponentFixture<unknown>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

describe('GateSection (v4 GateSection.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers the three modes with v4’s labels and titles, one checked', async () => {
    const fixture = await render(gatedDraft([chip()]));
    const radios = [...el(fixture).querySelectorAll('[role="radio"]')];
    expect(radios.map((r) => r.textContent?.trim())).toEqual([
      'Anyone',
      'Only show if…',
      'Do not show if…',
    ]);
    expect(radios.map((r) => r.getAttribute('title'))).toEqual([
      'Every character in the scene is offered this tool',
      'Offer it only to a character whose fact sheet passes every test below',
      'Withhold it from a character whose fact sheet passes every test below',
    ]);
    expect(radios.map((r) => r.getAttribute('aria-checked'))).toEqual(['false', 'true', 'false']);
  });

  it('says who is offered the tool while the mode is Anyone, and shows no chips', async () => {
    const draft = gatedDraft([chip()]);
    draft.gateMode = 'none';
    const fixture = await render(draft);
    expect(text(fixture)).toContain('Every character in the scene is offered this tool.');
    expect(el(fixture).querySelectorAll('qt-gate-chip')).toHaveLength(0);
  });

  it('keeps the chips when the mode changes — flipping the clause is the commonest edit', async () => {
    const fixture = await render(gatedDraft([chip()]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    el(fixture).querySelectorAll<HTMLButtonElement>('[role="radio"]')[2].click();

    expect(next?.gateMode).toBe('withheld');
    expect(next?.gateConditions).toHaveLength(1);
  });

  it('renders the clause-specific lead line', async () => {
    expect(text(await render(gatedDraft([chip()])))).toContain(
      'Offered only to a character whose sheet passes the tests below:',
    );
    const withheld = gatedDraft([chip()]);
    withheld.gateMode = 'withheld';
    expect(text(await render(withheld))).toContain(
      'Withheld from a character whose sheet passes the tests below:',
    );
  });

  it('joins chips with AND, from the second onwards', async () => {
    const fixture = await render(
      gatedDraft([chip(), chip({ id: 'g2', key: 'cleared' }), chip({ id: 'g3', key: 'house' })]),
    );
    expect(text(fixture).match(/AND/g)).toHaveLength(2);
  });

  it('offers all eight comparators — a metadata key’s stored type is unknowable', async () => {
    const fixture = await render(gatedDraft([chip()]));
    const options = [
      ...el(fixture).querySelectorAll('select[aria-label="Comparator"] option'),
    ].map((o) => (o as HTMLOptionElement).value);
    expect(options).toEqual(['gt', 'gte', 'lt', 'lte', 'eq', 'neq', 'contains', 'ncontains']);
  });

  it('shows the literal-type picker for eq/neq only', async () => {
    const eq = await render(gatedDraft([chip({ comparator: 'eq' })]));
    expect(el(eq).querySelector('select[aria-label="Literal type"]')).not.toBeNull();

    const ordering = await render(
      gatedDraft([chip({ comparator: 'gte', operand: { kind: 'number', text: '3' } })]),
    );
    expect(el(ordering).querySelector('select[aria-label="Literal type"]')).toBeNull();

    const containment = await render(
      gatedDraft([chip({ comparator: 'contains', operand: { kind: 'string', text: 'a' } })]),
    );
    expect(el(containment).querySelector('select[aria-label="Literal type"]')).toBeNull();
  });

  it('carries the fail-soft ⓘ on ordering and containment, with the matching sentence', async () => {
    const ordering = await render(
      gatedDraft([chip({ comparator: 'gte', operand: { kind: 'number', text: '3' } })]),
    );
    expect(el(ordering).querySelector('span[title*="the stored value is a number"]')).not.toBeNull();

    const containment = await render(
      gatedDraft([chip({ comparator: 'contains', operand: { kind: 'string', text: 'a' } })]),
    );
    expect(el(containment).querySelector('span[title*="the stored value is text"]')).not.toBeNull();

    const eq = await render(gatedDraft([chip()]));
    expect(el(eq).querySelector('span[title*="fail-soft"]')).toBeNull();
  });

  it('marks the chip a validator issue names, and prints the section-level error', async () => {
    // A gate whose only condition has a blank key: one chip-level issue, and the
    // section-level "must test something" beside it.
    const fixture = await render(gatedDraft([chip({ key: '' })]));
    expect(text(fixture)).toContain('the gate must test something');
    expect(fixture.componentInstance.conditionErrorIds().has('g1')).toBe(true);
    expect(el(fixture).querySelector('.qt-input-error')).not.toBeNull();
  });

  it('adds a chip seeded as an eq/true test with a blank key', async () => {
    const fixture = await render(gatedDraft([]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    [...el(fixture).querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.textContent?.includes('add condition'))!
      .click();

    expect(next?.gateConditions).toHaveLength(1);
    expect(next?.gateConditions[0]).toMatchObject({
      key: '',
      comparator: 'eq',
      operand: { kind: 'boolean', value: true },
    });
  });

  it('blocks a duplicate key+comparator slot and says so, emitting nothing', async () => {
    const conditions = [
      chip({ id: 'g1', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } }),
      chip({ id: 'g2', key: 'house', comparator: 'gte', operand: { kind: 'number', text: '1' } }),
    ];
    const fixture = await render(gatedDraft(conditions));
    let emitted = 0;
    fixture.componentInstance.draftChange.subscribe(() => (emitted += 1));

    // Retype g2's key onto g1's — same key, same comparator.
    const keys = el(fixture).querySelectorAll<HTMLInputElement>('input[aria-label="Metadata key"]');
    keys[1].value = 'rank';
    keys[1].dispatchEvent(new Event('input'));
    fixture.detectChanges();

    expect(emitted).toBe(0);
    expect(fixture.componentInstance.duplicateNotice()).toBe(
      'A line of the gate can test “rank” with ≥ only once.',
    );
    expect(text(fixture)).toContain('A line of the gate can test “rank” with ≥ only once.');
  });

  it('re-seats the operand when the comparator can no longer use it', async () => {
    const fixture = await render(gatedDraft([chip({ operand: { kind: 'boolean', value: true } })]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    const comparator = el(fixture).querySelector<HTMLSelectElement>(
      'select[aria-label="Comparator"]',
    )!;
    comparator.value = 'gte';
    comparator.dispatchEvent(new Event('change'));

    expect(next?.gateConditions[0].operand).toEqual({ kind: 'number', text: '' });
  });

  it('carries a typed number across into the containment text box, and back', async () => {
    const fixture = await render(
      gatedDraft([chip({ comparator: 'gte', operand: { kind: 'number', text: '42' } })]),
    );
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    const comparator = el(fixture).querySelector<HTMLSelectElement>(
      'select[aria-label="Comparator"]',
    )!;
    comparator.value = 'contains';
    comparator.dispatchEvent(new Event('change'));

    expect(next?.gateConditions[0].operand).toEqual({ kind: 'string', text: '42' });
  });

  it('deletes a chip', async () => {
    const fixture = await render(gatedDraft([chip(), chip({ id: 'g2', key: 'house' })]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    el(fixture)
      .querySelectorAll<HTMLButtonElement>('button[aria-label="Remove this condition"]')[0]
      .click();

    expect(next?.gateConditions.map((c) => c.id)).toEqual(['g2']);
  });

  it('explains the asymmetry the two clauses exist for', async () => {
    expect(text(await render(gatedDraft([chip()])))).toContain(
      'A key the character lacks never matches.',
    );
  });
});

/**
 * The gate's second subject (v4 `0587d1e96`). Every string below is v4's, and
 * the two `title` attributes are the ones an author actually reads on hover.
 */
describe('GateSection — the progress subject', () => {
  const subjectSelect = (f: ComponentFixture<GateSection>) =>
    el(f).querySelector<HTMLSelectElement>('select[aria-label="Gate subject"]')!;

  it('offers the five subjects, metadata first', async () => {
    const fixture = await render(gatedDraft([chip()]));
    const select = subjectSelect(fixture);
    expect(Array.from(select.options).map((o) => o.value)).toEqual([
      'metadata',
      'progress',
      'inventory',
      'inventoryCount',
      'inventoryAggregate',
    ]);
    expect(Array.from(select.options).map((o) => o.textContent?.trim())).toEqual([
      'metadata',
      'progress',
      'inventory state',
      'inventory count',
      'inventory aggregate',
    ]);
    expect(select.title).toBe(
      "Which sheet this test reads: the character's own metadata.json, their derived progressions, or what they carry.",
    );
  });

  it('shows the chip’s own subject as selected', async () => {
    const fixture = await render(gatedDraft([chip({ subject: 'progress', key: 'cannon.complete' })]));
    expect(subjectSelect(fixture).value).toBe('progress');
  });

  it('carries a subject change into the draft, leaving the key alone', async () => {
    const fixture = await render(gatedDraft([chip()]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    const select = subjectSelect(fixture);
    select.value = 'progress';
    select.dispatchEvent(new Event('change'));

    expect(next?.gateConditions[0].subject).toBe('progress');
    expect(next?.gateConditions[0].key).toBe('rank');
  });

  it('re-labels the key input for a progress chip', async () => {
    const metadata = el(await render(gatedDraft([chip()]))).querySelector('input[type="text"]')!;
    expect(metadata.getAttribute('aria-label')).toBe('Metadata key');
    expect(metadata.getAttribute('placeholder')).toBe("key on the character's fact sheet");
    expect(metadata.getAttribute('title')).toBe(
      "The invoking character's fact sheet — a key the character lacks never matches.",
    );

    const progress = el(
      await render(gatedDraft([chip({ subject: 'progress', key: 'cannon.complete' })])),
    ).querySelector('input[type="text"]')!;
    expect(progress.getAttribute('aria-label')).toBe('Progress key');
    expect(progress.getAttribute('placeholder')).toBe('cannon.complete');
    expect(progress.getAttribute('title')).toBe(
      'A progression of the invoking character, keyed "<id>.<field>" — percent, complete, ' +
        'remainingMs, state, started, elapsedMs, startTime, endTime, name, elapsed, remaining, ' +
        'quantity. A progression they do not carry never matches.',
    );
  });

  it('seeds a new chip on metadata — what a gate has always meant', async () => {
    const fixture = await render(gatedDraft([]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('button'))
      .find((b) => b.textContent?.trim() === 'add condition')!
      .click();

    expect(next?.gateConditions).toHaveLength(1);
    expect(next?.gateConditions[0].subject).toBe('metadata');
  });

  it('names the sheets in the card’s hint', async () => {
    expect(text(await render(gatedDraft([chip()]))).replace(/\s+/g, ' ')).toContain(
      'Tested before the deal, against the invoking character’s metadata.json, their timed ' +
        'progressions, or what they carry in this chat — the only things known before a roll exists.',
    );
  });
});


// ===========================================================================
// P4.D334 — lines, the Inventory subjects and the aggregate key (v4 a53024d3c,
// e1872c94f at the pin).
// ===========================================================================

const typeText = (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input'));
};

describe('GateSection — chips drawn as lines (v4 a53024d3c)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const TWO_LINES = () => [
    chip({ id: 'g1', key: 'a' }),
    chip({ id: 'g2', key: 'b' }),
    chip({ id: 'g3', key: 'a', joiner: 'or' }),
    chip({ id: 'g4', key: 'c' }),
  ];

  it('draws an any-of gate as lines and flips a joiner (hand-built twin)', async () => {
    const fixture = await render(gatedDraft(TWO_LINES()));
    const toggles = [...el(fixture).querySelectorAll<HTMLButtonElement>('qt-joiner-toggle button')];
    expect(toggles.map((b) => b.textContent?.trim())).toEqual(['AND', 'OR', 'AND']);
    // The second line carries the hairline; the first carries no class.
    const lines = [...el(fixture).querySelectorAll('qt-condition-groups > div')];
    expect(lines).toHaveLength(2);
    expect(lines[0].hasAttribute('class')).toBe(false);
    expect(lines[1].getAttribute('class')).toBe('border-t qt-border-default pt-1 mt-1');

    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));
    toggles[1].click();
    expect(next?.gateConditions.map((c) => c.joiner ?? 'and')).toEqual(['and', 'and', 'and', 'and']);
    expect(toggles[1].getAttribute('aria-label')).toBe('Joined by OR — switch to AND');
  });

  it('clears the duplicate notice when a joiner flips', async () => {
    const fixture = await render(
      gatedDraft([
        chip({ id: 'g1', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } }),
        chip({ id: 'g2', key: 'house', comparator: 'gte', operand: { kind: 'number', text: '1' } }),
      ]),
    );
    const keys = el(fixture).querySelectorAll<HTMLInputElement>('input[aria-label="Metadata key"]');
    typeText(keys[1], 'rank');
    fixture.detectChanges();
    expect(fixture.componentInstance.duplicateNotice()).not.toBeNull();
    el(fixture).querySelector<HTMLButtonElement>('qt-joiner-toggle button')!.click();
    fixture.detectChanges();
    expect(fixture.componentInstance.duplicateNotice()).toBeNull();
  });

  it('allows the same slot on another line, and across subjects', async () => {
    const fixture = await render(
      gatedDraft([
        chip({ id: 'g1', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } }),
        chip({ id: 'g2', key: 'house', comparator: 'gte', operand: { kind: 'number', text: '1' }, joiner: 'or' }),
        chip({ id: 'g3', subject: 'inventoryCount', key: 'rank', comparator: 'lt', operand: { kind: 'number', text: '1' }, joiner: 'or' }),
      ]),
    );
    let emitted = 0;
    fixture.componentInstance.draftChange.subscribe(() => (emitted += 1));
    // g2 takes g1's slot — but they sit on different lines.
    const keys = el(fixture).querySelectorAll<HTMLInputElement>('input[aria-label="Metadata key"]');
    typeText(keys[1], 'rank');
    expect(emitted).toBe(1);
    expect(fixture.componentInstance.duplicateNotice()).toBeNull();
  });

  it('refuses the same slot on one line, only within the same subject', async () => {
    const fixture = await render(
      gatedDraft([
        chip({ id: 'g1', key: 'rank', comparator: 'gte', operand: { kind: 'number', text: '3' } }),
        chip({ id: 'g2', subject: 'inventoryCount', key: 'rank', comparator: 'lt', operand: { kind: 'number', text: '1' } }),
      ]),
    );
    let emitted = 0;
    fixture.componentInstance.draftChange.subscribe(() => (emitted += 1));
    // Same key and comparator, but another subject: allowed.
    const comparators = el(fixture).querySelectorAll<HTMLSelectElement>('select[aria-label="Comparator"]');
    comparators[1].value = 'gte';
    comparators[1].dispatchEvent(new Event('change'));
    expect(emitted).toBe(1);
  });

  it('states the line hint before the missing-key hint', async () => {
    const hints = [...el(await render(gatedDraft([chip()]))).querySelectorAll('p.qt-hint')].map((p) =>
      p.textContent?.replace(/\s+/g, ' ').trim(),
    );
    expect(hints[1]).toBe('Tests on one line must all hold; any line that holds opens the door.');
    expect(hints[2]).toMatch(/^A key the character lacks never matches\./);
  });

  it('draws an any-of gate as lines and flips a joiner', async () => {
    const { draftFromDefinition } = await import('../../pascal/tool-draft');
    const draft = draftFromDefinition({
      name: 'tune',
      description: 'Tune.',
      availableWhen: [{ metadata: { a: { eq: true } } }, { metadata: { b: { eq: true } } }],
      outcomes: [{ when: true, message: '-', state: 'info' }],
    })!;
    const fixture = await render(draft);
    expect(el(fixture).querySelectorAll('qt-joiner-toggle')).toHaveLength(1);
  });
});

describe('GateSection — the Inventory subjects (v4 e1872c94f, a53024d3c)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const aggChip = (over: Partial<DraftGateCondition> = {}) =>
    chip({ subject: 'inventoryAggregate', key: 'owned.names', comparator: 'contains', operand: { kind: 'string', text: 'x' }, ...over });

  it('comparatorsFor: a state is = / ≠, a count is the ordering ladder, the rest offer everything', () => {
    expect(comparatorsFor('inventory')).toEqual(['eq', 'neq']);
    expect(comparatorsFor('inventoryCount')).toEqual(['gt', 'gte', 'lt', 'lte', 'eq', 'neq']);
    expect(comparatorsFor('inventoryAggregate')).toHaveLength(8);
    expect(comparatorsFor('metadata')).toHaveLength(8);
  });

  it('withSubject: the state defaults to equipped unless already a state; neq is kept, any other comparator → eq', () => {
    const base = chip({ comparator: 'gte', operand: { kind: 'string', text: 'worn' } });
    expect(withSubject(base, 'inventory')).toMatchObject({ comparator: 'eq', operand: { kind: 'string', text: 'equipped' } });
    expect(withSubject({ ...base, comparator: 'neq', operand: { kind: 'string', text: 'carried' } }, 'inventory')).toMatchObject({
      comparator: 'neq',
      operand: { kind: 'string', text: 'carried' },
    });
  });

  it('withSubject: a count turns containment into ≥ and a non-number operand into 1', () => {
    const base = chip({ comparator: 'contains', operand: { kind: 'string', text: 'x' } });
    expect(withSubject(base, 'inventoryCount')).toMatchObject({ comparator: 'gte', operand: { kind: 'number', text: '1' } });
    expect(withSubject(chip({ comparator: 'lt', operand: { kind: 'number', text: '4' } }), 'inventoryCount')).toMatchObject({
      comparator: 'lt',
      operand: { kind: 'number', text: '4' },
    });
    expect(withSubject(base, 'inventoryAggregate')).toMatchObject({ subject: 'inventoryAggregate', comparator: 'contains' });
  });

  it('an inventory state chip: the four-state select, = and ≠ only, the object box and its title', async () => {
    const fixture = await render(
      gatedDraft([chip({ subject: 'inventory', key: 'Excalibur', operand: { kind: 'string', text: 'equipped' } })]),
    );
    const states = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
    expect([...states.options].map((o) => o.value)).toEqual(['equipped', 'carried', 'reachable', 'absent']);
    expect([...el(fixture).querySelectorAll<HTMLOptionElement>('select[aria-label="Comparator"] option')].map((o) => o.value)).toEqual(['eq', 'neq']);
    const key = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Inventory object reference"]')!;
    expect(key.getAttribute('placeholder')).toBe('Excalibur');
    expect(key.getAttribute('title')).toBe(
      'An object by id, title or name, written literally — a gate has no parameters to fill a placeholder. With no character or chat, the test never matches.',
    );
    // No literal-type picker, no ordering note.
    expect(el(fixture).querySelector('select[aria-label="Literal type"]')).toBeNull();
    expect(text(fixture)).not.toContain('ⓘ');
  });

  it('shows a — option while the state text is not a state, and emits the chosen state', async () => {
    const fixture = await render(
      gatedDraft([chip({ subject: 'inventory', key: 'x', operand: { kind: 'string', text: 'worn' } })]),
    );
    const states = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Inventory state"]')!;
    expect(states.options[0].value).toBe('');
    expect(states.options[0].textContent?.trim()).toBe('—');
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));
    states.value = 'carried';
    states.dispatchEvent(new Event('change'));
    expect(next?.gateConditions[0].operand).toEqual({ kind: 'string', text: 'carried' });
  });

  it('an aggregate chip: the key box with Insert key…, offering every comparator and no ordering note', async () => {
    const fixture = await render(gatedDraft([aggChip()]));
    expect(el(fixture).querySelector('qt-aggregate-key-field')).not.toBeNull();
    expect(el(fixture).querySelector('input[aria-label="Metadata key"]')).toBeNull();
    expect(el(fixture).querySelectorAll('select[aria-label="Comparator"] option')).toHaveLength(8);
    const insert = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Insert an aggregate key"]')!;
    expect(insert.options[0].textContent?.trim()).toBe('Insert key…');
  });

  it('turns a subject change into a legal chip', async () => {
    const fixture = await render(gatedDraft([chip({ comparator: 'gte', operand: { kind: 'number', text: '3' } })]));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));
    const subject = el(fixture).querySelector<HTMLSelectElement>('select[aria-label="Gate subject"]')!;
    subject.value = 'inventory';
    subject.dispatchEvent(new Event('change'));
    expect(next?.gateConditions[0]).toMatchObject({
      subject: 'inventory',
      comparator: 'eq',
      operand: { kind: 'string', text: 'equipped' },
    });
  });
});
