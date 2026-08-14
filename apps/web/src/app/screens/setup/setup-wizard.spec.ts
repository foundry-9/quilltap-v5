import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreRequest, CoreResponse } from '../../core/core-contract';
import { SetupWizard } from './setup-wizard';

function stubClient(dispatch: (req: CoreRequest) => Promise<CoreResponse>): Partial<CoreClient> {
  return { dispatch, connect: vi.fn() } as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<SetupWizard>> {
  TestBed.configureTestingModule({
    imports: [SetupWizard],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(SetupWizard);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function clickButton(fixture: ComponentFixture<SetupWizard>, label: string): void {
  const el = fixture.nativeElement as HTMLElement;
  const button = Array.from(el.querySelectorAll('button')).find((b) => b.textContent?.trim() === label);
  if (!button) {
    throw new Error(`no button "${label}"; buttons: ${Array.from(el.querySelectorAll('button')).map((b) => b.textContent?.trim())}`);
  }
  button.click();
}

function setInput(fixture: ComponentFixture<SetupWizard>, id: string, value: string): void {
  const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(`#${id}`);
  if (!input) {
    throw new Error(`no input #${id}`);
  }
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

describe('SetupWizard', () => {
  it('shows the Welcome copy in needs-setup mode', async () => {
    const fixture = await render(stubClient(async () => ({ type: 'ack', data: {} })));
    expect(fixture.nativeElement.textContent).toContain('Welcome to Quilltap');
    expect(fixture.nativeElement.textContent).toContain('Set Up without Passphrase');
  });

  it('dispatches setup and reveals the pepper once with the one-time warning', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({
      type: 'setup',
      data: { pepper: 'PEPPER-SHOWN-ONCE', requiresRestart: false, message: 'Encryption key generated and stored. Save this value — it will not be displayed again.' },
    }));
    const fixture = await render(stubClient(dispatch));

    clickButton(fixture, 'Set Up without Passphrase');
    await fixture.whenStable();
    fixture.detectChanges();

    expect(dispatch).toHaveBeenCalledWith({ type: 'setup', passphrase: '' });
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Setup Complete');
    expect(text).toContain('Save this encryption pepper now. It will not be shown again.');
    expect(text).toContain('PEPPER-SHOWN-ONCE');
    expect(text).toContain('ENCRYPTION_MASTER_PEPPER');
    expect(text).toContain('Continue to Quilltap');
    // The happy path says nothing about restarting.
    expect(text).not.toContain('One last formality');
  });

  // P4.46 / v4 bug 64: setup wrote the key and the instance, but the databases
  // would not reopen. The pepper reveal STILL happens (it is displayed exactly
  // once, so it is never withheld behind an error) — with the restart notice
  // beside it and v4's "Continue Anyway" label on the button.
  it('reveals the pepper WITH the restart notice when requiresRestart is set', async () => {
    const fixture = await render(
      stubClient(async () => ({
        type: 'setup',
        data: { pepper: 'PEPPER-SHOWN-ONCE', requiresRestart: true, message: 'm' },
      })),
    );

    clickButton(fixture, 'Set Up without Passphrase');
    await fixture.whenStable();
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('PEPPER-SHOWN-ONCE');
    expect(text).toContain('One last formality: please restart Quilltap.');
    expect(text).toContain('the strongroom door');
    expect(text).toContain('Continue Anyway');
    expect(text).not.toContain('Continue to Quilltap');
  });

  it('validates a passphrase mismatch before dispatching', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const fixture = await render(stubClient(dispatch));

    setInput(fixture, 'qt-setup-pass', 'longenough');
    fixture.detectChanges();
    setInput(fixture, 'qt-setup-confirm', 'different');
    fixture.detectChanges();
    clickButton(fixture, 'Set Up with Passphrase');
    await fixture.whenStable();
    fixture.detectChanges();

    expect(dispatch).not.toHaveBeenCalled();
    expect(fixture.nativeElement.textContent).toContain('Passphrases do not match');
  });

  it('validates a too-short passphrase', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const fixture = await render(stubClient(dispatch));

    setInput(fixture, 'qt-setup-pass', 'short');
    fixture.detectChanges();
    setInput(fixture, 'qt-setup-confirm', 'short');
    fixture.detectChanges();
    clickButton(fixture, 'Set Up with Passphrase');
    await fixture.whenStable();
    fixture.detectChanges();

    expect(dispatch).not.toHaveBeenCalled();
    expect(fixture.nativeElement.textContent).toContain('Passphrase must be at least 8 characters');
  });

  it('emits complete when the user continues from the pepper reveal', async () => {
    const fixture = await render(
      stubClient(async () => ({ type: 'setup', data: { pepper: 'P', requiresRestart: false, message: 'm' } })),
    );
    const completed = vi.fn();
    fixture.componentInstance.complete.subscribe(completed);

    clickButton(fixture, 'Set Up without Passphrase');
    await fixture.whenStable();
    fixture.detectChanges();
    clickButton(fixture, 'Continue to Quilltap');

    expect(completed).toHaveBeenCalled();
  });
});
