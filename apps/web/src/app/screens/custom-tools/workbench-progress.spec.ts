import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { draftFromDefinition, validateDraft, type ToolDraft } from '../../pascal/tool-draft';
import { OutcomesSection } from './outcomes-section';
import { ProvingBench } from './proving-bench';
import { SideEffectsSection } from './side-effects-section';

/**
 * Pascal's Workbench — the `progress` affordances (v4 `0587d1e96`'s
 * `components/custom-tools/{OutcomesSection, ProvingBench,
 * SideEffectsSection}.tsx` hunks).
 *
 * The gate chip's own subject select lives in `gate-section.spec.ts` beside the
 * rest of that panel's coverage. What is here is the three surfaces that had no
 * per-panel spec of their own for these hunks: the outcome subject and its key
 * input, the two placeholder menu items (and the `window.prompt` they open),
 * the bench's derived-progressions list, and the side-effects prefix button.
 *
 * ⚠ `window.prompt` returns `null` under jsdom exactly as it does in a headless
 * browser pane, so every case that opens one stubs it — a bare click would
 * assert nothing.
 *
 * @module screens/custom-tools/workbench-progress.spec
 */

const PROGRESS_TOOL = {
  name: 'fire',
  description: 'Fire the gun.',
  outcomes: [
    {
      when: { progress: { 'cannon.complete': { eq: true } } },
      message: 'It fires.',
      state: 'success',
    },
    { when: true, message: 'It clicks.', state: 'failure' },
  ],
};

const PLAIN_TOOL = {
  name: 'plain',
  description: 'No progressions here.',
  outcomes: [{ when: true, message: 'Done.', state: 'info' }],
};

function draft(definition: unknown): ToolDraft {
  const built = draftFromDefinition(definition);
  if (!built) throw new Error('fixture failed to load');
  return built;
}

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient() {
  return {
    dispatchData: (async (req: Req) => {
      if (req.type === 'customToolsDestinations') return { characters: [] };
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function renderOutcomes(d: ToolDraft): Promise<ComponentFixture<OutcomesSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [OutcomesSection] });
  const fixture = TestBed.createComponent(OutcomesSection);
  fixture.componentRef.setInput('draft', d);
  fixture.componentRef.setInput('issues', validateDraft(d));
  fixture.detectChanges();
  return fixture;
}

async function renderBench(d: ToolDraft): Promise<ComponentFixture<ProvingBench>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProvingBench],
    providers: [{ provide: CoreClient, useValue: stubClient() }],
  });
  const fixture = TestBed.createComponent(ProvingBench);
  fixture.componentRef.setInput('draft', d);
  fixture.componentRef.setInput('valid', true);
  fixture.detectChanges();
  return fixture;
}

async function renderEffects(d: ToolDraft): Promise<ComponentFixture<SideEffectsSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [SideEffectsSection] });
  const fixture = TestBed.createComponent(SideEffectsSection);
  fixture.componentRef.setInput('draft', d);
  fixture.componentRef.setInput('issues', validateDraft(d));
  fixture.detectChanges();
  return fixture;
}

const el = (f: ComponentFixture<unknown>) => f.nativeElement as HTMLElement;
const text = (f: ComponentFixture<unknown>) => el(f).textContent ?? '';
const menuItem = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(el(f).querySelectorAll<HTMLButtonElement>('button[role="menuitem"]')).find(
    (b) => b.textContent?.trim() === label,
  );

afterEach(() => {
  vi.restoreAllMocks();
});

describe('OutcomesSection — the progress subject', () => {
  it('offers Progress… beside Metadata…, with v4’s title', async () => {
    // A tool with a chip, so the subject select exists: a catch-all row has none.
    const fixture = await renderOutcomes(draft(PROGRESS_TOOL));
    const select = el(fixture).querySelector<HTMLSelectElement>(
      'select[aria-label="Condition subject"]',
    )!;
    const option = Array.from(select.options).find((o) => o.value === 'progress')!;
    expect(option.textContent?.trim()).toBe('Progress…');
    expect(option.title).toBe("A derived field of one of the character's timed progressions");
    // v4 lists it immediately after Metadata…, before the consult options.
    const values = Array.from(select.options).map((o) => o.value);
    expect(values.indexOf('progress')).toBe(values.indexOf('metadata') + 1);
  });

  it('renders the key input for a progress chip, with v4’s twelve-field title', async () => {
    const fixture = await renderOutcomes(draft(PROGRESS_TOOL));
    const input = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Progress key"]')!;
    expect(input.value).toBe('cannon.complete');
    expect(input.placeholder).toBe('cannon.complete');
    expect(input.title).toBe(
      'A progression of the invoking character, keyed "<id>.<field>" — percent, complete, ' +
        'remainingMs, state, started, elapsedMs, startTime, endTime, name, elapsed, remaining, ' +
        'quantity. A progression they do not carry simply doesn’t match.',
    );
  });

  it('carries an edited progress key into the draft', async () => {
    const fixture = await renderOutcomes(draft(PROGRESS_TOOL));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    const input = el(fixture).querySelector<HTMLInputElement>('input[aria-label="Progress key"]')!;
    input.value = 'cannon.percent';
    input.dispatchEvent(new Event('input'));

    expect(next?.outcomes[0].conditions[0].subject).toEqual({
      kind: 'progress',
      key: 'cannon.percent',
    });
  });

  it('switches a metadata chip onto progress, keeping the comparator', async () => {
    const fixture = await renderOutcomes(
      draft({
        ...PLAIN_TOOL,
        outcomes: [
          { when: { metadata: { rank: { eq: 'captain' } } }, message: 'a', state: 'success' },
          { when: true, message: 'b', state: 'info' },
        ],
      }),
    );
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    const select = el(fixture).querySelector<HTMLSelectElement>(
      'select[aria-label="Condition subject"]',
    )!;
    select.value = 'progress';
    select.dispatchEvent(new Event('change'));

    // v4 starts the progress key EMPTY: a metadata key is not a progress key.
    expect(next?.outcomes[0].conditions[0].subject).toEqual({ kind: 'progress', key: '' });
    expect(next?.outcomes[0].conditions[0].comparator).toBe('eq');
  });

  it('offers every comparator on a progress chip — the stored type is unknowable', async () => {
    const fixture = await renderOutcomes(draft(PROGRESS_TOOL));
    const comparator = el(fixture).querySelector<HTMLSelectElement>(
      'select[aria-label="Comparator"]',
    )!;
    expect(Array.from(comparator.options).map((o) => o.value)).toEqual([
      'gt',
      'gte',
      'lt',
      'lte',
      'eq',
      'neq',
      'contains',
      'ncontains',
    ]);
  });
});

describe('OutcomesSection — the two new placeholder menu items', () => {
  const openMenu = (f: ComponentFixture<unknown>) => {
    el(f).querySelector<HTMLButtonElement>('button[aria-expanded]')!.click();
    f.detectChanges();
  };

  it('offers Progress field… and Now (epoch ms), after Metadata key…', async () => {
    const fixture = await renderOutcomes(draft(PLAIN_TOOL));
    openMenu(fixture);
    const labels = Array.from(
      el(fixture).querySelectorAll<HTMLButtonElement>('button[role="menuitem"]'),
    ).map((b) => b.textContent?.trim());
    expect(labels).toContain('Progress field…');
    expect(labels).toContain('Now (epoch ms)');
    expect(labels.indexOf('Progress field…')).toBe(labels.indexOf('Metadata key…') + 1);
    expect(labels.indexOf('Now (epoch ms)')).toBe(labels.indexOf('Progress field…') + 1);
  });

  it('inserts {{now}} with no prompt at all', async () => {
    const fixture = await renderOutcomes(draft(PLAIN_TOOL));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    openMenu(fixture);
    menuItem(fixture, 'Now (epoch ms)')!.click();

    expect(next?.outcomes[0].message).toContain('{{now}}');
  });

  it('prompts for a progress field and inserts what the author typed', async () => {
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('cannon.complete');
    const fixture = await renderOutcomes(draft(PLAIN_TOOL));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    openMenu(fixture);
    menuItem(fixture, 'Progress field…')!.click();

    expect(prompt).toHaveBeenCalledWith(
      'Progress field to render, as "<progression id>.<field>":',
      'cannon.percent',
    );
    expect(next?.outcomes[0].message).toContain('{{progress.cannon.complete}}');
  });

  it('suggests a key the tool already tests, over the fallback', async () => {
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue(null);
    const fixture = await renderOutcomes(draft(PROGRESS_TOOL));

    openMenu(fixture);
    menuItem(fixture, 'Progress field…')!.click();

    expect(prompt).toHaveBeenCalledWith(expect.any(String), 'cannon.complete');
  });

  it('inserts nothing when the author dismisses the prompt', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue(null);
    const fixture = await renderOutcomes(draft(PLAIN_TOOL));
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    openMenu(fixture);
    menuItem(fixture, 'Progress field…')!.click();

    expect(next).toBeUndefined();
  });
});

describe('ProvingBench — the derived progressions', () => {
  /** A cannon that finished long ago, so the derived state is stable. */
  const SHEET = JSON.stringify({
    progressions: {
      cannon: {
        name: 'Cannon recharge',
        startTime: '2020-01-01T14:00:00Z',
        endTime: '2020-01-01T14:10:00Z',
        timeIncrement: 'minute',
      },
    },
  });

  it('names both subjects in the fact-sheet hint', async () => {
    const fixture = await renderBench(draft(PLAIN_TOOL));
    expect(text(fixture).replace(/\s+/g, ' ')).toContain(
      'Metadata tests read the invoking character’s metadata.json, and progress tests read the ' +
        'progressions key inside it. Lend the bench a sheet, or it rolls as nobody in particular.',
    );
  });

  it('shows nothing while the sheet carries no progressions', async () => {
    const fixture = await renderBench(draft(PLAIN_TOOL));
    expect(text(fixture)).not.toContain('Progressions derived from this sheet');
  });

  it('lists every derived field of a hand-typed sheet', async () => {
    const fixture = await renderBench(draft(PLAIN_TOOL));
    fixture.componentInstance.sheet.set({ mode: 'manual', text: SHEET });
    fixture.detectChanges();

    expect(text(fixture)).toContain('Progressions derived from this sheet');
    const keys = fixture.componentInstance.derivedProgress()!.map((r) => r.key);
    expect(keys).toEqual([
      'cannon.name',
      'cannon.percent',
      'cannon.elapsedMs',
      'cannon.remainingMs',
      'cannon.startTime',
      'cannon.endTime',
      'cannon.started',
      'cannon.complete',
      'cannon.state',
      'cannon.elapsed',
      'cannon.remaining',
    ]);
    expect(text(fixture)).toContain('cannon.complete');
    expect(text(fixture)).toContain('complete');
  });

  it('declines to guess at a character-backed sheet — that one lives on the server', async () => {
    const fixture = await renderBench(draft(PLAIN_TOOL));
    fixture.componentInstance.sheet.set({ mode: 'character', characterId: 'c-1' });
    fixture.detectChanges();
    expect(fixture.componentInstance.derivedProgress()).toBeNull();
  });

  it('answers the gate WITH the derived sheet', async () => {
    const gated = draft({
      ...PLAIN_TOOL,
      availableWhen: { progress: { 'cannon.complete': { eq: true } } },
    });
    const fixture = await renderBench(gated);
    fixture.componentInstance.sheet.set({ mode: 'manual', text: SHEET });
    fixture.detectChanges();
    expect(fixture.componentInstance.gateVerdict()).toEqual({ available: true });

    // The same gate against a sheet carrying no such progression fails CLOSED.
    fixture.componentInstance.sheet.set({ mode: 'manual', text: '{}' });
    fixture.detectChanges();
    expect(fixture.componentInstance.gateVerdict()).toEqual({
      available: false,
      withheldBy: 'availableWhen',
    });
  });
});

describe('SideEffectsSection — the progress. prefix', () => {
  const effectDraft = () => {
    const d = draft(PLAIN_TOOL);
    d.effects = [
      {
        id: 'e1',
        when: { kind: 'always' },
        target: '',
        valueKind: 'literal-number',
        value: '1',
      },
    ];
    return d;
  };

  it('offers all three prefixes, progress. last', async () => {
    const fixture = await renderEffects(effectDraft());
    const labels = Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('button.font-mono'))
      .map((b) => b.textContent?.trim())
      .filter((t) => t?.endsWith('.'));
    expect(labels).toEqual(['state.', 'metadata.', 'progress.']);
  });

  it('carries v4’s three-way title', async () => {
    const fixture = await renderEffects(effectDraft());
    const button = Array.from(
      el(fixture).querySelectorAll<HTMLButtonElement>('button.font-mono'),
    ).find((b) => b.textContent?.trim() === 'progress.')!;
    expect(button.title).toBe(
      'Adjust one of the rolling character’s timed progressions — progress.<id>.<field>. An id ' +
        'nobody authored is created; write true to progress.<id>.remove to delete one.',
    );
  });

  it('seeds the target with the prefix', async () => {
    const fixture = await renderEffects(effectDraft());
    let next: ToolDraft | undefined;
    fixture.componentInstance.draftChange.subscribe((d: ToolDraft) => (next = d));

    Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('button.font-mono'))
      .find((b) => b.textContent?.trim() === 'progress.')!
      .click();

    expect(next?.effects[0].target).toBe('progress.');
  });
});
