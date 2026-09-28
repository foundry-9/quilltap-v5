import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_CONCIERGE_SETTINGS,
  type ConciergeSettingsUpdate,
} from '../chat/chat-settings.types';
import {
  configureConcierge,
  conciergeStub,
  el,
  mountConcierge,
  settingsRow,
  settle,
  text,
  toggleByHeading,
  change,
  check,
} from './concierge.spec-harness';
import {
  ConciergeSettingsCard,
  effectiveConcierge,
  mergeConciergeUpdate,
} from './concierge-settings.api';
import { DisplayCard } from './display-card';
import { OnDutyCard } from './on-duty-card';
import { PreScreeningCard } from './pre-screening-card';

afterEach(() => TestBed.resetTestingModule());

/**
 * The Concierge save path (v4 `3b463d6b1` `useChatSettings.handleConciergeUpdate`).
 * The server REPLACES `conciergeSettings` whole, so every save must carry the
 * whole object, deep-merged over the LATEST row at send time.
 */
describe('Concierge settings — the defaults and the merge', () => {
  it('DEFAULT_CONCIERGE_SETTINGS is the server default, field by field', () => {
    // v4 `lib/services/dangerous-content/resolver.service.ts:32-53` at `acadcc7cd`.
    expect(DEFAULT_CONCIERGE_SETTINGS).toEqual({
      enabled: true,
      uncensoredTextProfileId: null,
      uncensoredImageProfileId: null,
      uncensoredVisionProfileId: null,
      imagePromptProfileId: null,
      autoSwitchAfterRefusals: 2,
      newChatsStartAs: 'moderated',
      display: { mode: 'SHOW', showWarningBadges: true },
      preScreen: {
        enabled: false,
        threshold: 0.7,
        scanTextChat: true,
        scanImagePrompts: true,
        scanImageGeneration: false,
        customClassificationPrompt: null,
        summaryClassification: false,
      },
    });
  });

  it('the effective object fills a stored object one level down', () => {
    const eff = effectiveConcierge({
      enabled: false,
      display: { mode: 'BLUR' },
      preScreen: { enabled: true },
    } as never);
    expect(eff.enabled).toBe(false);
    expect(eff.display).toEqual({ mode: 'BLUR', showWarningBadges: true });
    expect(eff.preScreen).toEqual({ ...DEFAULT_CONCIERGE_SETTINGS.preScreen, enabled: true });
    expect(eff.autoSwitchAfterRefusals).toBe(2);
    expect(effectiveConcierge(undefined)).toEqual(DEFAULT_CONCIERGE_SETTINGS);
  });

  it('the merge replaces top-level fields and deep-merges display + preScreen', () => {
    const current = { ...DEFAULT_CONCIERGE_SETTINGS, uncensoredTextProfileId: 'p1' };
    const next = mergeConciergeUpdate(current, {
      autoSwitchAfterRefusals: 4,
      preScreen: { threshold: 0.3 },
    });
    expect(next).toEqual({
      ...current,
      autoSwitchAfterRefusals: 4,
      preScreen: { ...current.preScreen, threshold: 0.3 },
    });
    expect(mergeConciergeUpdate(null, { enabled: false })).toEqual({
      ...DEFAULT_CONCIERGE_SETTINGS,
      enabled: false,
    });
  });
});

@Component({
  selector: 'qt-two-concierge-cards',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DisplayCard, PreScreeningCard],
  template: `<qt-concierge-display-card /><qt-concierge-pre-screening-card />`,
})
class TwoCards {}

/** Drives `update()` directly with v4's own argument objects. */
@Component({
  selector: 'qt-concierge-update-probe',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '',
})
class UpdateProbe extends ConciergeSettingsCard {
  run(updates: ConciergeSettingsUpdate): Promise<void> {
    return this.update(updates);
  }
}

describe('Concierge settings — handleConciergeUpdate', () => {
  it('handleConciergeUpdate deep-merges display and preScreen, keeping every other field', async () => {
    // v4 `__tests__/unit/hooks/useChatSettings.test.tsx` (`3b463d6b1`), v4's
    // two calls verbatim through the one write path.
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }));
    const fixture = await mountConcierge(UpdateProbe, stub);
    await fixture.componentInstance.run({ display: { mode: 'BLUR' } });
    await fixture.componentInstance.run({
      preScreen: { enabled: true },
      autoSwitchAfterRefusals: 5,
    });

    const [first, second] = stub.updates.map(
      (u) => u['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS,
    );
    expect(first.display).toEqual({ mode: 'BLUR', showWarningBadges: true });
    expect(first.preScreen.enabled).toBe(false);
    // The second update keeps the first's display change and every preScreen default.
    expect(second.display).toEqual({ mode: 'BLUR', showWarningBadges: true });
    expect(second.preScreen).toMatchObject({ enabled: true, threshold: 0.7, scanTextChat: true });
    expect(second.autoSwitchAfterRefusals).toBe(5);
    expect(second.enabled).toBe(true);
    // Every PUT is the WHOLE object — the server replaces the column.
    expect(Object.keys(second).sort()).toEqual(Object.keys(DEFAULT_CONCIERGE_SETTINGS).sort());
    expect(Object.keys(second.preScreen).sort()).toEqual(
      Object.keys(DEFAULT_CONCIERGE_SETTINGS.preScreen).sort(),
    );
    // And nothing else rides along: the body is `{ conciergeSettings }` alone.
    expect(stub.updates.map((u) => Object.keys(u))).toEqual([
      ['conciergeSettings'],
      ['conciergeSettings'],
    ]);
    // v4 `mutateSettings(updated, false)` — the response seeds the row.
    expect(
      (stub.current()['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS).preScreen.enabled,
    ).toBe(true);
  });

  it("two cards saving one after the other: the second carries the first's change", async () => {
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }));
    const fixture = await mountConcierge(TwoCards, stub);
    await change(fixture, el<HTMLSelectElement>(fixture, '#concierge-display-mode'), 'BLUR');
    await check(fixture, toggleByHeading(fixture, 'Pre-screen before sending'), true);
    const second = stub.updates[1]['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS;
    expect(second.display.mode).toBe('BLUR');
    expect(second.preScreen.enabled).toBe(true);
  });

  it('merges over a stored object that predates a field (the defaults fill it)', async () => {
    const stub = conciergeStub(settingsRow({ conciergeSettings: { enabled: true } }));
    const fixture = await mountConcierge(OnDutyCard, stub);
    await check(fixture, toggleByHeading(fixture, 'The Concierge is on duty'), false);
    expect(stub.updates).toEqual([
      { conciergeSettings: { ...DEFAULT_CONCIERGE_SETTINGS, enabled: false } },
    ]);
  });

  it("two cards saving at once: the second merge sees the first's result (the race arm)", async () => {
    // v5's `saving` is per-card, so two cards CAN save concurrently (v4's one
    // provider-wide flag forbids it). Fire both before either settles.
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }));
    configureConcierge(stub);
    const fixture = TestBed.createComponent(TwoCards);
    fixture.detectChanges();
    await settle(fixture);

    const mode = el<HTMLSelectElement>(fixture, '#concierge-display-mode');
    const summary = toggleByHeading(
      fixture,
      "Read each chat's summary in the background and switch it when it looks dangerous",
    );
    mode.value = 'COLLAPSE';
    mode.dispatchEvent(new Event('change'));
    summary.checked = true;
    summary.dispatchEvent(new Event('change'));
    await settle(fixture);

    expect(stub.updates).toHaveLength(2);
    const second = stub.updates[1]['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS;
    expect(second.display.mode).toBe('COLLAPSE');
    expect(second.preScreen.summaryClassification).toBe(true);
    // The stored row ends with BOTH changes.
    const stored = stub.current()['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS;
    expect(stored.display.mode).toBe('COLLAPSE');
    expect(stored.preScreen.summaryClassification).toBe(true);
  });

  it('a rejected save does not poison the next one (the stored chain link never rejects)', async () => {
    // Added at the round's unification (review NIT): `ChatSettingsCard.save`
    // swallows its own errors today, so the chain is safe only by that
    // accident; a save that DOES reject must not strand every later save.
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }));
    const fixture = await mountConcierge(UpdateProbe, stub);
    const probe = fixture.componentInstance;
    const save = vi
      .spyOn(probe as unknown as { save: (...args: unknown[]) => Promise<void> }, 'save')
      .mockRejectedValueOnce(new Error('boom'));
    await expect(probe.run({ display: { mode: 'BLUR' } })).rejects.toThrow('boom');
    save.mockRestore();
    await probe.run({ autoSwitchAfterRefusals: 4 });
    expect(stub.updates).toHaveLength(1);
    expect(
      (stub.updates[0]['conciergeSettings'] as typeof DEFAULT_CONCIERGE_SETTINGS)
        .autoSwitchAfterRefusals,
    ).toBe(4);
  });

  it("surfaces a failed save with v4's failure message", async () => {
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }), {
      failUpdate: true,
    });
    const fixture = await mountConcierge(OnDutyCard, stub);
    await check(fixture, toggleByHeading(fixture, 'The Concierge is on duty'), false);
    expect(text(el(fixture, 'qt-error-alert'))).toContain(
      "Failed to update the Concierge's settings",
    );
  });
});
