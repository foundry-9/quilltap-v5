import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CoreRequest, CoreResponse } from '../../core/core-contract';
import { Unlock } from './unlock';

function stubClient(dispatch: (req: CoreRequest) => Promise<CoreResponse>): Partial<CoreClient> {
  return { dispatch, connect: vi.fn() } as Partial<CoreClient>;
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<Unlock>> {
  TestBed.configureTestingModule({ imports: [Unlock], providers: [{ provide: CoreClient, useValue: client }] });
  const fixture = TestBed.createComponent(Unlock);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function type(fixture: ComponentFixture<Unlock>, value: string): void {
  const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('#qt-passphrase')!;
  input.value = value;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

function clickUnlock(fixture: ComponentFixture<Unlock>): void {
  const el = fixture.nativeElement as HTMLElement;
  Array.from(el.querySelectorAll('button'))
    .find((b) => b.textContent?.trim() === 'Unlock' || b.textContent?.trim() === 'Unlocking...')!
    .click();
}

describe('Unlock', () => {
  afterEach(() => sessionStorage.clear());

  it('shows the cold-start heading', async () => {
    const fixture = await render(stubClient(async () => ({ type: 'ack', data: {} })));
    expect(fixture.nativeElement.textContent).toContain('Quilltap Awaits Your Credentials');
  });

  it('emits unlocked + connects on a resolved unlock', async () => {
    const client = stubClient(async () => ({
      type: 'unlockState',
      data: { state: 'resolved', hasUserPassphrase: true },
    }));
    const fixture = await render(client);
    const unlocked = vi.fn();
    fixture.componentInstance.unlocked.subscribe(unlocked);

    type(fixture, 'open sesame');
    clickUnlock(fixture);
    await fixture.whenStable();
    fixture.detectChanges();

    expect(unlocked).toHaveBeenCalled();
    expect(client.connect).toHaveBeenCalled();
  });

  it('shows an error on a wrong passphrase and counts the attempt', async () => {
    const fixture = await render(
      stubClient(async () => ({ type: 'error', data: { kind: 'bad-request', message: 'Invalid passphrase' } })),
    );
    type(fixture, 'nope');
    clickUnlock(fixture);
    await fixture.whenStable();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Invalid passphrase');
  });

  it('disables the form and shows the exhaustion copy after three failures', async () => {
    const fixture = await render(
      stubClient(async () => ({ type: 'error', data: { kind: 'bad-request', message: 'Invalid passphrase' } })),
    );
    for (let i = 0; i < 3; i++) {
      type(fixture, `try-${i}`);
      clickUnlock(fixture);
      await fixture.whenStable();
      fixture.detectChanges();
    }
    expect(fixture.nativeElement.textContent).toContain(
      'Three attempts exhausted. You may set ENCRYPTION_MASTER_PEPPER as an environment variable and restart.',
    );
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('#qt-passphrase')!;
    expect(input.disabled).toBe(true);
  });

  it('guards an empty passphrase with the client-side message', async () => {
    const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
    const fixture = await render(stubClient(dispatch));
    clickUnlock(fixture);
    await fixture.whenStable();
    fixture.detectChanges();
    expect(dispatch).not.toHaveBeenCalled();
    expect(fixture.nativeElement.textContent).toContain('A passphrase is required to unlock.');
  });
});
