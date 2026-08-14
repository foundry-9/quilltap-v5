import { describe, expect, it } from 'vitest';

import { cardStub, checkboxes, mountCard, settingsRow, toggle } from './chat-settings.spec-harness';
import { ComposerEmojiSettings } from './composer-emoji-settings';
import { ComposerUnicodeSettings } from './composer-unicode-settings';

/**
 * The two P4.D75 Composer toggles, following the scalar-toggle card shape: v4's
 * default-when-unset, the exact `chatSettingsUpdate` payload per interaction,
 * the copy that tells a reader the toolbar button is NOT gated, and the
 * dogfood-#6 rule that a failed save surfaces visibly.
 *
 * @module screens/settings/chat/composer-char-insert-cards.spec
 */

describe('ComposerEmojiSettings', () => {
  it('defaults to checked when composerEmoji is unset (v4 ?? true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerEmojiSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('renders the persisted false', async () => {
    const stub = cardStub(settingsRow({ composerEmoji: false }));
    const fixture = await mountCard(ComposerEmojiSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ composerEmoji: true }));
    const fixture = await mountCard(ComposerEmojiSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ composerEmoji: false }]);
  });

  it('says out loud that the toolbar button is not gated (v4 copy)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerEmojiSettings, stub);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Emoji shortcuts');
    expect(text).toContain("The toolbar's emoji button works either way.");
  });

  it('surfaces a failed save instead of reverting silently (dogfood #6)', async () => {
    const stub = cardStub(settingsRow({ composerEmoji: true }), true);
    const fixture = await mountCard(ComposerEmojiSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update emoji shortcut setting',
    );
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });
});

describe('ComposerUnicodeSettings', () => {
  it('defaults to checked when composerUnicode is unset (v4 ?? true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerUnicodeSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('renders the persisted false', async () => {
    const stub = cardStub(settingsRow({ composerUnicode: false }));
    const fixture = await mountCard(ComposerUnicodeSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ composerUnicode: false }));
    const fixture = await mountCard(ComposerUnicodeSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ composerUnicode: true }]);
  });

  it('carries v4`s copy, math bail and Ω button included', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerUnicodeSettings, stub);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Symbol shortcuts');
    expect(text).toContain('Nothing fires inside a formula');
    expect(text).toContain('$$\\phi$$');
    expect(text).toContain('works either way');
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(ComposerUnicodeSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update symbol shortcut setting',
    );
  });
});
