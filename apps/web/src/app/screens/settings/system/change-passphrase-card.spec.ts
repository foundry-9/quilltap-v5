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

// ===========================================================================
// P4.D64 — the archive re-encryption sweep (v4 `ChangePassphraseCard.tsx`)
// ===========================================================================

describe('ChangePassphraseCard — archive bundles (P4.D64)', () => {
  /** A client with BOTH legs: the courtesy count read and the change dispatch. */
  function archiveClient(opts: {
    files?: unknown[] | Error;
    archives?: Record<string, unknown>;
    seen?: Array<Record<string, unknown>>;
  }): Partial<CoreClient> {
    return {
      dispatch: (async (req: CoreRequest) => {
        opts.seen?.push(req as unknown as Record<string, unknown>);
        return {
          type: 'ack',
          data: opts.archives ? { archives: opts.archives } : {},
        } as CoreResponse;
      }) as CoreClient['dispatch'],
      dispatchData: (async (req: Record<string, unknown>) => {
        opts.seen?.push(req);
        if (req['type'] === 'filesList') {
          if (opts.files instanceof Error) throw opts.files;
          return { files: opts.files ?? [] };
        }
        return {};
      }) as CoreClient['dispatchData'],
    };
  }

  async function settle(fixture: ComponentFixture<ChangePassphraseCard>): Promise<void> {
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  async function change(fixture: ComponentFixture<ChangePassphraseCard>): Promise<void> {
    type(field(fixture, 'cp-new'), 'alpha');
    type(field(fixture, 'cp-confirm'), 'alpha');
    fixture.detectChanges();
    submitBtn(fixture).click();
    await settle(fixture);
  }

  it('asks for the ARCHIVE category on mount and warns, pluralized', async () => {
    const seen: Array<Record<string, unknown>> = [];
    const { fixture } = await mount(archiveClient({ files: [{ id: 'a' }, { id: 'b' }], seen }));
    await settle(fixture);
    expect(seen).toContainEqual({ type: 'filesList', category: 'ARCHIVE' });
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain(
      'Your 2 archived-character bundles are sealed under this passphrase and will each be rewritten to open with the new one.',
    );
    expect(text).toContain(
      'This cannot be interrupted halfway without leaving some archives on the old passphrase; if that happens, the ones left behind are named below.',
    );
  });

  it('singularizes one bundle', async () => {
    const { fixture } = await mount(archiveClient({ files: [{ id: 'a' }] }));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain(
      'Your 1 archived-character bundle is sealed under this passphrase and will be rewritten to open with the new one.',
    );
  });

  it('says nothing when there are no bundles, or when the count read fails', async () => {
    const { fixture } = await mount(archiveClient({ files: [] }));
    await settle(fixture);
    expect(fixture.nativeElement.textContent).not.toContain('archived-character bundle');

    const failed = await mount(archiveClient({ files: new Error('no files route') }));
    await settle(failed.fixture);
    // The count is a courtesy: a failure must not surface an error either.
    expect(failed.fixture.nativeElement.textContent).not.toContain('archived-character bundle');
    expect(failed.fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
  });

  it('appends the all-rewritten sentence to the success alert', async () => {
    const { fixture } = await mount(
      archiveClient({
        files: [{ id: 'a' }, { id: 'b' }, { id: 'c' }],
        archives: { total: 3, reencrypted: 3, failures: [] },
      }),
    );
    await settle(fixture);
    await change(fixture);
    const alert = fixture.nativeElement.querySelector('.qt-alert-success') as HTMLElement;
    expect(alert.textContent).toContain(
      'All 3 archived-character bundles were rewritten under the new passphrase.',
    );
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
  });

  it('singularizes the rewritten sentence', async () => {
    const { fixture } = await mount(
      archiveClient({ files: [{ id: 'a' }], archives: { total: 1, reencrypted: 1, failures: [] } }),
    );
    await settle(fixture);
    await change(fixture);
    expect(
      (fixture.nativeElement.querySelector('.qt-alert-success') as HTMLElement).textContent,
    ).toContain('Your archived-character bundle was rewritten under the new passphrase.');
  });

  it('names the bundles left behind, and suppresses the all-rewritten sentence', async () => {
    // A partial sweep speaks through the error alert ALONE — claiming "all
    // rewritten" beside a list of failures would be a lie.
    const { fixture } = await mount(
      archiveClient({
        files: [{ id: 'a' }, { id: 'b' }],
        archives: {
          total: 2,
          reencrypted: 1,
          failures: [{ fileId: 'f2', filename: 'marchpane.qtap', reason: 'bad passphrase' }],
        },
      }),
    );
    await settle(fixture);
    await change(fixture);
    const error = fixture.nativeElement.querySelector('.qt-alert-error') as HTMLElement;
    expect(error.textContent).toContain(
      'One archived-character bundle could not be rewritten and still expects the old passphrase:',
    );
    expect(error.textContent).toContain('marchpane.qtap — bad passphrase');
    expect(
      (fixture.nativeElement.querySelector('.qt-alert-success') as HTMLElement).textContent,
    ).not.toContain('rewritten under the new passphrase');
  });

  it('pluralizes the failure heading', async () => {
    const { fixture } = await mount(
      archiveClient({
        files: [{ id: 'a' }, { id: 'b' }],
        archives: {
          total: 2,
          reencrypted: 0,
          failures: [
            { fileId: 'f1', filename: 'one.qtap', reason: 'locked' },
            { fileId: 'f2', filename: 'two.qtap', reason: 'locked' },
          ],
        },
      }),
    );
    await settle(fixture);
    await change(fixture);
    expect(
      (fixture.nativeElement.querySelector('.qt-alert-error') as HTMLElement).textContent,
    ).toContain(
      '2 archived-character bundles could not be rewritten and still expect the old passphrase:',
    );
  });

  it('keeps the pre-change count when the sweep itself failed (total -1)', async () => {
    const { fixture } = await mount(
      archiveClient({
        files: [{ id: 'a' }, { id: 'b' }],
        archives: { total: -1, reencrypted: 0, failures: [] },
      }),
    );
    await settle(fixture);
    await change(fixture);
    // `total: -1` means the sweep could not run; adopting it would render
    // "Your -1 archived-character bundles…" on the next visit.
    expect(fixture.nativeElement.textContent).toContain('Your 2 archived-character bundles');
    expect(fixture.nativeElement.textContent).not.toContain('-1 archived-character');
  });
});
