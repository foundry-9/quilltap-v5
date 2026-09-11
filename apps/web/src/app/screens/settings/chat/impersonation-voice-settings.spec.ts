import { describe, expect, it } from 'vitest';

import { cardStub, checkboxes, mountCard, settingsRow, toggle } from './chat-settings.spec-harness';
import { ImpersonationVoiceSettings } from './impersonation-voice-settings';

/**
 * The Composer card's In Their Own Words toggle (v4
 * `components/settings/chat-settings/ImpersonationVoiceSettings.tsx`,
 * `686954937`) — v4's default-when-unset, the exact `chatSettingsUpdate` payload,
 * the dogfood-#6 visible-failure rule, and v4's copy byte for byte.
 */
describe('ImpersonationVoiceSettings', () => {
  it('defaults to UNCHECKED when impersonationVoiceRewrite is unset (v4 ?? false)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('renders the persisted true', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceRewrite: true }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceRewrite: false }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ impersonationVoiceRewrite: true }]);
  });

  it('PUTs false on the way back off', async () => {
    const stub = cardStub(settingsRow({ impersonationVoiceRewrite: true }));
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ impersonationVoiceRewrite: false }]);
  });

  it('surfaces a failed save instead of reverting silently (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Failed to update impersonated-line voice setting');
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });

  it("wears the shared toggle-row styling and carries v4's copy verbatim", async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ImpersonationVoiceSettings, stub);
    const el = fixture.nativeElement as HTMLElement;

    const row = el.querySelector('label.qt-settings-toggle-row');
    expect(row).toBeTruthy();
    expect(row?.querySelector('input.qt-checkbox.mt-1')).toBeTruthy();
    expect(row?.querySelector('.qt-settings-section-heading')?.textContent?.trim()).toBe(
      "Impersonated lines in the character's own words",
    );

    // v4's body copy, whitespace-collapsed the way JSX renders it.
    const body = row?.querySelector('.qt-text-small.mt-1')?.textContent ?? '';
    expect(body.replace(/\s+/g, ' ').trim()).toBe(
      "When you have taken a character's seat with the Impersonate button, your draft " +
        'is handed first to that character — their own model, their own voice — and returned ' +
        'for your inspection before a syllable reaches the room. Send the restatement, have ' +
        'it attempted afresh, retire to the composer and rewrite, or send your own words ' +
        'exactly as typed. Nothing posts until you say so. Speaking as yourself is untouched, ' +
        'as are sends that carry only attachments or tool results.',
    );
  });
});
