import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CoreRequest, CoreResponse } from '../../../core/core-contract';
import { ChangePassphraseCard } from './change-passphrase-card';
import { SystemSettingsSignals } from './system-settings-signals.service';

function stubClient(dispatch: (req: CoreRequest) => Promise<CoreResponse>): Partial<CoreClient> {
  return { dispatch: dispatch as CoreClient['dispatch'] };
}

async function mount(
  client: Partial<CoreClient>,
): Promise<{ fixture: ComponentFixture<ChangePassphraseCard>; signals: SystemSettingsSignals }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ChangePassphraseCard],
    providers: [{ provide: CoreClient, useValue: client }, SystemSettingsSignals],
  });
  const fixture = TestBed.createComponent(ChangePassphraseCard);
  fixture.detectChanges();
  const signals = TestBed.inject(SystemSettingsSignals);
  return { fixture, signals };
}

function field(fixture: ComponentFixture<ChangePassphraseCard>, id: string): HTMLInputElement {
  return fixture.nativeElement.querySelector(`#${id}`);
}

function type(input: HTMLInputElement, value: string): void {
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

function submitBtn(fixture: ComponentFixture<ChangePassphraseCard>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button[type="submit"]');
}

describe('ChangePassphraseCard', () => {
  beforeEach(() => {
    // A stable success envelope for the happy paths.
  });

  it('shows a mismatch warning and disables submit when new/confirm differ', async () => {
    const { fixture } = await mount(stubClient(async () => ({ type: 'ack', data: {} })));
    type(field(fixture, 'cp-new'), 'alpha');
    type(field(fixture, 'cp-confirm'), 'beta');
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Passphrases do not match');
    expect(submitBtn(fixture).disabled).toBe(true);
  });

  it('dispatches changePassphrase, shows success, resets the form and notifies', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const { fixture, signals } = await mount(stubClient(dispatch));
    type(field(fixture, 'cp-current'), 'old-secret');
    type(field(fixture, 'cp-new'), 'new-secret');
    type(field(fixture, 'cp-confirm'), 'new-secret');
    fixture.detectChanges();

    submitBtn(fixture).click();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();

    expect(dispatch).toHaveBeenCalledWith({
      type: 'changePassphrase',
      oldPassphrase: 'old-secret',
      newPassphrase: 'new-secret',
    });
    expect(fixture.nativeElement.textContent).toContain('Passphrase changed successfully');
    // The form reset clears every field.
    expect(field(fixture, 'cp-new').value).toBe('');
    expect(signals.passphraseChanged()).toBe(1);
  });

  it('allows the empty-new removal path (both new + confirm empty)', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const { fixture } = await mount(stubClient(dispatch));
    type(field(fixture, 'cp-current'), 'old-secret');
    // Leave new + confirm empty — the removal sentinel.
    fixture.detectChanges();
    expect(submitBtn(fixture).disabled).toBe(false);

    submitBtn(fixture).click();
    await Promise.resolve();
    expect(dispatch).toHaveBeenCalledWith({
      type: 'changePassphrase',
      oldPassphrase: 'old-secret',
      newPassphrase: '',
    });
  });

  it('surfaces the server error message on an error envelope', async () => {
    const dispatch = vi.fn(
      async (): Promise<CoreResponse> => ({
        type: 'error',
        data: { kind: 'invalid_input', message: 'Current passphrase is wrong' },
      }),
    );
    const { fixture, signals } = await mount(stubClient(dispatch));
    type(field(fixture, 'cp-new'), 'x');
    type(field(fixture, 'cp-confirm'), 'x');
    fixture.detectChanges();
    submitBtn(fixture).click();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('Current passphrase is wrong');
    expect(signals.passphraseChanged()).toBe(0);
  });
});
