import type { ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { cardStub, mountCard, settingsRow, settle } from './chat-settings.spec-harness';
import { ImpersonationVoiceSettings } from './impersonation-voice-settings';

/**
 * The Composer card's "Impersonated lines in the character's own words" MODE
 * (v4 `components/settings/chat-settings/ImpersonationVoiceSettings.tsx` at
 * `07b8f0209` — three radios in place of `686954937`'s checkbox).
 *
 * The first six `it`s are v4's re-shaped
 * `__tests__/unit/components/settings/ImpersonationVoiceSettings.test.tsx`,
 * transcribed case for case with v4's names (v4 finds each radio by role +
 * label regex; v5 by the label row's `.font-medium` text). The rest pin what v4's
 * spec leaves to the component: the exact `chatSettingsUpdate` payload, the
 * dogfood-#6 visible failure, and every byte of the copy.
 *
 * Off by default matters here: a row that defaulted otherwise would quietly
 * open a dialog on every impersonated line on a fresh instance.
 */

function radio(fixture: ComponentFixture<unknown>, label: string): HTMLInputElement {
  const rows = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('label.qt-settings-toggle-row'),
  );
  const row = rows.find((r) => r.querySelector('.font-medium')?.textContent?.trim() === label);
  const input = row?.querySelector('input[type="radio"]');
  if (!(input instanceof HTMLInputElement)) throw new Error(`no radio labelled ${label}`);
  return input;
}

/** Pick a radio the way a user would, then let the save settle. */
async function pick(fixture: ComponentFixture<unknown>, el: HTMLInputElement): Promise<void> {
  el.click();
  await settle(fixture);
}

describe('ImpersonationVoiceSettings', () => {
  it('selects Never when the field has never been set', async () => {
    const fixture = await mountCard(ImpersonationVoiceSettings, cardStub(settingsRow()));
    expect(radio(fixture, 'Never').checked).toBe(true);
    expect(radio(fixture, 'Ask each time').checked).toBe(false);
    expect(radio(fixture, 'Always restate').checked).toBe(false);
  });

  it('reflects a stored ask', async () => {
    const fixture = await mountCard(
      ImpersonationVoiceSettings,
      cardStub(settingsRow({ impersonationVoiceMode: 'ask' })),
    );
    expect(radio(fixture, 'Ask each time').checked).toBe(true);
  });

  it('reflects a stored always', async () => {
    const fixture = await mountCard(
      ImpersonationVoiceSettings,
      cardStub(settingsRow({ impersonationVoiceMode: 'always' })),
    );
    expect(radio(fixture, 'Always restate').checked).toBe(true);
  });

  it('reports the chosen mode', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'off' }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await pick(fixture, radio(fixture, 'Ask each time'));
    await pick(fixture, radio(fixture, 'Always restate'));
    // The bare key and nothing else, once per pick (Shared contract item 5).
    expect(stub.updates).toEqual([
      { impersonationVoiceMode: 'ask' },
      { impersonationVoiceMode: 'always' },
    ]);
  });

  it('is disabled while a save is in flight', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'off' }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    // Hold the save open: the card's `saving` is what v4's `saving` prop is.
    const dispatch = stub.client.dispatchExpect as unknown as {
      mockImplementationOnce(fn: () => Promise<never>): void;
    };
    dispatch.mockImplementationOnce(() => new Promise<never>(() => {}));
    radio(fixture, 'Ask each time').click();
    await settle(fixture);
    expect(radio(fixture, 'Ask each time').disabled).toBe(true);
    expect(radio(fixture, 'Never').disabled).toBe(true);
    expect(radio(fixture, 'Always restate').disabled).toBe(true);
  });

  it('names the Impersonate button as the trigger and says what is left alone', async () => {
    const fixture = await mountCard(ImpersonationVoiceSettings, cardStub(settingsRow()));
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toMatch(/Impersonate button/);
    expect(text).toMatch(/Speaking as yourself is untouched/);
  });

  it('picking the mode already stored saves nothing (a radio fires no change on itself)', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'ask' }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await pick(fixture, radio(fixture, 'Ask each time'));
    expect(stub.updates).toEqual([]);
  });

  it('never sends the retired boolean', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'off' }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await pick(fixture, radio(fixture, 'Always restate'));
    await pick(fixture, radio(fixture, 'Never'));
    expect(stub.updates).toEqual([
      { impersonationVoiceMode: 'always' },
      { impersonationVoiceMode: 'off' },
    ]);
    for (const u of stub.updates) expect(Object.keys(u)).toEqual(['impersonationVoiceMode']);
  });

  it('surfaces a failed save instead of reverting silently (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await pick(fixture, radio(fixture, 'Ask each time'));
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Failed to update impersonated-line voice setting');
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });

  it('a refused save leaves the STORED mode lit, not the clicked one (v4 controlled `checked`)', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'off' }), true);
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await pick(fixture, radio(fixture, 'Always restate'));
    expect(radio(fixture, 'Never').checked).toBe(true);
    expect(radio(fixture, 'Always restate').checked).toBe(false);
  });

  it('a successful save never re-lights the old radio, even before the query notifies', async () => {
    // The §3 review of the `07b8f0209` unification: the query's `data()` reaches
    // `mode()` a macrotask after `setQueryData`, so a resync on SUCCESS would
    // flip the just-picked radio back for a beat (and race Playwright's
    // `.check()`). Read the DOM the instant `onChange` resolves — no `settle()`.
    const stub = cardStub(settingsRow({ impersonationVoiceMode: 'off' }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    const ask = radio(fixture, 'Ask each time');
    ask.checked = true; // what the user's click does before `change` fires
    await (fixture.componentInstance as unknown as {
      onChange(v: string): Promise<void>;
    }).onChange('ask');
    expect(stub.updates).toEqual([{ impersonationVoiceMode: 'ask' }]);
    expect(ask.checked).toBe(true);
    expect(radio(fixture, 'Never').checked).toBe(false);
    await settle(fixture);
    expect(ask.checked).toBe(true);
    expect(radio(fixture, 'Never').checked).toBe(false);
  });

  it("carries v4's structure, classes and copy verbatim (`ImpersonationVoiceSettings.tsx:11-71`)", async () => {
    const fixture = await mountCard(ImpersonationVoiceSettings, cardStub(settingsRow()));
    const el = fixture.nativeElement as HTMLElement;
    const collapse = (t: string | null | undefined) => (t ?? '').replace(/\s+/g, ' ').trim();

    const fieldset = el.querySelector('fieldset');
    expect(fieldset).toBeTruthy();
    expect(
      collapse(fieldset?.querySelector('legend.qt-settings-section-heading')?.textContent),
    ).toBe("Impersonated lines in the character's own words");

    // The paragraph is the fieldset's own `.qt-text-small.mt-1` child (v4 `:45-50`).
    const body = fieldset?.querySelector(':scope > .qt-text-small.mt-1');
    expect(collapse(body?.textContent)).toBe(
      "When you have taken a character's seat with the Impersonate button, your draft may " +
        'be handed first to that character — their own model, their own voice — and returned for ' +
        'your inspection before a syllable reaches the room. Nothing posts until you say so. ' +
        'Speaking as yourself is untouched, as are sends that carry only attachments or tool results.',
    );

    // Three rows under `mt-3 space-y-2`, in v4's MODES order (`:11-31`).
    const rows = Array.from(
      fieldset?.querySelectorAll(':scope > .mt-3.space-y-2 > label.qt-settings-toggle-row') ?? [],
    );
    expect(
      rows.map((r) => {
        const input = r.querySelector('input') as HTMLInputElement;
        return {
          type: input.type,
          name: input.name,
          value: input.value,
          classes: input.className,
          label: collapse(r.querySelector('.flex-1 > .font-medium')?.textContent),
          description: collapse(r.querySelector('.flex-1 > .qt-text-small.mt-1')?.textContent),
        };
      }),
    ).toEqual([
      {
        type: 'radio',
        name: 'impersonationVoiceMode',
        value: 'off',
        classes: 'qt-radio mt-1',
        label: 'Never',
        description:
          'Your line goes straight to the room, exactly as typed. No dialog, no rehearsal.',
      },
      {
        type: 'radio',
        name: 'impersonationVoiceMode',
        value: 'ask',
        classes: 'qt-radio mt-1',
        label: 'Ask each time',
        description:
          'The dialog opens with your draft and nothing more — no model is troubled. Send it as ' +
          'written, or ask the character to restate it in their own voice.',
      },
      {
        type: 'radio',
        name: 'impersonationVoiceMode',
        value: 'always',
        classes: 'qt-radio mt-1',
        label: 'Always restate',
        description:
          'The dialog opens and the character begins restating your draft at once. You may ' +
          'still send your own words as written.',
      },
    ]);
  });
});
