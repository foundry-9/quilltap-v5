import { describe, expect, it } from 'vitest';

import { cardStub, checkboxes, mountCard, settingsRow, toggle } from './chat-settings.spec-harness';
import { ComposerSlashCommandsSettings } from './composer-slash-commands-settings';

/**
 * The Slash Commands toggle (v4 `ComposerSlashCommandsSettings.tsx`,
 * `4877fc2da`), on the scalar-toggle card shape of
 * `composer-char-insert-cards.spec.ts`: v4's default-when-unset, the exact
 * `chatSettingsUpdate` payload, v4's copy, and the dogfood-#6 rule that a
 * failed save surfaces visibly.
 *
 * @module screens/settings/chat/composer-slash-commands-settings.spec
 */
describe('ComposerSlashCommandsSettings', () => {
  it('defaults to checked when composerSlashCommands is unset (v4 ?? true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerSlashCommandsSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('renders the persisted false', async () => {
    const stub = cardStub(settingsRow({ composerSlashCommands: false }));
    const fixture = await mountCard(ComposerSlashCommandsSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerSlashCommandsSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ composerSlashCommands: false }]);
  });

  it("carries v4's copy, the `/` in code", async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerSlashCommandsSettings, stub);
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('.qt-settings-section-heading')?.textContent).toBe('Slash commands');
    const copy = (el.querySelector('.qt-text-small')?.textContent ?? '').replace(/\s+/g, ' ').trim();
    expect(copy).toBe(
      'Type / at the start of a word to run a tool or open a composer action. The gutter buttons and the sidebar are never affected.',
    );
    expect(el.querySelector('.qt-text-small code')?.textContent).toBe('/');
  });

  it('surfaces a failed save instead of reverting silently (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(ComposerSlashCommandsSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update composer slash-commands setting',
    );
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });
});
