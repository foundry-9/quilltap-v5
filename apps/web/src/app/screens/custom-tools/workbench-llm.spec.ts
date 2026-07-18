import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { draftFromDefinition, validateDraft, type ToolDraft } from '../../pascal/tool-draft';
import { BuilderForm } from './builder-form';
import { OutcomesSection } from './outcomes-section';
import { ProvingBench } from './proving-bench';

/**
 * Pascal's Workbench — the LLM-consult surfaces.
 *
 * A CASE-FOR-CASE port of v4's own
 * `__tests__/unit/components/custom-tools/workbench-llm.test.tsx` (148 lines,
 * nine cases): render-level coverage for the oracle's three homes — the
 * BuilderForm section (enable toggle, prompt, error line, inline issues), the
 * OutcomesSection condition chips (consult answer / consult succeeded appear
 * only while the consult is on), and the ProvingBench oracle card (scripted /
 * silence / live, with the scripted answer riding the preview body).
 *
 * The one deliberate divergence is the transport: v4 spies on `fetch` and reads
 * the request body; v5 stubs `CoreClient.dispatchData` and reads the dispatch
 * request, which is the same assertion one layer down (the §B contract).
 */

const ORACLE_TOOL = {
  name: 'augury',
  description: 'Consult the oracle.',
  llm: { prompt: 'YES or NO about {{value}}?', errorMessage: 'The wire went dead.' },
  outcomes: [
    { when: { llm: { ok: false } }, message: 'Silence: {{llm}}', state: 'failure' },
    { when: { llm: { eq: 'YES' } }, message: 'Assent.', state: 'success' },
    { when: true, message: 'Demurral.', state: 'info' },
  ],
};

const PLAIN_TOOL = {
  name: 'plain',
  description: 'No oracle here.',
  outcomes: [{ when: true, message: 'Done.', state: 'info' }],
};

function oracleDraft(): ToolDraft {
  const draft = draftFromDefinition(ORACLE_TOOL);
  if (!draft) throw new Error('fixture failed to load');
  return draft;
}

function plainDraft(): ToolDraft {
  const draft = draftFromDefinition(PLAIN_TOOL);
  if (!draft) throw new Error('fixture failed to load');
  return draft;
}

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient(hooks: { onDispatch?: (req: Req) => void; preview?: unknown } = {}) {
  return {
    dispatchData: (async (req: Req) => {
      hooks.onDispatch?.(req);
      if (req.type === 'customToolPreview') return hooks.preview ?? {};
      if (req.type === 'customToolsDestinations') return { characters: [] };
      return {};
    }) as CoreClient['dispatchData'],
  };
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

async function renderBench(
  draft: ToolDraft,
  client: Partial<CoreClient>,
): Promise<ComponentFixture<ProvingBench>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProvingBench],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ProvingBench);
  fixture.componentRef.setInput('draft', draft);
  fixture.componentRef.setInput('valid', true);
  fixture.detectChanges();
  return fixture;
}

function host(fixture: ComponentFixture<unknown>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

/**
 * Testing-Library's `getByLabelText`, in the two forms v4's markup uses: an
 * `aria-label`, a `<label for=…>` pointing at an id, and a `<label>` that
 * WRAPS its control (which is how the enable toggle is written).
 */
function byLabel<T extends HTMLElement>(fixture: ComponentFixture<unknown>, label: string): T {
  const root = host(fixture);
  const aria = root.querySelector<T>(`[aria-label="${label}"]`);
  if (aria) return aria;

  const labels = Array.from(root.querySelectorAll('label')).filter(
    (l) => l.textContent?.trim() === label,
  );
  for (const el of labels) {
    const id = el.getAttribute('for');
    const target = id ? root.querySelector<T>(`#${id}`) : el.querySelector<T>('input, textarea');
    if (target) return target;
  }
  throw new Error(`no control labelled "${label}"`);
}

function radioByName(fixture: ComponentFixture<unknown>, name: string): HTMLElement | null {
  return (
    Array.from(host(fixture).querySelectorAll<HTMLElement>('[role="radio"]')).find(
      (b) => b.textContent?.trim() === name,
    ) ?? null
  );
}

afterEach(() => {
  TestBed.resetTestingModule();
});

describe('BuilderForm — the consulted oracle section', () => {
  it('shows the prompt and error fields while the consult is enabled', async () => {
    const fixture = await renderForm(oracleDraft());
    expect(byLabel<HTMLInputElement>(fixture, 'Consult an LLM').checked).toBe(true);
    expect(byLabel<HTMLTextAreaElement>(fixture, 'The question').value).toBe(
      'YES or NO about {{value}}?',
    );
    expect(byLabel<HTMLInputElement>(fixture, 'When the oracle is silent').value).toBe(
      'The wire went dead.',
    );
  });

  it('hides the fields and explains itself while disabled', async () => {
    const fixture = await renderForm(plainDraft());
    expect(byLabel<HTMLInputElement>(fixture, 'Consult an LLM').checked).toBe(false);
    expect(host(fixture).querySelector('#wb-llm-prompt')).toBeNull();
    expect(host(fixture).textContent).toContain('Off. Enable it and every run asks a model');
  });

  it('toggling the switch flips llmEnabled', async () => {
    const draft = plainDraft();
    const fixture = await renderForm(draft);
    const spy = vi.fn();
    fixture.componentInstance.draftChange.subscribe(spy);

    const toggle = byLabel<HTMLInputElement>(fixture, 'Consult an LLM');
    toggle.checked = true;
    toggle.dispatchEvent(new Event('change'));

    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ llmEnabled: true }));
  });

  it('surfaces validateDraft issues inline', async () => {
    const draft = oracleDraft();
    draft.llmPrompt = '';
    const fixture = await renderForm(draft);
    expect(host(fixture).textContent).toContain('the consult needs a prompt');
  });
});

describe('OutcomesSection — consult condition chips', () => {
  it('renders the llm chips of a loaded definition', async () => {
    const fixture = await renderOutcomes(oracleDraft());
    // Row 1 tests ok; row 2 tests the answer.
    const subjects = Array.from(
      host(fixture).querySelectorAll<HTMLSelectElement>('[aria-label="Condition subject"]'),
    );
    expect(subjects.map((s) => s.value)).toEqual(expect.arrayContaining(['llm-ok', 'llm']));
  });

  it('offers the consult subjects only while the consult is enabled', async () => {
    const fixture = await renderOutcomes(oracleDraft());
    const subject = host(fixture).querySelector<HTMLSelectElement>(
      '[aria-label="Condition subject"]',
    )!;
    expect(Array.from(subject.options).map((o) => o.value)).toEqual(
      expect.arrayContaining(['llm', 'llm-ok']),
    );

    const withoutOracle = oracleDraft();
    withoutOracle.llmEnabled = false;
    // Keep only the catch-all so no llm chips linger in the render.
    withoutOracle.outcomes = [withoutOracle.outcomes[withoutOracle.outcomes.length - 1]];
    const second = await renderOutcomes(withoutOracle);
    expect(second.nativeElement.querySelector('[aria-label="Condition subject"]')).toBeNull();
  });
});

describe('ProvingBench — the oracle card', () => {
  it('offers scripted / silence / live modes when the tool consults', async () => {
    const fixture = await renderBench(oracleDraft(), stubClient());
    expect(radioByName(fixture, 'Scripted answer')).not.toBeNull();
    expect(radioByName(fixture, 'Silence')).not.toBeNull();
    expect(radioByName(fixture, 'Ask it live')).not.toBeNull();
  });

  it('shows no oracle card for a tool without a consult', async () => {
    const fixture = await renderBench(plainDraft(), stubClient());
    expect(radioByName(fixture, 'Scripted answer')).toBeNull();
  });

  it('sends the scripted answer with a preview roll', async () => {
    const seen: Req[] = [];
    const fixture = await renderBench(
      oracleDraft(),
      stubClient({
        onDispatch: (req) => seen.push(req),
        preview: {
          tool: 'augury',
          params: {},
          rollForm: 'range',
          raw: 0.5,
          value: 0.5,
          state: 'success',
          outcomeIndex: 1,
          message: 'Assent.',
          diceBreakdown: '',
          visibility: 'public',
          llm: { ok: true, output: 'YES', prompt: 'YES or NO about 0.5?' },
        },
      }),
    );

    const answer = byLabel<HTMLTextAreaElement>(fixture, 'Scripted oracle answer');
    answer.value = 'YES';
    answer.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    await fixture.componentInstance.roll();
    fixture.detectChanges();

    expect(host(fixture).textContent).toContain('consult answered');
    const preview = seen.find((r) => r.type === 'customToolPreview');
    expect(preview).toMatchObject({ llm: { output: 'YES' } });
  });

  /**
   * Not one of v4's nine, but the §B invariant its shape encodes: the audit
   * body can never carry `live`, whatever the card is set to.
   */
  it('never sends a live oracle on an audit', async () => {
    const seen: Req[] = [];
    const fixture = await renderBench(oracleDraft(), stubClient({ onDispatch: (r) => seen.push(r) }));
    radioByName(fixture, 'Ask it live')!.click();
    fixture.detectChanges();

    await fixture.componentInstance.runAudit();

    const audit = seen.find((r) => r.type === 'customToolAudit');
    expect(audit?.['llm']).toEqual({ fail: true });
  });
});
