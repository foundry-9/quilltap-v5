import type { ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { ELLIPSIS, EN_DASH } from '../../../smart-typography/engine';
import { cardStub, checkboxes, mountCard, settingsRow, settle, toggle } from './chat-settings.spec-harness';
import { SmartTypographySettings } from './smart-typography-settings';

/**
 * The Smart Typography card (v4 `SmartTypographySettings.tsx`, Layer 1.6).
 *
 * Two things are worth pinning beyond the usual read/write pair: the bag is
 * sent WHOLE on every change (v4's merge-then-PUT, so a `dashes` flip cannot
 * drop `displayQuotes`), and the try-it box runs the SHARED engine — if it ever
 * grew its own arithmetic, the preview would start lying about what the
 * composer does.
 */

function textarea(fixture: ComponentFixture<unknown>): HTMLTextAreaElement {
  const el = (fixture.nativeElement as HTMLElement).querySelector('textarea');
  if (!el) throw new Error('the try-it box is missing');
  return el as HTMLTextAreaElement;
}

/** Type `key` at the end of the try-it box, the way a writer would. */
function press(fixture: ComponentFixture<unknown>, key: string): KeyboardEvent {
  const ta = textarea(fixture);
  ta.selectionStart = ta.value.length;
  ta.selectionEnd = ta.value.length;
  const event = new KeyboardEvent('keydown', { key, cancelable: true });
  ta.dispatchEvent(event);
  return event;
}

describe('SmartTypographySettings — the three toggles', () => {
  it('shows v4 defaults when the bag is absent (quotes off, both rules on)', async () => {
    const fixture = await mountCard(SmartTypographySettings, cardStub(settingsRow()));
    const [quotes, dashes, ellipsis] = checkboxes(fixture);
    expect(quotes.checked).toBe(false);
    expect(dashes.checked).toBe(true);
    expect(ellipsis.checked).toBe(true);
  });

  it('reflects each stored value independently', async () => {
    const fixture = await mountCard(
      SmartTypographySettings,
      cardStub(
        settingsRow({
          smartTypographySettings: { displayQuotes: true, dashes: false, ellipsis: true },
        }),
      ),
    );
    const [quotes, dashes, ellipsis] = checkboxes(fixture);
    expect(quotes.checked).toBe(true);
    expect(dashes.checked).toBe(false);
    expect(ellipsis.checked).toBe(true);
  });

  it('sends the WHOLE bag on a change, merged over the defaults and the stored row', async () => {
    const stub = cardStub(settingsRow({ smartTypographySettings: { displayQuotes: true } }));
    const fixture = await mountCard(SmartTypographySettings, stub);
    await toggle(fixture, checkboxes(fixture)[1], false); // dashes off

    // The PUT carries all three keys: v4's merge-then-PUT, so flipping one
    // rule cannot silently drop the setting the other card half owns.
    expect(stub.updates).toEqual([
      { smartTypographySettings: { displayQuotes: true, dashes: false, ellipsis: true } },
    ]);
  });

  it('writes displayQuotes without disturbing the typing rules', async () => {
    const stub = cardStub(settingsRow({ smartTypographySettings: { ellipsis: false } }));
    const fixture = await mountCard(SmartTypographySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([
      { smartTypographySettings: { displayQuotes: true, dashes: true, ellipsis: false } },
    ]);
  });

  it('surfaces a failed save (the dogfood-#6 rule)', async () => {
    const stub = cardStub(settingsRow(), /* failUpdate */ true);
    const fixture = await mountCard(SmartTypographySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update smart typography settings',
    );
  });
});

describe('SmartTypographySettings — the try-it box', () => {
  it('substitutes through the shared engine, and moves the caret with it', async () => {
    const fixture = await mountCard(SmartTypographySettings, cardStub(settingsRow()));
    const ta = textarea(fixture);
    ta.value = 'wait-';

    const event = press(fixture, '-');

    expect(event.defaultPrevented).toBe(true);
    expect(ta.value).toBe(`wait${EN_DASH}`);
    expect(ta.selectionStart).toBe(5);
  });

  it('walks the ladder and the ellipsis exactly as the composer does', async () => {
    const fixture = await mountCard(SmartTypographySettings, cardStub(settingsRow()));
    const ta = textarea(fixture);

    ta.value = `wait${EN_DASH}`;
    press(fixture, '-');
    expect(ta.value).toBe('wait—');

    ta.value = 'Well..';
    press(fixture, '.');
    expect(ta.value).toBe(`Well${ELLIPSIS}`);
  });

  it('leaves an ordinary keystroke alone', async () => {
    const fixture = await mountCard(SmartTypographySettings, cardStub(settingsRow()));
    const ta = textarea(fixture);
    ta.value = 'wait-';
    const event = press(fixture, 'x');
    expect(event.defaultPrevented).toBe(false);
    expect(ta.value).toBe('wait-');
  });

  it('honours the toggles it sits beneath', async () => {
    const fixture = await mountCard(
      SmartTypographySettings,
      cardStub(settingsRow({ smartTypographySettings: { dashes: false } })),
    );
    const ta = textarea(fixture);
    ta.value = 'wait-';
    expect(press(fixture, '-').defaultPrevented).toBe(false);
    expect(ta.value).toBe('wait-');

    // ...while the rule that is still on keeps working.
    ta.value = 'Well..';
    expect(press(fixture, '.').defaultPrevented).toBe(true);
    expect(ta.value).toBe(`Well${ELLIPSIS}`);
  });

  it('says so when both rules are off, and stops substituting', async () => {
    const fixture = await mountCard(
      SmartTypographySettings,
      cardStub(settingsRow({ smartTypographySettings: { dashes: false, ellipsis: false } })),
    );
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Both toggles are off — substitutions are paused.',
    );
    const ta = textarea(fixture);
    ta.value = 'Well..';
    expect(press(fixture, '.').defaultPrevented).toBe(false);
    expect(ta.value).toBe('Well..');
  });

  it('does not say so while either rule is on', async () => {
    const fixture = await mountCard(
      SmartTypographySettings,
      cardStub(settingsRow({ smartTypographySettings: { dashes: false } })),
    );
    await settle(fixture);
    expect((fixture.nativeElement as HTMLElement).textContent).not.toContain(
      'substitutions are paused',
    );
  });
});
