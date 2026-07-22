import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { VisibilitySection, type VisibilityState } from './visibility-section';

/**
 * The Visibility section's writes (v4 `VisibilitySection` + the
 * `useChatControls` handlers behind it): every control sends `{chat: {…}}`,
 * adopts optimistically, and reverts with v4's error copy on failure.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let failNext = false;

function stubClient(): Partial<CoreClient> {
  return {
    dispatch: (async (req: Req) => {
      sent.push(req);
      if (failNext) {
        failNext = false;
        return { type: 'error', data: { message: 'nope' } };
      }
      return { type: 'chat', data: { chat: {} } };
    }) as CoreClient['dispatch'],
  };
}

function state(over: Partial<VisibilityState> = {}): VisibilityState {
  return {
    allowCrossCharacterVaultReads: false,
    coreWhisperEnabled: null,
    coreWhisperInterval: null,
    turnSkippingEnabled: null,
    ...over,
  };
}

@Component({
  imports: [VisibilitySection],
  template: `
    <qt-visibility-section
      [chatId]="'chat-1'"
      [state]="visibility()"
      [turnSkippingApplies]="turnSkippingApplies()"
      [showAllWhispers]="showAllWhispers()"
      (toggleAllWhispers)="whisperToggles = whisperToggles + 1"
      (chatUpdated)="refetched = refetched + 1"
    />
  `,
})
class Host {
  readonly visibility = signal<VisibilityState>(state());
  readonly turnSkippingApplies = signal(true);
  readonly showAllWhispers = signal(false);
  whisperToggles = 0;
  refetched = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [{ provide: CoreClient, useValue: stubClient() }],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function control(fixture: ComponentFixture<Host>, label: string): HTMLElement {
  return fixture.nativeElement.querySelector(`[aria-label="${label}"]`) as HTMLElement;
}

async function choose(
  fixture: ComponentFixture<Host>,
  select: HTMLSelectElement,
  value: string,
): Promise<void> {
  select.value = value;
  select.dispatchEvent(new Event('change'));
  await fixture.whenStable();
  fixture.detectChanges();
}

async function click(fixture: ComponentFixture<Host>, el: HTMLElement): Promise<void> {
  el.click();
  await fixture.whenStable();
  fixture.detectChanges();
}

describe('VisibilitySection', () => {
  beforeEach(() => {
    sent.length = 0;
    failNext = false;
  });

  it('leaves All Whispers to the Salon (display only, no write)', async () => {
    const fixture = await render();
    await click(fixture, control(fixture, 'All Whispers'));
    expect(fixture.componentInstance.whisperToggles).toBe(1);
    expect(sent).toEqual([]);
  });

  it('writes the shared-vault toggle with v4’s info copy', async () => {
    const fixture = await render();
    await click(fixture, control(fixture, 'Shared Vaults'));
    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { allowCrossCharacterVaultReads: true } },
    ]);
    expect(fixture.nativeElement.textContent).toContain(
      'Shared vault reads enabled — characters may peek at each other’s dossiers',
    );
    expect(control(fixture, 'Shared Vaults').getAttribute('aria-checked')).toBe('true');
  });

  it('reverts and reports v4’s error copy when a write fails', async () => {
    const fixture = await render();
    failNext = true;
    await click(fixture, control(fixture, 'Shared Vaults'));
    expect(control(fixture, 'Shared Vaults').getAttribute('aria-checked')).toBe('false');
    expect(fixture.nativeElement.textContent).toContain('Could not update shared-vault setting');
    expect(fixture.componentInstance.refetched).toBe(0);
  });

  it('flips turn skipping off from the null default (v4’s === false ? true : false)', async () => {
    const fixture = await render();
    expect(control(fixture, 'Turn Skipping').getAttribute('aria-checked')).toBe('true');
    await click(fixture, control(fixture, 'Turn Skipping'));
    expect(sent[0]).toEqual({
      type: 'chatUpdate',
      chatId: 'chat-1',
      chat: { turnSkippingEnabled: false },
    });
    expect(control(fixture, 'Turn Skipping').getAttribute('aria-checked')).toBe('false');
  });

  it('hides the turn-skipping row when the chat does not qualify', async () => {
    const fixture = await render();
    fixture.componentInstance.turnSkippingApplies.set(false);
    fixture.detectChanges();
    expect(control(fixture, 'Turn Skipping')).toBeNull();
  });

  it('writes Aurora’s Core Whisper offering and cadence (the F3 third level)', async () => {
    const fixture = await render();
    const offering = control(fixture, 'Core whisper offering') as HTMLSelectElement;
    const cadence = control(fixture, 'Core whisper cadence') as HTMLSelectElement;

    expect(Array.from(cadence.options).map((o) => o.textContent?.trim())).toEqual([
      'Inherit',
      '3 turns',
      '6 turns',
      '9 turns',
      '12 turns',
      '15 turns',
      '20 turns',
      '25 turns',
      '30 turns',
      '40 turns',
      '50 turns',
    ]);

    await choose(fixture, offering, 'off');
    await choose(fixture, cadence, '12');
    await choose(fixture, cadence, '');

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { coreWhisperEnabled: false } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { coreWhisperInterval: 12 } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { coreWhisperInterval: null } },
    ]);
  });

  it('writes the thinking and answer-confirmation tri-states', async () => {
    const fixture = await render();
    await choose(fixture, control(fixture, 'Thinking visibility') as HTMLSelectElement, 'on');
    await choose(fixture, control(fixture, 'Answer confirmation') as HTMLSelectElement, 'OFF');
    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { showThinking: true } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { answerConfirmationOverride: 'OFF' } },
    ]);
    // Neither column comes back from the chat GET, so the section must keep the
    // choice (v4's `useChatControls` state does the same).
    expect((control(fixture, 'Thinking visibility') as HTMLSelectElement).value).toBe('on');
    expect((control(fixture, 'Answer confirmation') as HTMLSelectElement).value).toBe('OFF');
  });

  it('seeds the controls from the chat record', async () => {
    const fixture = await render();
    fixture.componentInstance.visibility.set(
      state({
        allowCrossCharacterVaultReads: true,
        coreWhisperEnabled: true,
        coreWhisperInterval: 25,
        turnSkippingEnabled: false,
      }),
    );
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(control(fixture, 'Shared Vaults').getAttribute('aria-checked')).toBe('true');
    expect((control(fixture, 'Core whisper offering') as HTMLSelectElement).value).toBe('on');
    expect((control(fixture, 'Core whisper cadence') as HTMLSelectElement).value).toBe('25');
    expect(control(fixture, 'Turn Skipping').getAttribute('aria-checked')).toBe('false');
  });
});
