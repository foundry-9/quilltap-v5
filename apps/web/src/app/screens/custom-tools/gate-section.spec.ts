import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import {
  newDraft,
  validateDraft,
  type DraftGateCondition,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { GateSection } from './gate-section';

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
      'Offered only to a character whose sheet passes every test:',
    );
    const withheld = gatedDraft([chip()]);
    withheld.gateMode = 'withheld';
    expect(text(await render(withheld))).toContain(
      'Withheld from a character whose sheet passes every test:',
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
      'The gate can test “rank” with ≥ only once.',
    );
    expect(text(fixture)).toContain('The gate can test “rank” with ≥ only once.');
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
