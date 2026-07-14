import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { CompositionModeSettings } from './composition-mode-settings';

type DispatchExpect = CoreClient['dispatchExpect'];

function stubClient(initial: boolean, spy?: (req: unknown) => void): Partial<CoreClient> {
  const dispatchExpect = ((req: { type: string; settings?: Record<string, unknown> }) => {
    spy?.(req);
    if (req.type === 'chatSettingsUpdate') {
      return Promise.resolve({ data: { compositionModeDefault: req.settings?.['compositionModeDefault'] } });
    }
    return Promise.resolve({ data: { compositionModeDefault: initial } });
  }) as unknown as DispatchExpect;
  return { dispatchExpect };
}

async function mount(client: Partial<CoreClient>): Promise<ComponentFixture<CompositionModeSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CompositionModeSettings],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CompositionModeSettings);
  fixture.detectChanges();
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
  return fixture;
}

function checkbox(fixture: ComponentFixture<CompositionModeSettings>): HTMLInputElement {
  return fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
}

describe('CompositionModeSettings', () => {
  it('loads the persisted compositionModeDefault into the checkbox', async () => {
    const fixture = await mount(stubClient(true));
    expect(checkbox(fixture).checked).toBe(true);
  });

  it('defaults to unchecked when the flag is absent', async () => {
    const fixture = await mount(stubClient(false));
    expect(checkbox(fixture).checked).toBe(false);
  });

  it('persists the flag through chatSettingsUpdate on toggle', async () => {
    const spy = vi.fn();
    const fixture = await mount(stubClient(false, spy));
    const el = checkbox(fixture);
    el.checked = true;
    el.dispatchEvent(new Event('change'));
    await Promise.resolve();
    expect(spy).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'chatSettingsUpdate',
        settings: { compositionModeDefault: true },
      }),
    );
  });
});
