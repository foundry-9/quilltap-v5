import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { AutoLockSettingsCard } from './auto-lock-settings-card';
import { SystemSettingsSignals } from './system-settings-signals.service';

interface Stub {
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/** A CoreClient stub over both `unlockState` and the chat-settings pair. */
function stub(opts: { hasPassphrase: boolean; autoLockSettings?: unknown }): Stub {
  const updates: Record<string, unknown>[] = [];
  let current = { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' } as ChatSettingsDto;
  if (opts.autoLockSettings !== undefined) {
    current = { ...current, autoLockSettings: opts.autoLockSettings } as ChatSettingsDto;
  }

  const dispatchExpect = vi.fn(async (req: { type: string; settings?: Record<string, unknown> }) => {
    if (req.type === 'unlockState') {
      return {
        type: 'unlockState',
        data: { state: 'unlocked', hasUserPassphrase: opts.hasPassphrase },
      };
    }
    if (req.type === 'chatSettingsUpdate') {
      updates.push(req.settings ?? {});
      current = { ...current, ...(req.settings ?? {}) } as ChatSettingsDto;
      return { type: 'chatSettings', data: current };
    }
    return { type: 'chatSettings', data: current };
  });

  return { updates, client: { dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<AutoLockSettingsCard>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AutoLockSettingsCard],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
      SystemSettingsSignals,
    ],
  });
  const fixture = TestBed.createComponent(AutoLockSettingsCard);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('AutoLockSettingsCard', () => {
  it('gates on a passphrase — shows the deadbolt copy when none is set', async () => {
    const fixture = await mount(stub({ hasPassphrase: false }));
    expect(fixture.nativeElement.textContent).toContain('Auto-lock requires a passphrase');
    expect(fixture.nativeElement.querySelector('input[type="checkbox"]')).toBeNull();
  });

  it('shows the toggle (and hides minutes while disabled) when a passphrase exists', async () => {
    const fixture = await mount(stub({ hasPassphrase: true }));
    const toggle = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(toggle).not.toBeNull();
    expect(toggle.checked).toBe(false);
    expect(fixture.nativeElement.querySelector('input[type="number"]')).toBeNull();
  });

  it('saves autoLockSettings and notifies the provider on toggle', async () => {
    const s = stub({ hasPassphrase: true });
    const fixture = await mount(s);
    const signals = TestBed.inject(SystemSettingsSignals);
    const toggle = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    toggle.checked = true;
    toggle.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(s.updates).toContainEqual({ autoLockSettings: { enabled: true, idleMinutes: 15 } });
    expect(signals.autoLockSettingsChanged()).toBe(1);
  });

  it('persists a new idle-minutes draft on blur', async () => {
    const s = stub({ hasPassphrase: true, autoLockSettings: { enabled: true, idleMinutes: 15 } });
    const fixture = await mount(s);
    const minutes = fixture.nativeElement.querySelector('input[type="number"]') as HTMLInputElement;
    expect(minutes.value).toBe('15');
    minutes.value = '5';
    minutes.dispatchEvent(new Event('input'));
    minutes.dispatchEvent(new Event('blur'));
    await settle(fixture);
    expect(s.updates).toContainEqual({ autoLockSettings: { enabled: true, idleMinutes: 5 } });
  });
});
