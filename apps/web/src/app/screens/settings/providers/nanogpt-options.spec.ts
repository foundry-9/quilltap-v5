import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { ThinkingTurnRule } from '../../../core/core-contract';
import { ProviderOptionsPanel } from './provider-options-panel';
import type { ProviderOptionsSchema } from './provider-options-schema';
import { evaluateThinkingTurn } from './thinking-turn';

/**
 * NanoGPT's provider options, as the server serves them and the client must
 * render and evaluate them. VERIFY-ONLY on the rendering side: v4 added no
 * bespoke editor code for NanoGPT (`781fc420` touches no panel file), so the
 * group must come out of the EXISTING schema-driven panel with zero new code.
 * A red here would mean the generic panel cannot express this schema.
 *
 * The fixture is the Shared contract of P4.D101 / P4.D102, transcribed from
 * v4's plugin `index.ts:109-147` — not read from the server, so the two halves
 * cannot drift into agreement by accident.
 */

const NANOGPT_SCHEMA: ProviderOptionsSchema = {
  groups: [
    {
      title: 'NanoGPT Options',
      fields: [
        {
          key: 'reasoning_effort',
          label: 'Reasoning Effort',
          type: 'enum',
          default: '',
          enumValues: [
            { value: '', label: '(model default)' },
            { value: 'none', label: 'None' },
            { value: 'minimal', label: 'Minimal' },
            { value: 'low', label: 'Low' },
            { value: 'medium', label: 'Medium' },
            { value: 'high', label: 'High' },
            { value: 'xhigh', label: 'XHigh' },
          ],
        },
      ],
    },
  ],
};

const NANOGPT_RULE: ThinkingTurnRule = {
  optionKey: 'reasoning_effort',
  enabledValues: ['minimal', 'low', 'medium', 'high', 'xhigh'],
  disabledValues: ['none'],
};

async function renderPanel(
  parameters: Record<string, unknown> = {},
): Promise<ComponentFixture<ProviderOptionsPanel>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ProviderOptionsPanel] });
  const fixture = TestBed.createComponent(ProviderOptionsPanel);
  fixture.componentRef.setInput('schema', NANOGPT_SCHEMA);
  fixture.componentRef.setInput('parameters', parameters);
  fixture.componentRef.setInput('fetchedModels', []);
  fixture.componentRef.setInput('modelName', '');
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

describe('NanoGPT options group (verify-only — no NanoGPT-specific client code)', () => {
  it('renders the group title and the Reasoning Effort row through the generic panel', async () => {
    const fixture = await renderPanel();
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('NanoGPT Options');
    expect(text).toContain('Reasoning Effort');
  });

  it('offers all seven enum values in the contract’s order', async () => {
    const fixture = await renderPanel();
    const select = (fixture.nativeElement as HTMLElement).querySelector('select');
    expect(select).not.toBeNull();
    const options = Array.from((select as HTMLSelectElement).options);
    expect(options.map((o) => o.value)).toEqual([
      '',
      'none',
      'minimal',
      'low',
      'medium',
      'high',
      'xhigh',
    ]);
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      '(model default)',
      'None',
      'Minimal',
      'Low',
      'Medium',
      'High',
      'XHigh',
    ]);
  });

  it('shows a stored value as the selected one', async () => {
    const fixture = await renderPanel({ reasoning_effort: 'high' });
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      'select',
    ) as HTMLSelectElement;
    expect(select.value).toBe('high');
  });
});

/**
 * The client twin of v4's partition test (`__tests__/unit/plugins/
 * nanogpt-reasoning.test.ts:46-68`): every non-blank editor value is classified
 * by exactly one side of the rule, and the blank "(model default)" value by
 * neither.
 */
describe('NanoGPT thinkingTurnRule partition (v4 nanogpt-reasoning.test.ts:46-68)', () => {
  const editorValues = (NANOGPT_SCHEMA.groups[0].fields[0].enumValues ?? []).map((v) => v.value);

  it('classifies every non-blank editor value on exactly one side', () => {
    const enabled = NANOGPT_RULE.enabledValues ?? [];
    const disabled = NANOGPT_RULE.disabledValues ?? [];
    for (const value of editorValues) {
      const hits = [enabled.includes(value), disabled.includes(value)].filter(Boolean).length;
      expect(hits, `value ${JSON.stringify(value)}`).toBe(value === '' ? 0 : 1);
    }
  });

  it('disables exactly “none” (v4 :67)', () => {
    expect(NANOGPT_RULE.disabledValues).toEqual(['none']);
  });

  it('evaluates every enabled value as a thinking turn', () => {
    for (const value of NANOGPT_RULE.enabledValues ?? []) {
      expect(
        evaluateThinkingTurn({ rule: NANOGPT_RULE, parameters: { reasoning_effort: value } }),
        `enabled ${value}`,
      ).toBe(true);
    }
  });

  it('evaluates “none” as no thinking turn even for a thinks-by-default model', () => {
    expect(
      evaluateThinkingTurn({
        rule: NANOGPT_RULE,
        parameters: { reasoning_effort: 'none' },
        model: { supportsThinking: true, thinksByDefault: true },
      }),
    ).toBe(false);
  });

  /**
   * The blank value is in neither list, so it defers to the model — which is
   * what makes a `:thinking` catalogue entry meaningful (v4 `:70-78`).
   */
  it('defers the blank value to the model’s own habit', () => {
    expect(
      evaluateThinkingTurn({
        rule: NANOGPT_RULE,
        parameters: { reasoning_effort: '' },
        model: { supportsThinking: true, thinksByDefault: true },
      }),
    ).toBe(true);
    expect(
      evaluateThinkingTurn({
        rule: NANOGPT_RULE,
        parameters: { reasoning_effort: '' },
        model: { supportsThinking: true, thinksByDefault: false },
      }),
    ).toBe(false);
    expect(evaluateThinkingTurn({ rule: NANOGPT_RULE, parameters: {} })).toBe(false);
  });
});
