import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { newDraft, validateDraft, type DraftEffect, type ToolDraft } from '../../pascal/tool-draft';
import { SideEffectsSection, whenChoiceOf } from './side-effects-section';

/**
 * SideEffectsSection — asserted against v4
 * `components/custom-tools/SideEffectsSection.tsx`. The load-bearing claims:
 * 'verbatim' is NEVER an option in the When select (it is a read-only badge,
 * so a condition richer than the form can draw cannot be silently flattened by
 * touching the control), the value-kind switch reseeds the value the way v4
 * does, the Add button stops at the cap, and every issue `validateDraft`
 * anchors to an effect row renders beside THAT row.
 */

function effect(over: Partial<DraftEffect> = {}): DraftEffect {
  return {
    id: 'e1',
    when: { kind: 'always' },
    target: 'state.tally',
    valueKind: 'literal-number',
    value: '1',
    ...over,
  };
}

function draftWith(effects: DraftEffect[]): ToolDraft {
  const draft = newDraft();
  draft.name = 'unlock';
  draft.description = 'Pick the lock.';
  draft.outcomes[0] = { ...draft.outcomes[0], message: 'Open.' };
  return { ...draft, effects };
}

async function render(draft: ToolDraft): Promise<ComponentFixture<SideEffectsSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [SideEffectsSection] });
  const fixture = TestBed.createComponent(SideEffectsSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function html(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).innerHTML;
}

describe('whenChoiceOf (v4 SideEffectsSection:34-38)', () => {
  it('flattens the three draft shapes onto the select values, verbatim apart', () => {
    expect(whenChoiceOf(effect({ when: { kind: 'always' } }))).toBe('always');
    expect(whenChoiceOf(effect({ when: { kind: 'outcome-state', state: 'failure' } }))).toBe(
      'failure',
    );
    expect(whenChoiceOf(effect({ when: { kind: 'verbatim', when: { gt: 0.5 } } }))).toBe('verbatim');
  });
});

describe('SideEffectsSection (v4 SideEffectsSection.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('says what an effect is for when none are declared', async () => {
    const fixture = await render(draftWith([]));
    expect(text(fixture)).toContain('None declared.');
    expect(text(fixture)).toContain('Add effect (0/16)');
  });

  it('renders a verbatim condition as a read-only badge, never as a select option', async () => {
    const fixture = await render(
      draftWith([effect({ when: { kind: 'verbatim', when: { gt: 0.5, outcome: { neq: 'failure' } } } })]),
    );
    // The badge shows the condition's own JSON, so nothing is lost to a form
    // that cannot draw it.
    expect(text(fixture)).toContain('{"gt":0.5,"outcome":{"neq":"failure"}}');
    // And the control itself is absent: there is no way to overwrite the
    // condition by nudging a select.
    expect(html(fixture)).not.toContain('aria-label="When this effect applies"');
  });

  it('offers Always and the four outcome states for a form-authored condition', async () => {
    const fixture = await render(draftWith([effect()]));
    const select = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="When this effect applies"]',
    );
    expect(select).not.toBeNull();
    expect(Array.from(select!.options).map((o) => o.value)).toEqual([
      'always',
      'success',
      'partial',
      'failure',
      'info',
    ]);
  });

  it('reseeds the value when the kind changes — true for a boolean, blank for a number', async () => {
    const fixture = await render(draftWith([effect({ valueKind: 'expression', value: '{{value}}' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    const kind = (fixture.nativeElement as HTMLElement).querySelector<HTMLSelectElement>(
      'select[aria-label="Value kind"]',
    )!;
    kind.value = 'literal-boolean';
    kind.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ valueKind: 'literal-boolean', value: 'true' });

    kind.value = 'literal-number';
    kind.dispatchEvent(new Event('change'));
    expect(emitted.at(-1)!.effects[0]).toMatchObject({ valueKind: 'literal-number', value: '' });
  });

  it('keeps the expression text when the kind stays an expression', async () => {
    const fixture = await render(draftWith([effect({ valueKind: 'expression', value: '{{value}} + 1' })]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.updateEffect('e1', { target: 'state.other' });
    expect(emitted.at(-1)!.effects[0].value).toBe('{{value}} + 1');
  });

  it('offers the two prefix shortcuts only while the target is blank', async () => {
    const blank = await render(draftWith([effect({ target: '' })]));
    expect(text(blank)).toContain('state.');
    expect(text(blank)).toContain('metadata.');

    const filled = await render(draftWith([effect({ target: 'state.tally' })]));
    const buttons = Array.from(
      (filled.nativeElement as HTMLElement).querySelectorAll('button'),
    ).map((b) => b.textContent?.trim());
    expect(buttons).not.toContain('metadata.');
  });

  it('adds and removes rows, and stops adding at the cap', async () => {
    const fixture = await render(draftWith([]));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.addEffect();
    expect(emitted.at(-1)!.effects).toHaveLength(1);
    // A fresh row is an expression with nothing in it — v4's defaults.
    expect(emitted.at(-1)!.effects[0]).toMatchObject({
      when: { kind: 'always' },
      target: '',
      valueKind: 'expression',
      value: '',
    });

    const full = await render(draftWith(Array.from({ length: 16 }, (_, i) => effect({ id: `e${i}` }))));
    const add = Array.from(
      (full.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button'),
    ).find((b) => b.textContent?.includes('Add effect'))!;
    expect(add.disabled).toBe(true);
    expect(add.getAttribute('title')).toBe('At most 16 effects');

    const removing = await render(draftWith([effect({ id: 'e1' }), effect({ id: 'e2' })]));
    const seen: ToolDraft[] = [];
    removing.componentInstance.draftChange.subscribe((d) => seen.push(d));
    removing.componentInstance.removeEffect('e1');
    expect(seen.at(-1)!.effects.map((e) => e.id)).toEqual(['e2']);
  });

  it('renders the blocking errors validateDraft anchors to the row', async () => {
    const fixture = await render(
      draftWith([effect({ target: 'nowhere.x', valueKind: 'expression', value: 'broken pick' })]),
    );
    expect(text(fixture)).toContain('target must start with "state." or "metadata."');
    expect(text(fixture)).toContain('the expression does not parse');
  });

  it('renders a warning beside the row rather than as an error', async () => {
    // `{{dice}}` outside the dice form is advisory — the run still deals.
    const draft = draftWith([effect({ valueKind: 'expression', value: "{{dice}} + ''" })]);
    const fixture = await render(draft);
    expect(text(fixture)).toContain('⚠');
    expect(text(fixture)).toContain('{{dice}} is an empty string outside the dice form');
  });
});
