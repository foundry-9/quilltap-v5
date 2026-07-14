import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { DataRetentionSettingsDto } from '../../../core/core-contract';
import { DataRetentionSettings } from './data-retention-settings';

function stubClient(
  initial: number,
  onUpdate?: (days: number) => void,
): Partial<CoreClient> {
  return {
    getDataRetentionSettings: async (): Promise<DataRetentionSettingsDto> => ({
      staleChatDays: initial,
    }),
    updateDataRetentionSettings: async (staleChatDays: number): Promise<DataRetentionSettingsDto> => {
      onUpdate?.(staleChatDays);
      return { staleChatDays };
    },
  };
}

async function mount(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<DataRetentionSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [DataRetentionSettings],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(DataRetentionSettings);
  fixture.detectChanges();
  // Flush the constructor's async load, then render.
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
  return fixture;
}

function input(fixture: ComponentFixture<DataRetentionSettings>): HTMLInputElement | null {
  return fixture.nativeElement.querySelector('#stale-chat-days');
}

describe('DataRetentionSettings', () => {
  it('loads the persisted stale-chat window into the input', async () => {
    const fixture = await mount(stubClient(45));
    expect(input(fixture)?.value).toBe('45');
  });

  it('persists a valid new value on blur', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(30, onUpdate));
    const el = input(fixture)!;
    el.value = '90';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).toHaveBeenCalledWith(90);
  });

  it('reverts an out-of-range entry without persisting', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(30, onUpdate));
    const el = input(fixture)!;
    el.value = '5000';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges(); // render the intermediate draft so the revert is a visible change
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(onUpdate).not.toHaveBeenCalled();
    expect(input(fixture)?.value).toBe('30');
  });

  it('skips the round-trip when the value is unchanged', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(30, onUpdate));
    const el = input(fixture)!;
    el.value = '30';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).not.toHaveBeenCalled();
  });
});
