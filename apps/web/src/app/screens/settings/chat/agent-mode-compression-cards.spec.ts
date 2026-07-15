import type { ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { AgentModeSettings } from './agent-mode-settings';
import { cardStub, checkboxes, mountCard, settingsRow, settle, toggle } from './chat-settings.spec-harness';
import { ContextCompressionSettings } from './context-compression-settings';

/**
 * Agent Mode + Context Compression (P4.6an unit 5).
 *
 * The compression card's specs drive the sliders through v4's real drag
 * protocol (mousedown → input → mouseup), because the protocol IS the behavior:
 * a slider that PUT on every `input` would hammer the server across a drag.
 */

function slider(fixture: ComponentFixture<unknown>, id: string): HTMLInputElement {
  return (fixture.nativeElement as HTMLElement).querySelector(`#${id}`) as HTMLInputElement;
}

/** One complete drag: press, move to `to`, release. */
async function drag(
  fixture: ComponentFixture<unknown>,
  el: HTMLInputElement,
  to: number,
): Promise<void> {
  el.dispatchEvent(new Event('mousedown'));
  await settle(fixture);
  el.value = String(to);
  el.dispatchEvent(new Event('input'));
  await settle(fixture);
  el.dispatchEvent(new Event('mouseup'));
  await settle(fixture);
}

function compressionRow(over: Record<string, unknown> = {}) {
  return settingsRow({
    contextCompressionSettings: {
      enabled: true,
      windowSize: 5,
      compressionTargetTokens: 800,
      systemPromptTargetTokens: 1500,
      projectContextReinjectInterval: 5,
      ...over,
    },
  });
}

describe('AgentModeSettings', () => {
  it('renders v4\'s defaults when the bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AgentModeSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#agent-max-turns',
    ) as HTMLSelectElement;
    expect(select.value).toBe('10');
  });

  it('displays the stored maxTurns (the select\'s options are [selected]-bound)', async () => {
    const stub = cardStub(settingsRow({ agentModeSettings: { maxTurns: 25, defaultEnabled: true } }));
    const fixture = await mountCard(AgentModeSettings, stub);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#agent-max-turns',
    ) as HTMLSelectElement;
    expect(select.value).toBe('25');
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('PUTs the whole bag on the defaultEnabled toggle (v4 handler 1 of 2)', async () => {
    const stub = cardStub(settingsRow({ agentModeSettings: { maxTurns: 15, defaultEnabled: false } }));
    const fixture = await mountCard(AgentModeSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ agentModeSettings: { maxTurns: 15, defaultEnabled: true } }]);
  });

  it('PUTs maxTurns as a NUMBER, not the select\'s string (v4 parseInt)', async () => {
    const stub = cardStub(settingsRow({ agentModeSettings: { maxTurns: 10, defaultEnabled: true } }));
    const fixture = await mountCard(AgentModeSettings, stub);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#agent-max-turns',
    ) as HTMLSelectElement;
    select.value = '20';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(stub.updates).toEqual([{ agentModeSettings: { maxTurns: 20, defaultEnabled: true } }]);
    expect(typeof (stub.updates[0]['agentModeSettings'] as { maxTurns: unknown }).maxTurns).toBe(
      'number',
    );
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(AgentModeSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update agent mode settings',
    );
  });
});

describe('ContextCompressionSettings', () => {
  it('renders v4\'s defaults when the bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ContextCompressionSettings, stub);
    expect(slider(fixture, 'compression-window').value).toBe('5');
    expect(slider(fixture, 'compression-target').value).toBe('800');
    const sw = (fixture.nativeElement as HTMLElement).querySelector('[role="switch"]');
    expect(sw?.getAttribute('aria-checked')).toBe('true');
  });

  it('toggles enabled through the switch, PUTting the whole bag', async () => {
    const stub = cardStub(compressionRow());
    const fixture = await mountCard(ContextCompressionSettings, stub);
    (
      (fixture.nativeElement as HTMLElement).querySelector('[role="switch"]') as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(stub.updates).toEqual([
      {
        contextCompressionSettings: {
          enabled: false,
          windowSize: 5,
          compressionTargetTokens: 800,
          systemPromptTargetTokens: 1500,
          projectContextReinjectInterval: 5,
        },
      },
    ]);
  });

  it('does NOT save while a slider is being dragged — only on release', async () => {
    const stub = cardStub(compressionRow());
    const fixture = await mountCard(ContextCompressionSettings, stub);
    const el = slider(fixture, 'compression-target');
    el.dispatchEvent(new Event('mousedown'));
    await settle(fixture);
    for (const v of ['900', '1000', '1100']) {
      el.value = v;
      el.dispatchEvent(new Event('input'));
      await settle(fixture);
    }
    expect(stub.updates).toEqual([]);
    // The label tracks the drag even though nothing has been written.
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('~1100 tokens');
    el.dispatchEvent(new Event('mouseup'));
    await settle(fixture);
    expect(stub.updates).toHaveLength(1);
    expect(stub.updates[0]['contextCompressionSettings']).toMatchObject({
      compressionTargetTokens: 1100,
    });
  });

  it('skips the round-trip when a drag ends where it began', async () => {
    const stub = cardStub(compressionRow());
    const fixture = await mountCard(ContextCompressionSettings, stub);
    await drag(fixture, slider(fixture, 'compression-target'), 800);
    expect(stub.updates).toEqual([]);
  });

  it('commits a window-size drag, preserving the untouched bag keys', async () => {
    const stub = cardStub(compressionRow({ windowSize: 5, projectContextReinjectInterval: 10 }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    await drag(fixture, slider(fixture, 'compression-window'), 8);
    expect(stub.updates).toEqual([
      {
        contextCompressionSettings: {
          enabled: true,
          windowSize: 8,
          compressionTargetTokens: 800,
          systemPromptTargetTokens: 1500,
          projectContextReinjectInterval: 10,
        },
      },
    ]);
  });

  it('pushes the re-injection interval up with the window when the window overtakes it', async () => {
    // Interval 5, window dragged to 9 — the interval may not be shorter than the
    // window, so ONE put carries both keys (v4 handleWindowSizeCommitWithValidation).
    const stub = cardStub(compressionRow({ windowSize: 5, projectContextReinjectInterval: 5 }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    await drag(fixture, slider(fixture, 'compression-window'), 9);
    expect(stub.updates).toEqual([
      {
        contextCompressionSettings: {
          enabled: true,
          windowSize: 9,
          compressionTargetTokens: 800,
          systemPromptTargetTokens: 1500,
          projectContextReinjectInterval: 9,
        },
      },
    ]);
  });

  it('leaves a "never" (0) interval alone when the window grows', async () => {
    const stub = cardStub(compressionRow({ windowSize: 3, projectContextReinjectInterval: 0 }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    await drag(fixture, slider(fixture, 'compression-window'), 10);
    expect(stub.updates[0]['contextCompressionSettings']).toMatchObject({
      windowSize: 10,
      projectContextReinjectInterval: 0,
    });
  });

  it('clamps an interval drag to the window floor', async () => {
    const stub = cardStub(compressionRow({ windowSize: 7, projectContextReinjectInterval: 10 }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    // Try to drag the interval below the window; it must stop at the window.
    await drag(fixture, slider(fixture, 'compression-interval'), 4);
    expect(stub.updates[0]['contextCompressionSettings']).toMatchObject({
      projectContextReinjectInterval: 7,
    });
  });

  it('renders "never" for a 0 interval', async () => {
    const stub = cardStub(compressionRow({ projectContextReinjectInterval: 0 }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('(every never)');
  });

  it('disables the sliders while compression is off', async () => {
    const stub = cardStub(compressionRow({ enabled: false }));
    const fixture = await mountCard(ContextCompressionSettings, stub);
    expect(slider(fixture, 'compression-window').disabled).toBe(true);
    expect(slider(fixture, 'compression-target').disabled).toBe(true);
    expect(slider(fixture, 'compression-interval').disabled).toBe(true);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(compressionRow(), true);
    const fixture = await mountCard(ContextCompressionSettings, stub);
    await drag(fixture, slider(fixture, 'compression-target'), 1200);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update context compression settings',
    );
  });
});
