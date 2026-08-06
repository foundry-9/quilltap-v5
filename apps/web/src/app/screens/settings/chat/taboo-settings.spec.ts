import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { TabooSettingsDto } from '../../../core/core-contract';
import { ToastService } from '../../../ui/toast.service';
import { TabooSettings } from './taboo-settings';

/**
 * Ported case-for-case from v4's
 * `__tests__/unit/components/settings/TabooSettings.test.tsx`.
 *
 * The card is instance-scoped and self-fetching, so what is worth pinning is
 * the REQUEST CONTRACT rather than the styling: every add and remove sends the
 * WHOLE `phrases` array, and the response — which the server has already
 * normalized — is what the card renders next. The client-side guards (empty,
 * duplicate, over-long) must not reach the network at all.
 */

interface Stub {
  client: Partial<CoreClient>;
  /** Every array sent to `updateTabooSettings`, in order. */
  sent: string[][];
}

function stubClient(
  initial: string[],
  onUpdate?: (next: string[]) => string[] | Promise<never>,
): Stub {
  const sent: string[][] = [];
  return {
    sent,
    client: {
      getTabooSettings: async (): Promise<TabooSettingsDto> => ({ phrases: initial }),
      updateTabooSettings: async (phrases: string[]): Promise<TabooSettingsDto> => {
        sent.push(phrases);
        // The stub echoes the submission unless the case models the server's
        // normalization explicitly.
        return { phrases: onUpdate ? await onUpdate(phrases) : phrases };
      },
    },
  };
}

const toasts = {
  showSuccess: vi.fn(),
  showError: vi.fn(),
} as unknown as ToastService;

async function mount(client: Partial<CoreClient>): Promise<ComponentFixture<TabooSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [TabooSettings],
    providers: [
      { provide: CoreClient, useValue: client },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(TabooSettings);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<TabooSettings>): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
}

function text(fixture: ComponentFixture<TabooSettings>): string {
  return fixture.nativeElement.textContent ?? '';
}

function input(fixture: ComponentFixture<TabooSettings>): HTMLInputElement {
  return fixture.nativeElement.querySelector('input[aria-label="Forbidden phrase"]');
}

function addButton(fixture: ComponentFixture<TabooSettings>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('button[type="submit"]');
}

function type(fixture: ComponentFixture<TabooSettings>, value: string): void {
  const el = input(fixture);
  el.value = value;
  el.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

describe('TabooSettings', () => {
  it('renders the stored phrases in stored order', async () => {
    const fixture = await mount(stubClient(['weight-bearing', "that's not nothing"]).client);
    // The badge's first child span is the label; the remove button's icon
    // renders a span of its own inside the same badge.
    const badges = [
      ...fixture.nativeElement.querySelectorAll('.qt-tag-badge > span:first-child'),
    ].map((el) => (el as HTMLElement).textContent);
    expect(badges).toEqual(['weight-bearing', "that's not nothing"]);
    expect(text(fixture)).toContain('2 phrases forbidden');
  });

  it('shows the empty state when nothing is forbidden', async () => {
    const fixture = await mount(stubClient([]).client);
    expect(text(fixture)).toContain('Nothing forbidden yet');
  });

  it('surfaces a load failure instead of rendering an empty list', async () => {
    const fixture = await mount({
      getTabooSettings: async () => {
        throw new Error('Failed to fetch taboo settings');
      },
    });
    expect(text(fixture)).toContain('Failed to fetch taboo settings');
    expect(text(fixture)).not.toContain('Nothing forbidden yet');
  });

  it('sends the whole array when a phrase is added', async () => {
    const stub = stubClient(['weight-bearing']);
    const fixture = await mount(stub.client);

    type(fixture, "that's not nothing");
    addButton(fixture).click();
    await settle(fixture);

    expect(stub.sent).toEqual([['weight-bearing', "that's not nothing"]]);
    expect(text(fixture)).toContain("that's not nothing");
  });

  it('adds on Enter as well as on the Add button', async () => {
    const stub = stubClient([]);
    const fixture = await mount(stub.client);

    type(fixture, 'deeply human');
    input(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await settle(fixture);

    expect(stub.sent).toEqual([['deeply human']]);
  });

  it('ignores other keys in the phrase input', async () => {
    const stub = stubClient([]);
    const fixture = await mount(stub.client);

    type(fixture, 'deeply human');
    input(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'a' }));
    await settle(fixture);

    expect(stub.sent).toEqual([]);
  });

  it('trims the typed phrase before sending it', async () => {
    const stub = stubClient([]);
    const fixture = await mount(stub.client);

    type(fixture, '   weight-bearing   ');
    addButton(fixture).click();
    await settle(fixture);

    expect(stub.sent).toEqual([['weight-bearing']]);
  });

  it('renders the NORMALIZED list the server returns, not what was submitted', async () => {
    // The server drops case-insensitive duplicates; the card must follow it
    // rather than trusting its own optimistic arithmetic.
    const stub = stubClient(['weight-bearing'], () => ['weight-bearing']);
    const fixture = await mount(stub.client);

    type(fixture, 'tapestry');
    addButton(fixture).click();
    await settle(fixture);

    expect(stub.sent).toEqual([['weight-bearing', 'tapestry']]);
    expect(text(fixture)).not.toContain('tapestry');
    expect(text(fixture)).toContain('1 phrase forbidden');
  });

  it('sends the remaining phrases when one is removed', async () => {
    const stub = stubClient(['weight-bearing', "that's not nothing"]);
    const fixture = await mount(stub.client);

    // The aria-label embeds double quotes, so match it by value rather than
    // through an attribute selector.
    const remove = ([
      ...fixture.nativeElement.querySelectorAll('.qt-tag-badge-remove'),
    ] as HTMLButtonElement[]).find(
      (b) => b.getAttribute('aria-label') === 'Remove "weight-bearing" from the Taboo list',
    );
    expect(remove).toBeTruthy();
    remove!.click();
    await settle(fixture);

    expect(stub.sent).toEqual([["that's not nothing"]]);
  });

  it('refuses a case-insensitive duplicate without touching the network', async () => {
    const stub = stubClient(['weight-bearing']);
    const fixture = await mount(stub.client);

    type(fixture, 'WEIGHT-BEARING');
    addButton(fixture).click();
    await settle(fixture);

    expect(text(fixture)).toContain('That phrase is already forbidden.');
    expect(stub.sent).toEqual([]);
  });

  it('does not send an empty or whitespace-only phrase', async () => {
    const stub = stubClient([]);
    const fixture = await mount(stub.client);

    type(fixture, '   ');
    // The Add button disables on a blank draft, so submit the form directly —
    // the same guard v4 exercises by clicking.
    fixture.nativeElement.querySelector('form').dispatchEvent(new Event('submit'));
    await settle(fixture);

    expect(stub.sent).toEqual([]);
  });

  it('keeps the draft for a retry when the save fails', async () => {
    const stub = stubClient([], () => Promise.reject(new Error('locked')));
    const fixture = await mount(stub.client);

    type(fixture, 'weight-bearing');
    addButton(fixture).click();
    await settle(fixture);

    expect(stub.sent).toEqual([['weight-bearing']]);
    expect(input(fixture).value).toBe('weight-bearing');
  });
});
