import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  draftFromDefinition,
  newDraft,
  validateDraft,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { BuilderForm, coerceIdentifier } from './builder-form';
import { OutcomesSection } from './outcomes-section';

/**
 * BuilderForm + OutcomesSection — asserted against v4's exact behaviors: the
 * live identifier coercion, the title→slug link and how it breaks, min/max
 * HIDING (not disabling) on non-numeric parameters, the loud
 * delete-a-referenced-parameter confirm, the range readout sentence, the pinned
 * catch-all tail, and the duplicate-slot block.
 */

function draftWith(over: Partial<ToolDraft> = {}): ToolDraft {
  return { ...newDraft(), ...over };
}

async function renderForm(draft: ToolDraft): Promise<ComponentFixture<BuilderForm>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [BuilderForm] });
  const fixture = TestBed.createComponent(BuilderForm);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.detectChanges();
  return fixture;
}

async function renderOutcomes(draft: ToolDraft): Promise<ComponentFixture<OutcomesSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutcomesSection] });
  const fixture = TestBed.createComponent(OutcomesSection);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('issues', validateDraft(draft));
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

/**
 * The Parameters `<section>` only.
 *
 * Found by its heading rather than by index: the sections now run identity,
 * availability gate, parameters, roll, consult, and an index would silently
 * follow the wrong card the next time one is inserted (which is exactly what
 * the gate's arrival did to `sections[1]`).
 */
function parametersSection(fixture: ComponentFixture<unknown>): string {
  const sections = [...(fixture.nativeElement as HTMLElement).querySelectorAll('section')];
  const parameters = sections.find((s) => s.querySelector('h2')?.textContent === 'Parameters');
  return parameters?.textContent ?? '';
}

describe('coerceIdentifier (v4 BuilderForm:49-55)', () => {
  it('lowercases, strips illegal characters, and trims a leading non-letter', () => {
    expect(coerceIdentifier('Force The Lock!')).toBe('forcethelock');
    expect(coerceIdentifier('3d_roll')).toBe('d_roll');
    expect(coerceIdentifier('Já-vu')).toBe('j-vu');
  });

  it('caps at 64 characters', () => {
    expect(coerceIdentifier('a'.repeat(80))).toHaveLength(64);
  });
});

describe('BuilderForm (v4 BuilderForm.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('slugs the name from the title while the name is still empty', async () => {
    const fixture = await renderForm(draftWith({ name: '', title: '' }));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.onTitleChange('Force the Lock');
    expect(next?.name).toBe('force_the_lock');
    expect(next?.title).toBe('Force the Lock');
  });

  it('stops tracking the title once the name is typed by hand', async () => {
    const fixture = await renderForm(draftWith({ name: '', title: '' }));
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.onNameChange('bespoke');
    fixture.componentInstance.onTitleChange('Force the Lock');

    expect(emitted[0].name).toBe('bespoke');
    // The title change must NOT have re-slugged the name. (The emitted draft is
    // a full spread, so `name` is always present — "unchanged" is the claim.)
    expect(emitted[1].name).toBe('');
    expect(emitted[1].title).toBe('Force the Lock');
  });

  it('does not track the title when a draft arrives with a name already set', async () => {
    const fixture = await renderForm(draftWith({ name: 'existing', title: 'Existing' }));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.onTitleChange('Something Else');
    expect(next?.name).toBe('existing');
    expect(next?.title).toBe('Something Else');
  });

  it('coerces a typed name to the identifier grammar live', async () => {
    const fixture = await renderForm(draftWith({ name: '' }));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.onNameChange('Force The Lock!!');
    expect(next?.name).toBe('forcethelock');
  });

  it('HIDES min/max on non-numeric parameters rather than disabling them', async () => {
    // Scoped to the Parameters <section>: the range-form roll block carries its
    // OWN Min/Max labels, so an unscoped text probe can never see this.
    const numeric = await renderForm(
      draftWith({
        parameters: [
          { id: 'p1', name: 'bonus', type: 'number', defaultValue: '0', description: '', min: '', max: '' },
        ],
      }),
    );
    expect(parametersSection(numeric)).toContain('Min');
    expect(parametersSection(numeric)).toContain('Max');

    const stringy = await renderForm(
      draftWith({
        parameters: [
          {
            id: 'p1',
            name: 'material',
            type: 'string',
            defaultValue: 'brass',
            description: '',
            min: '',
            max: '',
          },
        ],
      }),
    );
    // A bound merely greyed out still reads as a bound in force — so v4 removes
    // the controls entirely.
    expect(parametersSection(stringy)).not.toContain('Min');
    expect(parametersSection(stringy)).not.toContain('Max');
  });

  it('re-seats the default when a parameter type changes to boolean', async () => {
    const param = {
      id: 'p1',
      name: 'bonus',
      type: 'number' as const,
      defaultValue: '3',
      description: '',
      min: '2',
      max: '9',
    };
    const fixture = await renderForm(draftWith({ parameters: [param] }));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.onParamTypeChange(param, 'boolean');
    expect(next?.parameters[0].defaultValue).toBe(false);
    // Bounds are cleared with the type, not left dangling.
    expect(next?.parameters[0].min).toBe('');
    expect(next?.parameters[0].max).toBe('');
  });

  it('renames a parameter everywhere on BLUR, not on every keystroke', async () => {
    const draft = draftWith({
      parameters: [
        { id: 'p1', name: 'bonus', type: 'number', defaultValue: '0', description: '', min: '', max: '' },
      ],
      rollRange: { ...newDraft().rollRange, offset: { kind: 'param', name: 'bonus' } },
    });
    const fixture = await renderForm(draft);
    const emitted: ToolDraft[] = [];
    fixture.componentInstance.draftChange.subscribe((d) => emitted.push(d));

    fixture.componentInstance.onParamNameChange(draft.parameters[0], 'boon');
    // The keystroke renamed only the parameter itself — references untouched.
    expect(emitted[0].parameters[0].name).toBe('boon');
    expect(emitted[0].rollRange.offset).toEqual({ kind: 'param', name: 'bonus' });

    fixture.componentInstance.commitParamRename();
    // The blur rewrote the reference atomically.
    expect(emitted[1].rollRange.offset).toEqual({ kind: 'param', name: 'boon' });
  });

  it('deleting a REFERENCED parameter confirms loudly and lists the sites', async () => {
    const draft = draftWith({
      parameters: [
        { id: 'p1', name: 'bonus', type: 'number', defaultValue: '0', description: '', min: '', max: '' },
      ],
      rollRange: { ...newDraft().rollRange, offset: { kind: 'param', name: 'bonus' } },
    });
    const fixture = await renderForm(draft);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.deleteParam(draft.parameters[0]);
    expect(confirm).toHaveBeenCalledOnce();
    expect(confirm.mock.calls[0][0]).toContain('roll offset');
    // v4's promise: deletion does NOT rewrite references.
    expect(confirm.mock.calls[0][0]).toContain('will NOT rewrite those references');
    expect(next).toBeUndefined();

    confirm.mockReturnValue(true);
    fixture.componentInstance.deleteParam(draft.parameters[0]);
    expect(next?.parameters).toEqual([]);
    confirm.mockRestore();
  });

  it('deletes an UNREFERENCED parameter without a confirm', async () => {
    const draft = draftWith({
      parameters: [
        { id: 'p1', name: 'lonely', type: 'number', defaultValue: '0', description: '', min: '', max: '' },
      ],
    });
    const fixture = await renderForm(draft);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.deleteParam(draft.parameters[0]);
    expect(confirm).not.toHaveBeenCalled();
    expect(next?.parameters).toEqual([]);
    confirm.mockRestore();
  });

  it('reads out a literal range with its resulting bounds', async () => {
    const fixture = await renderForm(
      draftWith({
        rollRange: {
          min: { kind: 'literal', text: '1' },
          max: { kind: 'literal', text: '21' },
          multiplier: { kind: 'literal', text: '2' },
          offset: { kind: 'literal', text: '5' },
          round: true,
        },
      }),
    );
    expect(fixture.componentInstance.rangeReadout()).toBe(
      'Draws uniformly in [1, 21), then ×2, then +5, then rounds. Values land in [7, 47).',
    );
  });

  it('omits the resulting bounds when any field is a $param (unknowable)', async () => {
    const fixture = await renderForm(
      draftWith({
        parameters: [
          { id: 'p1', name: 'top', type: 'number', defaultValue: '9', description: '', min: '', max: '' },
        ],
        rollRange: { ...newDraft().rollRange, max: { kind: 'param', name: 'top' } },
      }),
    );
    const readout = fixture.componentInstance.rangeReadout();
    expect(readout).toContain('“top”');
    expect(readout).not.toContain('Values land in');
  });

  it('uses the documented defaults in the readout when fields are blank', async () => {
    const fixture = await renderForm(draftWith());
    expect(fixture.componentInstance.rangeReadout()).toBe(
      'Draws uniformly in [0, 1). Values land in [0, 1).',
    );
  });

  it('offers only numeric parameters to the roll $param pickers', async () => {
    const fixture = await renderForm(
      draftWith({
        parameters: [
          { id: 'p1', name: 'bonus', type: 'number', defaultValue: '0', description: '', min: '', max: '' },
          { id: 'p2', name: 'tally', type: 'integer', defaultValue: '0', description: '', min: '', max: '' },
          { id: 'p3', name: 'stuff', type: 'string', defaultValue: 'x', description: '', min: '', max: '' },
        ],
      }),
    );
    expect(fixture.componentInstance.numericNames()).toEqual(['bonus', 'tally']);
  });

  it('describes a parsed dice notation, and rejects an unparseable one', async () => {
    const good = await renderForm(draftWith({ rollForm: 'dice', rollDice: '3d6+2' }));
    expect(text(good)).toContain('3 dice, 6 sides, +2 — totals 5–20');

    const one = await renderForm(draftWith({ rollForm: 'dice', rollDice: '1d20' }));
    expect(text(one)).toContain('1 die, 20 sides');

    const bad = await renderForm(draftWith({ rollForm: 'dice', rollDice: '1d1' }));
    expect(text(bad)).toContain('Dice notation like');
  });

  it('caps the Add parameter button at MAX_PARAMETERS', async () => {
    const eight = Array.from({ length: 8 }, (_, i) => ({
      id: `p${i}`,
      name: `p${i}`,
      type: 'number' as const,
      defaultValue: '0',
      description: '',
      min: '',
      max: '',
    }));
    const fixture = await renderForm(draftWith({ parameters: eight }));
    const button = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find((b) =>
      b.textContent?.includes('Add parameter'),
    );
    expect(button?.disabled).toBe(true);
    expect(button?.textContent).toContain('(8/8)');
  });

  it('surfaces the identity errors validateDraft reports', async () => {
    const fixture = await renderForm(draftWith({ name: '', description: '' }));
    expect(fixture.componentInstance.nameError()).toBe(
      'name must be lowercase, start with a letter, and be 1–64 characters',
    );
    expect(fixture.componentInstance.descriptionError()).toBe('description is required');
  });

  it('reads out a $state roll field as `state:path` (v4 BuilderForm rangeReadout)', async () => {
    const fixture = await renderForm(
      draftWith({
        rollRange: {
          ...newDraft().rollRange,
          min: { kind: 'state', path: 'game.low', fallback: '0' },
        },
      }),
    );
    const readout = fixture.componentInstance.rangeReadout();
    expect(readout).toContain('state:game.low');
    // A $state field is not literal, so the resulting-bounds tail is omitted.
    expect(readout).not.toContain('Values land in');
  });

  it('renders a $state parameter default as a READ-ONLY pill (never authored)', async () => {
    const draft = draftFromDefinition({
      name: 'draw',
      description: 'x',
      parameters: { bonus: { type: 'number', default: { $state: 'player.bonus', fallback: 1 } } },
      outcomes: [{ when: true, message: 'f', state: 'info' }],
    })!;
    const fixture = await renderForm(draft);
    expect(parametersSection(fixture)).toContain('$state: player.bonus → 1');
    // No number/text input is offered for a $state default — it is a pill.
    const defaultInputs = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('input'),
    ].filter((i) => i.type === 'number' || i.type === 'text');
    // (Only the min/max numeric inputs remain; the default is a pill, not an input.)
    expect(defaultInputs.every((i) => i.getAttribute('value') !== '[object Object]')).toBe(true);
  });
});

describe('OutcomesSection (v4 OutcomesSection.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the pinned tail as "otherwise" with no move or delete controls', async () => {
    const fixture = await renderOutcomes(draftWith());
    const body = text(fixture);
    expect(body).toContain('otherwise');
    expect(body).toContain('when…');

    // Two rows, but only the non-tail row carries a delete control.
    const deletes = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].filter(
      (b) => b.getAttribute('aria-label')?.startsWith('Delete outcome'),
    );
    expect(deletes).toHaveLength(1);
  });

  it('renders a $state operand as a READ-ONLY pill with no type picker', async () => {
    const draft = draftFromDefinition({
      name: 'gate',
      description: 'x',
      outcomes: [
        { when: { gte: { $state: 'game.diff', fallback: 3 } }, message: 'a', state: 'success' },
        { when: true, message: 'f', state: 'info' },
      ],
    })!;
    const fixture = await renderOutcomes(draft);
    expect(text(fixture)).toContain('$state: game.diff → 3');
    // No literal-type picker is offered for a $state operand.
    const typePickers = [
      ...(fixture.nativeElement as HTMLElement).querySelectorAll('select'),
    ].filter((s) => s.getAttribute('aria-label') === 'Literal type');
    expect(typePickers).toHaveLength(0);
  });

  it('inserts a new outcome ABOVE the catch-all', async () => {
    const fixture = await renderOutcomes(draftWith());
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    fixture.componentInstance.addOutcome();
    expect(next?.outcomes).toHaveLength(3);
    expect(next?.outcomes[2].catchAll).toBe(true);
    expect(next?.outcomes[1].catchAll).toBe(false);
  });

  it('never moves a row into or past the pinned tail', async () => {
    const draft = draftWith();
    const fixture = await renderOutcomes(draft);
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d) => (next = d));

    // Index 0 is the only non-tail row; moving it down would swap the tail.
    fixture.componentInstance.move(0, 1);
    expect(next).toBeUndefined();
  });

  it('flashes the outcome a bench roll landed on', async () => {
    const draft = draftWith();
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [OutcomesSection] });
    const fixture = TestBed.createComponent(OutcomesSection);
    fixture.componentRef.setInput('draft', draft);
    fixture.componentRef.setInput('issues', validateDraft(draft));
    fixture.componentRef.setInput('flashOutcomeId', draft.outcomes[1].id);
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).querySelectorAll('.qt-highlight')).toHaveLength(1);
  });

  it('routes an outcome error to its own row', async () => {
    const draft = draftWith();
    const fixture = await renderOutcomes(draft);
    // The fresh non-catch-all row has no conditions, so validateDraft objects.
    expect(fixture.componentInstance.outcomeErrors(draft.outcomes[0].id)).toContain(
      'must test something — add a condition, or delete the row',
    );
    expect(fixture.componentInstance.outcomeErrors(draft.outcomes[1].id)).toEqual([]);
  });

  it('separates message warnings from blocking errors', async () => {
    const draft = draftWith();
    draft.outcomes[1].message = 'Rolled {{dice}}.';
    const fixture = await renderOutcomes(draft);
    expect(fixture.componentInstance.messageWarnings(draft.outcomes[1].id)).toEqual([
      '{{dice}} renders as an empty string outside the dice form',
    ]);
    expect(fixture.componentInstance.outcomeErrors(draft.outcomes[1].id)).toEqual([]);
  });

  it('renders the four state options as a radiogroup', async () => {
    const fixture = await renderOutcomes(draftWith());
    const radios = [...(fixture.nativeElement as HTMLElement).querySelectorAll('[role="radio"]')];
    // Four per row, two rows.
    expect(radios).toHaveLength(8);
    expect(radios.map((r) => r.textContent?.trim()).slice(0, 4)).toEqual([
      'success',
      'partial',
      'failure',
      'info',
    ]);
  });
});
