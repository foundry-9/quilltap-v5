import { describe, expect, it } from 'vitest';

import { AnswerConfirmationSettings } from './answer-confirmation-settings';
import { AutoScrollSettings } from './auto-scroll-settings';
import { AutomationSettings } from './automation-settings';
import { cardStub, checkboxes, mountCard, settingsRow, toggle } from './chat-settings.spec-harness';
import { ComposerSpellcheckSettings } from './composer-spellcheck-settings';

/**
 * The four scalar-toggle Chat-tab cards (P4.6an unit 3). Each asserts v4's
 * default-when-unset, the exact `chatSettingsUpdate` payload per interaction,
 * and the dogfood-#6 rule that a failed save surfaces visibly.
 */

describe('ComposerSpellcheckSettings', () => {
  it('defaults to checked when composerSpellcheck is unset (v4 ?? true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ComposerSpellcheckSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('renders the persisted false', async () => {
    const stub = cardStub(settingsRow({ composerSpellcheck: false }));
    const fixture = await mountCard(ComposerSpellcheckSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ composerSpellcheck: true }));
    const fixture = await mountCard(ComposerSpellcheckSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ composerSpellcheck: false }]);
  });

  it('surfaces a failed save instead of reverting silently (dogfood #6)', async () => {
    const stub = cardStub(settingsRow({ composerSpellcheck: true }), true);
    const fixture = await mountCard(ComposerSpellcheckSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Failed to update composer spellcheck setting');
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
  });
});

describe('AutoScrollSettings', () => {
  it('defaults to unchecked when unset (v4 ?? false)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AutoScrollSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('renders the persisted true', async () => {
    const stub = cardStub(settingsRow({ autoScrollOnResponseComplete: true }));
    const fixture = await mountCard(AutoScrollSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ autoScrollOnResponseComplete: false }));
    const fixture = await mountCard(AutoScrollSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ autoScrollOnResponseComplete: true }]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(AutoScrollSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update the auto-scroll setting',
    );
  });
});

describe('AutomationSettings', () => {
  it('defaults to checked when autoDetectRng is unset (v4 DEFAULT_AUTO_DETECT_RNG = true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AutomationSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('renders v4\'s single AUTOMATION_OPTIONS row with its copy', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AutomationSettings, stub);
    expect(checkboxes(fixture)).toHaveLength(1);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Auto-Detect RNG Calls');
  });

  it('PUTs the bare scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ autoDetectRng: true }));
    const fixture = await mountCard(AutomationSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ autoDetectRng: false }]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(AutomationSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update auto-detect RNG setting',
    );
  });
});

describe('AnswerConfirmationSettings', () => {
  it('defaults to unchecked when unset (v4 DEFAULT enabled false)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('renders the persisted enabled', async () => {
    const stub = cardStub(settingsRow({ answerConfirmationSettings: { enabled: true } }));
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('PUTs the WHOLE merged bag, not a nested patch (v4 spread-merge)', async () => {
    const stub = cardStub(settingsRow({ answerConfirmationSettings: { enabled: false } }));
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ answerConfirmationSettings: { enabled: true } }]);
  });

  it('merges over the defaults when the stored bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([{ answerConfirmationSettings: { enabled: true } }]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update answer-confirmation settings',
    );
  });

  /**
   * v4 `0506517d3` correction (f3): the card's hand-built
   * `flex items-start gap-3 p-4 border …` label was replaced by the shared
   * `SettingsToggleRow`, which the composer toggles already used — so the row
   * now wears `qt-settings-toggle-row`, its heading `qt-settings-section-heading`,
   * and its body `qt-text-small mt-1`. v5 adopts the SIBLING markup (six cards
   * already carry it), not v4's React component.
   */
  it('wears the shared qt-settings-toggle-row styling (v4 0506517d3 (f3))', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(AnswerConfirmationSettings, stub);
    const el = fixture.nativeElement as HTMLElement;

    const row = el.querySelector('label.qt-settings-toggle-row');
    expect(row).toBeTruthy();
    // The hand-built label is gone, not merely joined.
    expect(el.querySelector('label.qt-hover-accent')).toBeNull();

    expect(row?.querySelector('input.qt-checkbox.mt-1')).toBeTruthy();
    const heading = row?.querySelector('.qt-settings-section-heading');
    expect(heading?.textContent?.trim()).toBe('Confirm looked-up answers by default');
    const body = row?.querySelector('.qt-text-small.mt-1');
    expect(body?.textContent).toContain('a swift second reader');
  });
});
