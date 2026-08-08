import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { BrahmaConsoleSettings } from './brahma-console-settings';

/**
 * The Brahma Console budget card (P4.D59, v4 `6452e2c3`). Mirrors the Data
 * Retention card's number-input behaviour over the `brahmaConsoleSettings` /
 * `brahmaConsoleSettingsUpdate` dispatch verbs: load-on-init, commit-on-blur,
 * revert-out-of-range-without-nagging, no-op-when-unchanged.
 *
 * The card reads/writes through `CoreClient.dispatchData`, so the stub answers at
 * that seam (the settings precedent). The PUT echoes the STORED value, which is
 * what the card renders next.
 */
function stubClient(initial: number, onUpdate?: (turns: number) => void): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; maxAgentTurns?: number }) => {
      if (req.type === 'brahmaConsoleSettings') {
        return { maxAgentTurns: initial };
      }
      if (req.type === 'brahmaConsoleSettingsUpdate') {
        onUpdate?.(req.maxAgentTurns as number);
        return { maxAgentTurns: req.maxAgentTurns };
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function mount(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<BrahmaConsoleSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaConsoleSettings],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(BrahmaConsoleSettings);
  fixture.detectChanges();
  // Flush the constructor's async load, then render.
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
  return fixture;
}

function input(fixture: ComponentFixture<BrahmaConsoleSettings>): HTMLInputElement | null {
  return fixture.nativeElement.querySelector('#brahma-max-turns');
}

describe('BrahmaConsoleSettings', () => {
  it('loads the persisted turn budget into the input', async () => {
    const fixture = await mount(stubClient(80));
    expect(input(fixture)?.value).toBe('80');
  });

  it('falls back to the default (50) when the setting is unset', async () => {
    const fixture = await mount({
      dispatchData: (async () => ({})) as unknown as CoreClient['dispatchData'],
    });
    expect(input(fixture)?.value).toBe('50');
  });

  it('persists a valid new value on blur', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '120';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).toHaveBeenCalledWith(120);
  });

  it('reverts an out-of-range entry without persisting', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '500';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges(); // render the intermediate draft so the revert is a visible change
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(onUpdate).not.toHaveBeenCalled();
    expect(input(fixture)?.value).toBe('50');
  });

  it('reverts a below-minimum entry without persisting', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '4';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(onUpdate).not.toHaveBeenCalled();
    expect(input(fixture)?.value).toBe('50');
  });

  it('skips the round-trip when the value is unchanged', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '50';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).not.toHaveBeenCalled();
  });
});
