import { By } from '@angular/platform-browser';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { ProviderNumberField, ProviderOptionsPanel, toInputString } from './provider-options-panel';
import type { ProviderOptionsSchema } from './provider-options-schema';

/**
 * Parity specs for the schema-driven provider-options renderer. The oracle is
 * v4's CLIENT component, `components/settings/connection-profiles/
 * ProviderOptionsPanel.tsx` at `93ed8abf`; every case cites the lines it
 * mirrors.
 */

interface Written {
  key: string;
  value: unknown;
}

async function render(inputs: {
  schema: ProviderOptionsSchema | null;
  parameters?: Record<string, unknown>;
  fetchedModels?: string[];
  modelName?: string;
}): Promise<{ fixture: ComponentFixture<ProviderOptionsPanel>; writes: Written[] }> {
  // Reset first: several cases render twice in one `it` to compare two bags,
  // and TestBed refuses a second `configureTestingModule` once instantiated.
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ProviderOptionsPanel] });
  const fixture = TestBed.createComponent(ProviderOptionsPanel);
  const writes: Written[] = [];
  fixture.componentRef.setInput('schema', inputs.schema);
  fixture.componentRef.setInput('parameters', inputs.parameters ?? {});
  fixture.componentRef.setInput('fetchedModels', inputs.fetchedModels ?? []);
  fixture.componentRef.setInput('modelName', inputs.modelName ?? '');
  fixture.componentInstance.setParameter.subscribe((w) => writes.push(w));
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, writes };
}

function el<T extends HTMLElement>(fixture: ComponentFixture<unknown>, selector: string): T | null {
  return (fixture.nativeElement as HTMLElement).querySelector<T>(selector);
}

/** The child component drawing one `number` row, for the paths the DOM cannot reach. */
function numberField(fixture: ComponentFixture<unknown>, key: string): ProviderNumberField {
  const found = fixture.debugElement
    .queryAll(By.directive(ProviderNumberField))
    .map((d) => d.componentInstance as ProviderNumberField)
    .find((c) => c.field().key === key);
  if (!found) throw new Error(`no number field for ${key}`);
  return found;
}

interface RoundTrip {
  fixture: ComponentFixture<ProviderOptionsPanel>;
  /** The bag as the host currently holds it. */
  bag: () => Record<string, unknown>;
}

/**
 * The panel wired to its REAL host behaviour: every `setParameter` write lands
 * back in the `parameters` input, `undefined` DELETING the key exactly as
 * `ProfileModal.setParameter` does (v4's `ParameterHost`). Bug 72 only exists
 * in that round trip, so the number cases below need it.
 */
async function renderRoundTrip(inputs: {
  schema: ProviderOptionsSchema;
  parameters?: Record<string, unknown>;
}): Promise<RoundTrip> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ProviderOptionsPanel] });
  const fixture = TestBed.createComponent(ProviderOptionsPanel);
  let bag: Record<string, unknown> = { ...(inputs.parameters ?? {}) };
  fixture.componentRef.setInput('schema', inputs.schema);
  fixture.componentRef.setInput('parameters', bag);
  fixture.componentRef.setInput('fetchedModels', []);
  fixture.componentRef.setInput('modelName', '');
  fixture.componentInstance.setParameter.subscribe(({ key, value }) => {
    const next = { ...bag };
    if (value === undefined) {
      delete next[key];
    } else {
      next[key] = value;
    }
    bag = next;
    fixture.componentRef.setInput('parameters', bag);
  });
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return { fixture, bag: () => bag };
}

/** One keystroke at a time, the way `userEvent.type` drives v4's cases. */
async function typeInto(host: RoundTrip, input: HTMLInputElement, text: string): Promise<void> {
  for (const ch of text) {
    input.value = input.value + ch;
    input.dispatchEvent(new Event('input'));
    host.fixture.detectChanges();
    await host.fixture.whenStable();
    host.fixture.detectChanges();
  }
}

/** `userEvent.clear` — select all, delete, one input event. */
async function clearInput(host: RoundTrip, input: HTMLInputElement): Promise<void> {
  input.value = '';
  input.dispatchEvent(new Event('input'));
  host.fixture.detectChanges();
  await host.fixture.whenStable();
  host.fixture.detectChanges();
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('ProviderOptionsPanel (nothing to draw)', () => {
  it('renders nothing at all for a null schema (v4 `:57`)', async () => {
    const { fixture } = await render({ schema: null });
    expect((fixture.nativeElement as HTMLElement).children.length).toBe(0);
  });

  it('renders nothing for a schema with no groups (v4 `:57`)', async () => {
    const { fixture } = await render({ schema: { groups: [] } });
    expect((fixture.nativeElement as HTMLElement).children.length).toBe(0);
  });

  it('draws a group with a title and helpText around its fields (v4 `:73-93`)', async () => {
    const { fixture } = await render({
      schema: {
        groups: [
          {
            title: 'Sampling',
            helpText: 'Sent only when filled in.',
            fields: [{ key: 'top_k', label: 'Top K', type: 'number' }],
          },
        ],
      },
    });
    expect(el(fixture, 'h4.qt-settings-section-heading')!.textContent!.trim()).toBe('Sampling');
    expect(text(fixture)).toContain('Sent only when filled in.');
    expect(el(fixture, '#pof-top_k')).not.toBeNull();
  });

  it('draws one shell per group, in schema order (v4 `:68-95`)', async () => {
    const { fixture } = await render({
      schema: {
        groups: [
          { title: 'First', fields: [{ key: 'a', label: 'A', type: 'string' }] },
          { title: 'Second', fields: [{ key: 'b', label: 'B', type: 'string' }] },
        ],
      },
    });
    const headings = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('h4')).map(
      (h) => h.textContent!.trim(),
    );
    expect(headings).toEqual(['First', 'Second']);
    expect(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.qt-settings-shell').length,
    ).toBe(2);
  });

  it('renders an unknown field type as nothing, leaving its siblings alone (v4 `:135-136`)', async () => {
    const { fixture } = await render({
      schema: {
        groups: [
          {
            fields: [
              { key: 'weird', label: 'Weird', type: 'colour' as unknown as 'string' },
              { key: 'plain', label: 'Plain', type: 'string' },
            ],
          },
        ],
      },
    });
    expect(text(fixture)).not.toContain('Weird');
    expect(el(fixture, '#pof-plain')).not.toBeNull();
  });
});

describe('ProviderOptionsPanel (boolean fields)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [
      {
        title: 'Ollama Options',
        fields: [
          {
            key: 'enable_thinking',
            label: 'Enable Thinking',
            type: 'boolean',
            default: false,
            helpText: 'Let thinking-capable models reason before answering.',
          },
        ],
      },
    ],
  };

  it('is checked only on a real boolean true (v4 `:149`)', async () => {
    const on = await render({ schema, parameters: { enable_thinking: true } });
    expect(el<HTMLInputElement>(on.fixture, '#pof-enable_thinking')!.checked).toBe(true);

    const off = await render({ schema, parameters: {} });
    expect(el<HTMLInputElement>(off.fixture, '#pof-enable_thinking')!.checked).toBe(false);
  });

  it('shows the STRING "true" as OFF — v4 compares identity, not truthiness (v4 `:149`)', async () => {
    // The P4.D81 hardcoded row tolerated `'true'` because the Ollama wire side
    // does. That divergence retires with the row: v4's generic renderer shows
    // it off, and an untouched box writes nothing, so the string still reaches
    // the wire intact.
    const { fixture } = await render({ schema, parameters: { enable_thinking: 'true' } });
    expect(el<HTMLInputElement>(fixture, '#pof-enable_thinking')!.checked).toBe(false);
  });

  it('shows any other truthy value as OFF too (v4 `:149`)', async () => {
    const { fixture } = await render({ schema, parameters: { enable_thinking: 'yes please' } });
    expect(el<HTMLInputElement>(fixture, '#pof-enable_thinking')!.checked).toBe(false);
  });

  it('renders the label and helpText, and writes a real boolean (v4 `:157-165`)', async () => {
    const { fixture, writes } = await render({ schema, parameters: {} });
    expect(text(fixture)).toContain('Enable Thinking');
    expect(text(fixture)).toContain('Let thinking-capable models reason before answering.');
    const box = el<HTMLInputElement>(fixture, '#pof-enable_thinking')!;
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    expect(writes).toEqual([{ key: 'enable_thinking', value: true }]);
  });
});

describe('ProviderOptionsPanel (enum fields)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [
      {
        fields: [
          {
            key: 'keep_alive',
            label: 'Keep Model Loaded',
            type: 'enum',
            default: '',
            enumValues: [
              { value: '', label: 'Server default', description: 'Whatever your Ollama does' },
              { value: '5m', label: '5 minutes' },
              { value: '-1', label: 'Keep loaded' },
            ],
            helpText: 'How long Ollama keeps this model in memory.',
          },
        ],
      },
    ],
  };

  it('renders one option per enumValue, LABELS only — descriptions are not drawn (v4 `:192-196`)', async () => {
    const { fixture } = await render({ schema });
    const options = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLOptionElement>('option'),
    );
    expect(options.map((o) => o.value)).toEqual(['', '5m', '-1']);
    expect(options.map((o) => o.textContent!.trim())).toEqual([
      'Server default',
      '5 minutes',
      'Keep loaded',
    ]);
    expect(text(fixture)).not.toContain('Whatever your Ollama does');
  });

  it('selects the stored value, and the `` default row when the bag is empty (v4 `:180`, `:43-47`)', async () => {
    const stored = await render({ schema, parameters: { keep_alive: '5m' } });
    expect(el<HTMLSelectElement>(stored.fixture, '#pof-keep_alive')!.value).toBe('5m');

    const empty = await render({ schema });
    expect(el<HTMLSelectElement>(empty.fixture, '#pof-keep_alive')!.value).toBe('');
  });

  it('coerces a non-string stored value to the empty selection (v4 `:180`)', async () => {
    const { fixture } = await render({ schema, parameters: { keep_alive: 300 } });
    expect(el<HTMLSelectElement>(fixture, '#pof-keep_alive')!.value).toBe('');
  });

  it('selects nothing when the stored value is not among the options (v4 `:187`)', async () => {
    // React assigns `select.value` after the options mount, so a stored value
    // outside the list leaves `selectedIndex` at -1 and the control blank. The
    // panel matches by writing the value onto the element post-render for
    // exactly this case; nothing is written to the bag either way.
    const { fixture } = await render({ schema, parameters: { keep_alive: '10m' } });
    const select = el<HTMLSelectElement>(fixture, '#pof-keep_alive')!;
    expect(select.selectedIndex).toBe(-1);
    expect(select.value).toBe('');
  });

  it('writes the chosen option value verbatim (v4 `:189`)', async () => {
    const { fixture, writes } = await render({ schema });
    const select = el<HTMLSelectElement>(fixture, '#pof-keep_alive')!;
    select.value = '-1';
    select.dispatchEvent(new Event('change'));
    expect(writes).toEqual([{ key: 'keep_alive', value: '-1' }]);
  });

  it('writes the empty string for the "default" row rather than deleting the key (v4 `:189`)', async () => {
    const { fixture, writes } = await render({ schema, parameters: { keep_alive: '5m' } });
    const select = el<HTMLSelectElement>(fixture, '#pof-keep_alive')!;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    expect(writes).toEqual([{ key: 'keep_alive', value: '' }]);
  });
});

describe('ProviderOptionsPanel (number fields)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [
      {
        fields: [
          {
            key: 'request_timeout_seconds',
            label: 'Request Timeout (seconds)',
            type: 'number',
            default: 300,
            helpText: 'Leave blank for the default.',
          },
          { key: 'top_k', label: 'Top K', type: 'number' },
        ],
      },
    ],
  };

  it('renders an unset number as blank with the default as placeholder (v4 `:48-52`, `:331-334`)', async () => {
    // Absent and explicitly-default must not look identical, or the field's own
    // "leave blank for the default" is unreachable and unverifiable (Bug 72).
    const { fixture } = await render({ schema, parameters: { top_k: 20 } });
    const timeout = el<HTMLInputElement>(fixture, '#pof-request_timeout_seconds')!;
    expect(timeout.value).toBe('');
    expect(timeout.getAttribute('placeholder')).toBe('300');
    expect(el<HTMLInputElement>(fixture, '#pof-top_k')!.value).toBe('20');
  });

  it('leaves a field with no default without a placeholder at all (v4 `:334`)', async () => {
    const { fixture } = await render({ schema });
    const topK = el<HTMLInputElement>(fixture, '#pof-top_k')!;
    expect(topK.value).toBe('');
    expect(topK.hasAttribute('placeholder')).toBe(false);
  });

  it('writes a NUMBER for a numeric entry (v4 `:343`)', async () => {
    const { fixture, writes } = await render({ schema });
    const input = el<HTMLInputElement>(fixture, '#pof-top_k')!;
    input.value = '40';
    input.dispatchEvent(new Event('input'));
    expect(writes).toEqual([{ key: 'top_k', value: 40 }]);
    expect(typeof writes[0].value).toBe('number');
  });

  it('emits undefined for a cleared field — the host DELETES the key (v4 `:338-339`)', async () => {
    const { fixture, writes } = await render({ schema, parameters: { top_k: 20 } });
    const input = el<HTMLInputElement>(fixture, '#pof-top_k')!;
    input.value = '';
    input.dispatchEvent(new Event('input'));
    expect(writes).toEqual([{ key: 'top_k', value: undefined }]);
  });

  it('keeps an unparseable entry as the raw string (v4 `:343`)', async () => {
    // Unreachable through a real `<input type="number">`, which reports '' for
    // rubbish — carried because v4 carries it.
    const { fixture, writes } = await render({ schema });
    numberField(fixture, 'top_k')['onInput']('abc');
    expect(writes).toEqual([{ key: 'top_k', value: 'abc' }]);
  });

  it('hands a stored STRING to the input verbatim (v4 `:56-60`)', async () => {
    // The coercion passes the string through — v4's `toInputString` does the
    // same. Both apps then hand it to an `<input type="number">`, which
    // DISCARDS a non-numeric value and shows blank; a numeric string survives.
    const { fixture } = await render({ schema, parameters: { top_k: '20' } });
    expect(toInputString('abc')).toBe('abc');
    expect(el<HTMLInputElement>(fixture, '#pof-top_k')!.value).toBe('20');
  });
});

/**
 * Bug 72 — clearing a numeric option used to repaint the schema default with
 * the caret after it, so the next keystroke appended to it (300 → 3005).
 *
 * These mirror v4's own new cases in
 * `__tests__/unit/components/settings/provider-options-panel.test.tsx` at
 * `d123658d`, driven through the panel's REAL host round trip: `setParameter`
 * writes straight back into the `parameters` input, deleting on `undefined`
 * exactly as `ProfileModal.setParameter` does. The bug only exists in that
 * round trip.
 */
describe('ProviderOptionsPanel (number fields — Bug 72)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [
      {
        fields: [
          {
            key: 'request_timeout_seconds',
            label: 'Request Timeout (seconds)',
            type: 'number',
            default: 300,
            helpText: 'Leave blank for the default.',
          },
        ],
      },
    ],
  };

  it('stays empty when cleared, and the key leaves the bag', async () => {
    const host = await renderRoundTrip({ schema, parameters: { request_timeout_seconds: 300 } });
    const input = el<HTMLInputElement>(host.fixture, '#pof-request_timeout_seconds')!;
    expect(input.value).toBe('300');

    await clearInput(host, input);
    expect(input.value).toBe('');
    expect(host.bag()).not.toHaveProperty('request_timeout_seconds');
  });

  it('does not prepend the default to the value typed after a clear', async () => {
    const host = await renderRoundTrip({ schema, parameters: { request_timeout_seconds: 300 } });
    const input = el<HTMLInputElement>(host.fixture, '#pof-request_timeout_seconds')!;
    expect(input.value).toBe('300');

    await clearInput(host, input);
    await typeInto(host, input, '5');

    expect(input.value).toBe('5');
    expect(host.bag()['request_timeout_seconds']).toBe(5);
  });

  it('keeps a blank field absent across a reopen rather than writing the default', async () => {
    const first = await renderRoundTrip({ schema, parameters: { request_timeout_seconds: 300 } });
    await clearInput(first, el<HTMLInputElement>(first.fixture, '#pof-request_timeout_seconds')!);
    expect(first.bag()).not.toHaveProperty('request_timeout_seconds');

    // Reopening must not resurrect the default as a stored-looking value —
    // otherwise a later change to the plugin's default never reaches the
    // profiles that deliberately never set one.
    const reopened = await renderRoundTrip({ schema, parameters: first.bag() });
    expect(el<HTMLInputElement>(reopened.fixture, '#pof-request_timeout_seconds')!.value).toBe('');
  });

  it('re-seeds the box when the parameter moves for some other reason', async () => {
    const { fixture } = await render({ schema, parameters: { request_timeout_seconds: 300 } });
    fixture.componentRef.setInput('parameters', { request_timeout_seconds: 900 });
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el<HTMLInputElement>(fixture, '#pof-request_timeout_seconds')!.value).toBe('900');
  });

  it('does not rewrite the box when the host normalizes its own echo', async () => {
    // ⚠ THE MUTATION PROOF for the `syncedFrom` spelling. Reconciling the draft
    // against the incoming string alone (or a `linkedSignal` keyed on it)
    // rewrites `007` to the `7` the bag stored, under the caret — v4's commit
    // warns the naive re-sync reintroduces Bug 72 for exactly this reason.
    const host = await renderRoundTrip({ schema });
    const input = el<HTMLInputElement>(host.fixture, '#pof-request_timeout_seconds')!;
    await typeInto(host, input, '007');
    expect(host.bag()['request_timeout_seconds']).toBe(7);
    expect(input.value).toBe('007');
  });
});

describe('ProviderOptionsPanel (string fields)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [{ fields: [{ key: 'note', label: 'Note', type: 'string', helpText: 'Free text.' }] }],
  };

  it('shows the stored string and writes what is typed (v4 `:326`, `:335`)', async () => {
    const { fixture, writes } = await render({ schema, parameters: { note: 'hello' } });
    const input = el<HTMLInputElement>(fixture, '#pof-note')!;
    expect(input.value).toBe('hello');
    expect(text(fixture)).toContain('Free text.');
    input.value = 'goodbye';
    input.dispatchEvent(new Event('input'));
    expect(writes).toEqual([{ key: 'note', value: 'goodbye' }]);
  });

  it('coerces a non-string stored value to empty (v4 `:326`)', async () => {
    const { fixture } = await render({ schema, parameters: { note: 7 } });
    expect(el<HTMLInputElement>(fixture, '#pof-note')!.value).toBe('');
  });
});

describe('ProviderOptionsPanel (multi-enum fields)', () => {
  const fetched: ProviderOptionsSchema = {
    groups: [
      {
        fields: [
          {
            key: 'fallbackModels',
            label: 'Fallback Models (max 2)',
            type: 'multi-enum',
            multiEnumSource: 'fetchedModels',
            max: 2,
            default: [],
          },
        ],
      },
    ],
  };

  function boxes(fixture: ComponentFixture<unknown>): HTMLInputElement[] {
    return Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
        'input[type=checkbox]',
      ),
    );
  }

  it('draws nothing when there are no choices to offer (v4 `:232`)', async () => {
    const { fixture } = await render({ schema: fetched, fetchedModels: [] });
    expect(text(fixture)).not.toContain('Fallback Models');
  });

  it('offers the fetched models MINUS the profile’s own, capped at 50 (v4 `:220-224`)', async () => {
    const models = Array.from({ length: 60 }, (_, i) => `m${i}`);
    const { fixture } = await render({
      schema: fetched,
      fetchedModels: models,
      modelName: 'm0',
    });
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('span.truncate'),
    ).map((s) => s.textContent!.trim());
    expect(labels).toHaveLength(50);
    expect(labels).not.toContain('m0');
    expect(labels[0]).toBe('m1');
    expect(labels[49]).toBe('m50');
  });

  it('appends a ticked choice to the stored array (v4 `:256`)', async () => {
    const { fixture, writes } = await render({
      schema: fetched,
      fetchedModels: ['a', 'b', 'c'],
      parameters: { fallbackModels: ['a'] },
    });
    const b = boxes(fixture)[1];
    b.checked = true;
    b.dispatchEvent(new Event('change'));
    expect(writes).toEqual([{ key: 'fallbackModels', value: ['a', 'b'] }]);
  });

  it('removes an unticked choice (v4 `:258`)', async () => {
    const { fixture, writes } = await render({
      schema: fetched,
      fetchedModels: ['a', 'b', 'c'],
      parameters: { fallbackModels: ['a', 'b'] },
    });
    const a = boxes(fixture)[0];
    a.checked = false;
    a.dispatchEvent(new Event('change'));
    expect(writes).toEqual([{ key: 'fallbackModels', value: ['b'] }]);
  });

  it('disables the unselected rows once `max` is reached, leaving the selected ones live (v4 `:240`)', async () => {
    const { fixture } = await render({
      schema: fetched,
      fetchedModels: ['a', 'b', 'c'],
      parameters: { fallbackModels: ['a', 'b'] },
    });
    expect(boxes(fixture).map((b) => b.disabled)).toEqual([false, false, true]);
  });

  it('ignores a non-array stored value and treats the selection as empty (v4 `:216`)', async () => {
    const { fixture } = await render({
      schema: fetched,
      fetchedModels: ['a', 'b'],
      parameters: { fallbackModels: 'a' },
    });
    expect(boxes(fixture).map((b) => b.checked)).toEqual([false, false]);
  });

  it('uses fixed enumValues when no multiEnumSource is declared (v4 `:226-229`)', async () => {
    const { fixture } = await render({
      schema: {
        groups: [
          {
            fields: [
              {
                key: 'tags',
                label: 'Tags',
                type: 'multi-enum',
                enumValues: [
                  { value: 'x', label: 'Ex' },
                  { value: 'y', label: 'Why' },
                ],
              },
            ],
          },
        ],
      },
      fetchedModels: ['ignored'],
    });
    const labels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('span.truncate'),
    ).map((s) => s.textContent!.trim());
    expect(labels).toEqual(['Ex', 'Why']);
  });
});

describe('ProviderOptionsPanel (showIf)', () => {
  const schema: ProviderOptionsSchema = {
    groups: [
      {
        fields: [
          { key: 'enable_thinking', label: 'Enable Thinking', type: 'boolean', default: false },
          {
            key: 'thinking_effort',
            label: 'Thinking Effort',
            type: 'enum',
            default: '',
            showIf: { field: 'enable_thinking', equals: true },
            enumValues: [
              { value: '', label: 'Model default' },
              { value: 'low', label: 'Low' },
            ],
          },
        ],
      },
    ],
  };

  it('hides a guarded field while the sibling does not match (v4 `:39-40`)', async () => {
    const { fixture } = await render({ schema, parameters: {} });
    expect(el(fixture, '#pof-thinking_effort')).toBeNull();
    expect(text(fixture)).not.toContain('Thinking Effort');
  });

  it('shows it once the sibling holds the exact value (v4 `:40`)', async () => {
    const { fixture } = await render({ schema, parameters: { enable_thinking: true } });
    expect(el(fixture, '#pof-thinking_effort')).not.toBeNull();
  });

  it('compares by identity — the string "true" does NOT open the guard (v4 `:40`)', async () => {
    const { fixture } = await render({ schema, parameters: { enable_thinking: 'true' } });
    expect(el(fixture, '#pof-thinking_effort')).toBeNull();
  });

  it('reads the RAW bag, so a schema `default` alone never satisfies it (v4 `:40` vs `:43-47`)', async () => {
    // The guard consults `parameters[field]`, not `fieldValue()`. A sibling
    // defaulting to `true` with nothing stored keeps the guarded field hidden.
    const { fixture } = await render({
      schema: {
        groups: [
          {
            fields: [
              { key: 'gate', label: 'Gate', type: 'boolean', default: true },
              {
                key: 'guarded',
                label: 'Guarded',
                type: 'string',
                showIf: { field: 'gate', equals: true },
              },
            ],
          },
        ],
      },
      parameters: {},
    });
    expect(el<HTMLInputElement>(fixture, '#pof-gate')!.checked).toBe(true);
    expect(el(fixture, '#pof-guarded')).toBeNull();
  });

  it('re-evaluates when the bag changes (v4 re-renders on every parameters change)', async () => {
    const { fixture } = await render({ schema, parameters: {} });
    expect(el(fixture, '#pof-thinking_effort')).toBeNull();
    fixture.componentRef.setInput('parameters', { enable_thinking: true });
    fixture.detectChanges();
    expect(el(fixture, '#pof-thinking_effort')).not.toBeNull();
  });
});
